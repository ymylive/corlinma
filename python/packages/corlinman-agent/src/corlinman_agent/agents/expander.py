"""Character-card / ``{{角色}}`` expansion over a message list.

Semantics:

* Expansion only runs on **privileged** messages: ``role == "system"``
  turns, plus user-role turns whose content begins with a system-inject
  marker (``[系统提示:]`` / ``[系统邀请指令:]``).
* Two placeholder forms are recognised:

  - ``{{agent.<name>}}`` — always an agent reference.
  - ``{{<name>}}`` (legacy, bare) — only treated as an agent reference
    when ``<name>`` is registered. Unknown bare tokens are passed
    through untouched so the Rust placeholder engine can resolve them
    (variables, skills, dynamic tokens).

* A configurable **single-agent gate** — on by default — lets the
  first resolved agent claim the conversation: any subsequent agent
  references anywhere in the same message sequence are replaced with
  an empty string and logged as muted.
* Circular references (``agent.A`` -> ``agent.B`` -> ``agent.A``) are
  caught during recursive expansion and raise
  :class:`AgentCircularReferenceError` with the cycle path attached.

The expander is pure: given a registry and a message list, it returns
a new list. There is no I/O, no registry mutation, and no shared
state across ``expand`` calls — each invocation starts with fresh
gate / stack bookkeeping, so expansion is safe to call concurrently
from multiple reasoning loops.
"""

from __future__ import annotations

import logging
import re
from collections.abc import Mapping, Sequence
from dataclasses import dataclass, field
from typing import Any

from corlinman_agent.agents.registry import AgentCardRegistry

logger = logging.getLogger(__name__)

# System-inject prefix markers — copied from context_assembler so the
# agent expander can run as a pre-step before the placeholder bridge
# without re-importing that module. Keep these in sync if the markers
# are ever extended.
_SYSTEM_INJECT_PREFIXES: tuple[str, ...] = ("[系统提示:]", "[系统邀请指令:]")

# Token regexes. Anchored to ``{{...}}`` blocks with no whitespace
# between the braces and the name — avoids colliding with Jinja /
# Mustache fragments that users may put in prompts via `[raw]` style.
_NAME = r"[A-Za-z0-9_\-\u4e00-\u9fff]+"  # ascii + CJK; agent names are often Chinese.
_AGENT_NS_RE = re.compile(r"\{\{\s*agent\.(" + _NAME + r")\s*\}\}")
# Bare token regex deliberately rejects any '.' inside the token so it
# never eats `{{var.x}}`, `{{skill.y}}`, `{{sys.time}}` — those are the
# Rust engine's turf.
_BARE_RE = re.compile(r"\{\{\s*(" + _NAME + r")\s*\}\}")
# Local-variable references consumed by the card's own ``variables:``
# dict. We namespace them under ``var.`` so bare ``{{expertise}}``
# stays reserved for the bare-agent-token form above.
_VAR_NS_RE = re.compile(r"\{\{\s*var\.(" + _NAME + r")\s*\}\}")


class AgentCircularReferenceError(RuntimeError):
    """Raised when agent expansion detects a cycle.

    ``cycle`` is the ordered list of agent names visited, with the
    repeated name appended at the end so the loop is obvious in logs:
    e.g. ``["a", "b", "a"]`` means ``a -> b -> a``.
    """

    def __init__(self, cycle: list[str]) -> None:
        super().__init__("agent expansion cycle detected: " + " -> ".join(cycle))
        self.cycle = cycle


@dataclass
class ExpansionResult:
    """Return value of :meth:`AgentExpander.expand`.

    Attributes
    ----------
    expanded_messages
        Shallow copies of the input messages with privileged turns'
        ``content`` rewritten. Non-privileged messages are pass-through
        copies (same role/content, new dict).
    expanded_agent
        Name of the first agent that won the single-agent gate, or
        ``None`` if no agent token fired. Under ``single_agent_gate=False``
        this is set to the *first* agent resolved, purely for logging.
    muted_agents
        Names of agent references that were silenced by the gate, in
        encounter order. Empty when the gate is off or only one agent
        was referenced.
    """

    expanded_messages: list[dict[str, Any]]
    expanded_agent: str | None = None
    muted_agents: list[str] = field(default_factory=list)


def _message_is_privileged(message: Mapping[str, Any]) -> bool:
    """See module docstring for the rule. Non-string content short-
    circuits to ``False`` — we cannot regex-rewrite a list/None body."""
    role = message.get("role")
    if role == "system":
        content = message.get("content")
        return isinstance(content, str)
    if role == "user":
        content = message.get("content")
        if not isinstance(content, str):
            return False
        stripped = content.lstrip()
        return any(stripped.startswith(p) for p in _SYSTEM_INJECT_PREFIXES)
    return False


def _apply_local_vars(template: str, variables: Mapping[str, str]) -> str:
    """Substitute ``{{var.KEY}}`` tokens from the card's own variables.

    Scoped to one card — we do **not** traverse the cascade here; that
    is B2-BE4's job. Unknown ``var.*`` tokens are left literal so the
    downstream Rust render pass can try to resolve them against the
    cascade-aware resolver.
    """
    if not variables:
        return template

    def _sub(match: re.Match[str]) -> str:
        key = match.group(1)
        return variables.get(key, match.group(0))

    return _VAR_NS_RE.sub(_sub, template)


class AgentExpander:
    """Pure function wrapped in a class for injection ergonomics.

    The registry and gate flag are captured at construction time;
    ``expand`` builds fresh per-call state so one expander instance can
    be shared across concurrent sessions.
    """

    def __init__(
        self,
        registry: AgentCardRegistry,
        single_agent_gate: bool = True,
    ) -> None:
        self._registry = registry
        self._single_agent_gate = single_agent_gate

    # ------------------------------------------------------------------ #
    # public API                                                         #
    # ------------------------------------------------------------------ #

    def expand(self, messages: Sequence[Mapping[str, Any]]) -> ExpansionResult:
        """Walk ``messages``, expanding privileged turns.

        Returns a new list — the input is never mutated. Every output
        dict is a shallow copy, so callers can safely overwrite ``content``
        on the result without affecting the caller's originals.
        """
        gate_state = _GateState(single_agent_gate=self._single_agent_gate)
        out: list[dict[str, Any]] = []

        for original in messages:
            copy = dict(original)
            if not _message_is_privileged(original):
                out.append(copy)
                continue

            content = copy["content"]  # guaranteed str by _message_is_privileged
            copy["content"] = self._expand_text(content, gate_state)
            out.append(copy)

        return ExpansionResult(
            expanded_messages=out,
            expanded_agent=gate_state.first_expanded,
            muted_agents=list(gate_state.muted),
        )

    # ------------------------------------------------------------------ #
    # internals                                                          #
    # ------------------------------------------------------------------ #

    def _expand_text(self, text: str, gate: _GateState) -> str:
        """Expand agent placeholders in ``text`` using the gate state.

        Two regex passes are performed:

        1. Namespaced ``{{agent.NAME}}`` — always consumed.
        2. Bare ``{{NAME}}`` — consumed only when NAME is a registered
           agent. Other bare tokens are preserved verbatim so the
           downstream placeholder engine can handle them.

        Both passes go through :meth:`_resolve_one`, which handles the
        gate and the recursive-expansion/cycle check.
        """

        def _sub_namespaced(match: re.Match[str]) -> str:
            name = match.group(1)
            return self._resolve_one(name, gate, stack=[])

        text = _AGENT_NS_RE.sub(_sub_namespaced, text)

        def _sub_bare(match: re.Match[str]) -> str:
            name = match.group(1)
            if self._registry.get(name) is None:
                # Unknown bare token — not an agent, leave for the Rust
                # engine / downstream resolver.
                return match.group(0)
            return self._resolve_one(name, gate, stack=[])

        text = _BARE_RE.sub(_sub_bare, text)
        return text

    def _resolve_one(
        self,
        name: str,
        gate: _GateState,
        stack: list[str],
    ) -> str:
        """Resolve a single agent reference, honouring the gate and
        detecting cycles.

        * Unknown agent ->  literal passthrough (``{{agent.NAME}}``).
        * Gate is already claimed by a different agent -> empty string,
          recorded in ``gate.muted``.
        * Otherwise -> recursively expand the card's ``system_prompt``
          (after local-variable substitution) and return it.
        """
        card = self._registry.get(name)
        if card is None:
            # Preserve the original namespaced form — the bare form is
            # impossible here because the bare regex path filters by
            # membership before calling us.
            return "{{agent." + name + "}}"

        if self._single_agent_gate:
            if gate.first_expanded is None:
                gate.first_expanded = name
            elif gate.first_expanded != name:
                gate.muted.append(name)
                logger.info(
                    "agent_expander.muted",
                    extra={"agent": name, "muted_by": gate.first_expanded},
                )
                return ""
        else:
            # Still track the first resolution for observability.
            if gate.first_expanded is None:
                gate.first_expanded = name

        if name in stack:
            raise AgentCircularReferenceError(cycle=[*stack, name])

        next_stack = [*stack, name]
        body = _apply_local_vars(card.system_prompt, card.variables)

        # Recurse: the body may itself contain {{agent.X}} or bare
        # {{X}} references. We only expand agent-namespaced tokens here
        # (the bare-token legacy form is a top-level convenience that
        # we do NOT propagate into nested prompts — doing so would make
        # accidental string collisions with variable names catastrophic).
        def _sub(match: re.Match[str]) -> str:
            inner = match.group(1)
            return self._resolve_one(inner, gate, next_stack)

        logger.debug(
            "agent_expander.expanded",
            extra={"agent": name, "depth": len(next_stack)},
        )
        return _AGENT_NS_RE.sub(_sub, body)


@dataclass
class _GateState:
    """Per-``expand()``-call mutable state.

    Not part of the public API; kept private so callers can't
    accidentally reuse stale state across invocations.
    """

    single_agent_gate: bool
    first_expanded: str | None = None
    muted: list[str] = field(default_factory=list)


__all__ = [
    "AgentCircularReferenceError",
    "AgentExpander",
    "ExpansionResult",
]
