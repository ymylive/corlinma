"""Integrated context-assembly pipeline.

Runs the full five-stage preprocessing over a raw OpenAI-style message
list before the reasoning loop hands it to the provider::

    input: list[dict] messages, session_key, model_name
    pipeline:
      1. agent cards expansion   (AgentExpander on system / system-inject-gated)
      2. cascade var substitution (fixed / tar / var / sar via VariableCascade)
      3. skill context injection  (expanded_agent.skill_refs → system prompt)
      4. placeholder pass        (remaining {{namespace.*}} via Rust UDS)
      5. emit hook               (``message.preprocessed``)
    output: AssembledContext

Privilege gating
----------------

The same **privileged-message** rule from the B2-BE3 agent expander is
used throughout: ``role == "system"`` messages plus ``role == "user"``
messages whose content begins with a system-injection marker
(``[系统提示:]`` / ``[系统邀请指令:]``). Stages 1 and 3 mutate only
privileged messages. Stage 2 (cascade-var substitution) runs on **every**
string message — it matches the classic behavior of applying cascade
vars to user content too, and the pattern ``{{<bare-key>}}`` is narrow
enough that accidental collisions with user prose are rare. Stage 4
(placeholder engine render) runs only on privileged messages because the
``{{namespace.*}}`` token surface is powerful (resolvers can call out to
tools / vectors / skills) and must not be fired by arbitrary user input.

Return value
------------

:class:`AssembledContext` aggregates the pipeline's observations so
callers can surface them downstream (logs, metrics, admin UI):

* ``expanded_agent`` / ``muted_agents`` — from stage 1.
* ``unresolved_keys`` — the union of bare cascade keys left literal in
  stage 2 *plus* the namespaced keys the Rust engine reported as
  unresolved in stage 4.
* ``skill_errors`` — non-fatal ``check_requirements`` failures from
  stage 3; the skill's body is omitted but the pipeline continues.
* ``metadata`` — pass-through of the caller-supplied metadata for the
  reasoning loop to thread into downstream tooling.
"""

from __future__ import annotations

import re
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass, field
from typing import Any

import structlog

from corlinman_agent.agents import AgentCardRegistry, AgentExpander, ExpansionResult
from corlinman_agent.hooks import HookEmitter
from corlinman_agent.placeholder_client import PlaceholderClient, PlaceholderError
from corlinman_agent.skills import SkillRegistry
from corlinman_agent.variables import VariableCascade

logger = structlog.get_logger(__name__)

# System-injection inline markers. When either prefix appears at the
# start of a user-role ``content`` string the downstream agent treats
# the whole turn as a system instruction; we therefore expand placeholders
# in it even though the role is ``"user"``.
_SYSTEM_INJECT_PREFIXES: tuple[str, ...] = ("[系统提示:]", "[系统邀请指令:]")

# Bare cascade-key form: ``{{Name}}`` with no dot. The namespaced dotted
# forms (``{{var.x}}``, ``{{session.y}}``, ``{{tool.z}}``) are the
# placeholder engine's turf and are deliberately *not* matched here.
# Leading character must be a letter; underscores and digits are allowed
# after the first position.
_BARE_KEY_RE = re.compile(r"\{\{([A-Za-z][A-Za-z0-9_]*)\}\}")


def has_system_inject_prefix(content: str) -> bool:
    """Return ``True`` when ``content`` begins with any system-injection marker."""
    stripped = content.lstrip()
    return any(stripped.startswith(p) for p in _SYSTEM_INJECT_PREFIXES)


def _is_privileged(message: Mapping[str, Any]) -> bool:
    """System-role turns, plus user turns that carry a system-inject marker.

    Mirrors the privilege predicate used by :class:`AgentExpander` so the
    pipeline's stages agree on which messages are prompt templates and
    which are user input.
    """
    role = message.get("role")
    if role == "system":
        return True
    if role == "user":
        content = message.get("content")
        if isinstance(content, str) and has_system_inject_prefix(content):
            return True
    return False


@dataclass
class AssembledContext:
    """Result of :meth:`ContextAssembler.assemble`.

    Attributes
    ----------
    messages
        The fully-processed OpenAI-style chat messages. Each entry is a
        fresh ``dict`` (not an alias of the caller's input) so downstream
        tweaks don't leak back.
    expanded_agent
        Name of the agent that won the single-agent gate in stage 1, or
        ``None`` if no agent placeholder fired.
    muted_agents
        Agent names that were silenced by the gate, in encounter order.
    unresolved_keys
        Union of cascade keys unresolved in stage 2 and namespaced keys
        the placeholder engine reported as unresolved in stage 4. First
        occurrence order is preserved; duplicates are dropped.
    skill_errors
        Human-readable problem strings from skills whose
        ``check_requirements`` failed in stage 3. Non-fatal: the skill's
        body is not injected but the pipeline continues.
    metadata
        Pass-through of the caller-supplied metadata map (or ``{}`` when
        none was given). Useful so callers can chain the returned
        context into downstream stages without re-plumbing the map.
    """

    messages: list[dict[str, Any]]
    expanded_agent: str | None = None
    muted_agents: list[str] = field(default_factory=list)
    unresolved_keys: list[str] = field(default_factory=list)
    skill_errors: list[str] = field(default_factory=list)
    metadata: dict[str, str] = field(default_factory=dict)


class ContextAssembler:
    """Runs the full pre-provider pipeline.

    All resolver dependencies are injected: the assembler itself is
    pure-ish glue, so one instance is safe to share across concurrent
    sessions. The only per-call state lives on the local
    :class:`_State` dataclass inside :meth:`assemble`.
    """

    def __init__(
        self,
        *,
        agents: AgentCardRegistry,
        variables: VariableCascade,
        skills: SkillRegistry,
        placeholder_client: PlaceholderClient,
        hook_emitter: HookEmitter,
        config_lookup: Callable[[str], str | None],
        single_agent_gate: bool = True,
    ) -> None:
        self._agent_expander = AgentExpander(agents, single_agent_gate=single_agent_gate)
        self._variables = variables
        self._skills = skills
        self._placeholder = placeholder_client
        self._hook = hook_emitter
        self._config_lookup = config_lookup

    # ------------------------------------------------------------------ API

    async def assemble(
        self,
        messages: Sequence[Mapping[str, Any]],
        *,
        session_key: str,
        model_name: str,
        metadata: Mapping[str, str] | None = None,
    ) -> AssembledContext:
        """Run the five-stage pipeline over ``messages`` and return a
        :class:`AssembledContext`.

        The pipeline is fail-soft by design: stage 3 skill-requirement
        failures go into ``skill_errors`` rather than raising, and stage
        4 keeps propagating :class:`PlaceholderError` because a failed
        template render genuinely corrupts the prompt. Stage 2 leaves
        unresolved bare keys literal so the placeholder engine or a
        downstream resolver has another chance to handle them.
        """
        md: dict[str, str] = dict(metadata or {})

        # --- Stage 1: agent-card expansion ------------------------------
        expansion: ExpansionResult = self._agent_expander.expand(messages)
        msgs: list[dict[str, Any]] = expansion.expanded_messages

        # --- Stage 2: cascade-var substitution --------------------------
        unresolved: list[str] = []
        seen_unresolved: set[str] = set()
        for msg in msgs:
            content = msg.get("content")
            if not isinstance(content, str) or not content:
                continue
            new_content, unresolved_here = self._apply_cascade(content, model_name)
            if new_content is not content:
                msg["content"] = new_content
            for key in unresolved_here:
                if key not in seen_unresolved:
                    seen_unresolved.add(key)
                    unresolved.append(key)

        # --- Stage 3: skill injection -----------------------------------
        skill_errors: list[str] = []
        if expansion.expanded_agent is not None:
            card = self._agent_expander._registry.get(expansion.expanded_agent)
            if card is not None and card.skill_refs:
                self._inject_skills(msgs, card.skill_refs, skill_errors)

        # --- Stage 4: placeholder pass (system-only) --------------------
        for msg in msgs:
            if not _is_privileged(msg):
                continue
            content = msg.get("content")
            if not isinstance(content, str) or not content:
                continue
            try:
                result = await self._placeholder.render(
                    template=content,
                    session_key=session_key,
                    model_name=model_name,
                    metadata=md,
                )
            except PlaceholderError as exc:
                logger.error(
                    "context_assembler.render_failed",
                    role=msg.get("role"),
                    session_key=session_key,
                    error=str(exc),
                )
                raise
            msg["content"] = result.rendered
            for key in result.unresolved_keys:
                if key not in seen_unresolved:
                    seen_unresolved.add(key)
                    unresolved.append(key)

        # --- Stage 5: emit preprocessed hook ----------------------------
        self._emit_preprocessed(msgs, session_key=session_key, metadata=md)

        if unresolved:
            logger.debug(
                "context_assembler.unresolved",
                session_key=session_key,
                keys=unresolved,
            )

        return AssembledContext(
            messages=msgs,
            expanded_agent=expansion.expanded_agent,
            muted_agents=list(expansion.muted_agents),
            unresolved_keys=unresolved,
            skill_errors=skill_errors,
            metadata=md,
        )

    # ------------------------------------------------------------------ helpers

    def _apply_cascade(self, content: str, model_name: str) -> tuple[str, list[str]]:
        """Substitute bare ``{{Key}}`` tokens via :class:`VariableCascade`.

        Returns ``(new_content, unresolved_keys)``. Unresolved keys are
        the tokens the cascade returned ``None`` for — they stay literal
        in the output so stage 4 can have a crack at them (e.g. a
        future ``UnknownVar`` registration on the Rust side).

        Tokens where the cascade returned the empty string (a legitimate
        "gated off" Sar answer) are treated as resolved.
        """
        unresolved: list[str] = []

        def _sub(match: re.Match[str]) -> str:
            key = match.group(1)
            value = self._variables.resolve(key, model_name)
            if value is None:
                if key not in unresolved:
                    unresolved.append(key)
                return match.group(0)
            return value

        new_content = _BARE_KEY_RE.sub(_sub, content)
        return new_content, unresolved

    def _inject_skills(
        self,
        messages: list[dict[str, Any]],
        skill_refs: Sequence[str],
        skill_errors: list[str],
    ) -> None:
        """Prepend the body of each referenced skill to the first system
        message, gated by :meth:`SkillRegistry.check_requirements`.

        If no system-role message exists yet, we create one at position
        ``0``. Each injected section is fenced with a ``## Skill: <name>``
        heading so the model can distinguish injected skill bodies from
        the authored system prompt.
        """
        injections: list[str] = []
        for name in skill_refs:
            skill = self._skills.get(name)
            if skill is None:
                skill_errors.append(f"skill '{name}' is not registered")
                continue
            problems = self._skills.check_requirements(name, self._config_lookup)
            if problems:
                skill_errors.extend(problems)
                continue
            injections.append(f"## Skill: {skill.name}\n\n{skill.body_markdown}")

        if not injections:
            return

        section = "\n\n".join(injections)

        # Find the first system-role message; create one if absent.
        sys_idx: int | None = None
        for i, msg in enumerate(messages):
            if msg.get("role") == "system":
                sys_idx = i
                break

        if sys_idx is None:
            messages.insert(0, {"role": "system", "content": section})
            return

        existing = messages[sys_idx].get("content")
        if isinstance(existing, str) and existing:
            messages[sys_idx]["content"] = f"{section}\n\n{existing}"
        else:
            # Non-string (multimodal) or empty — don't try to splice.
            # Insert a dedicated system turn right before so the skill
            # body still reaches the provider in the right position.
            messages.insert(sys_idx, {"role": "system", "content": section})

    def _emit_preprocessed(
        self,
        messages: Sequence[Mapping[str, Any]],
        *,
        session_key: str,
        metadata: Mapping[str, str],
    ) -> None:
        """Fire the ``message.preprocessed`` lifecycle hook.

        The payload matches the Rust plugin-bus schema so the eventual
        gRPC wiring can forward it without translation.
        """
        first_user_text = ""
        for msg in messages:
            if msg.get("role") == "user":
                content = msg.get("content")
                if isinstance(content, str):
                    first_user_text = content
                break

        payload = {
            "session_key": session_key,
            "transcript": first_user_text[:500],
            "is_group": metadata.get("is_group") == "true",
            "group_id": metadata.get("group_id"),
        }
        self._hook.emit("message.preprocessed", payload)


__all__ = [
    "AssembledContext",
    "ContextAssembler",
    "has_system_inject_prefix",
]
