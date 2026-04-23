"""Failure-path and depth-semantics tests for :class:`ContextAssembler`.

These live alongside :mod:`test_context_assembler` but are kept in a
separate module because they lean on the public classes directly
(``AgentCircularReferenceError``, skill-requirement failures, depth of
the bare-key substitution pass) rather than on the golden-snapshot
harness. Goldens are for "what does the pipeline produce" — this file
is for "what does the pipeline *refuse* to produce or quietly survive".
"""

from __future__ import annotations

from pathlib import Path

import pytest
from corlinman_agent.agents import (
    AgentCardRegistry,
    AgentCircularReferenceError,
)
from corlinman_agent.context_assembler import ContextAssembler
from corlinman_agent.hooks import RecordingHookEmitter
from corlinman_agent.placeholder_client import RenderResult
from corlinman_agent.skills import SkillRegistry
from corlinman_agent.variables import VariableCascade

# --------------------------------------------------------------------------- #
# Minimal stubs — kept local so this file is independent of the goldens suite. #
# --------------------------------------------------------------------------- #


class _StubPlaceholderClient:
    """No-op placeholder stub that returns the template untouched.

    Stage 4 calls this for every privileged message; the edge tests here
    do not care about namespaced-token behaviour, so the stub just echoes
    the template and reports no unresolved keys.
    """

    def __init__(self) -> None:
        self.calls: list[str] = []

    async def render(
        self,
        *,
        template: str,
        session_key: str,
        model_name: str = "",
        metadata=None,
        max_depth: int = 0,
    ) -> RenderResult:
        self.calls.append(template)
        return RenderResult(rendered=template, unresolved_keys=[])


def _make_cascade(tmp_path: Path) -> VariableCascade:
    tar = tmp_path / "tar"
    var = tmp_path / "var"
    sar = tmp_path / "sar"
    fixed = tmp_path / "fixed"
    for d in (tar, var, sar, fixed):
        d.mkdir(parents=True, exist_ok=True)
    return VariableCascade(tar, var, sar, fixed, hot_reload=False)


def _write(dir_: Path, filename: str, body: str) -> None:
    (dir_ / filename).write_text(body, encoding="utf-8")


def _make_assembler(
    tmp_path: Path,
    *,
    agents_body: dict[str, str] | None = None,
    skills: list[tuple[str, str]] | None = None,
    cascade: VariableCascade | None = None,
    single_agent_gate: bool = True,
) -> ContextAssembler:
    agents_dir = tmp_path / "agents"
    skills_dir = tmp_path / "skills"
    agents_dir.mkdir(parents=True, exist_ok=True)
    skills_dir.mkdir(parents=True, exist_ok=True)
    for name, body in (agents_body or {}).items():
        _write(agents_dir, f"{name}.yaml", body)
    for filename, body in skills or []:
        _write(skills_dir, filename, body)

    ag = AgentCardRegistry.load_from_dir(agents_dir)
    sk = SkillRegistry.load_from_dir(skills_dir)
    vc = cascade if cascade is not None else _make_cascade(tmp_path)
    ph = _StubPlaceholderClient()
    hk = RecordingHookEmitter()

    return ContextAssembler(
        agents=ag,
        variables=vc,
        skills=sk,
        placeholder_client=ph,  # type: ignore[arg-type]
        hook_emitter=hk,
        config_lookup=lambda _k: None,
        single_agent_gate=single_agent_gate,
    )


# --------------------------------------------------------------------------- #
# 1. Agent A <-> B cycle raises                                                #
# --------------------------------------------------------------------------- #


@pytest.mark.asyncio
async def test_circular_agent_reference_raises(tmp_path: Path) -> None:
    """``agent.A`` -> ``agent.B`` -> ``agent.A`` must surface as a
    :class:`AgentCircularReferenceError` whose ``cycle`` list captures
    the full path including the repeated name.

    The single-agent gate is disabled so nested references actually
    recurse: with the gate on, B would be muted by A before its body is
    ever examined, masking the cycle.
    """
    assembler = _make_assembler(
        tmp_path,
        agents_body={
            "A": (
                "name: A\n"
                "description: loops into B\n"
                "system_prompt: |\n"
                "  A body references {{agent.B}}\n"
            ),
            "B": (
                "name: B\n"
                "description: loops back to A\n"
                "system_prompt: |\n"
                "  B body references {{agent.A}}\n"
            ),
        },
        single_agent_gate=False,
    )

    messages = [{"role": "system", "content": "{{agent.A}}"}]

    with pytest.raises(AgentCircularReferenceError) as exc_info:
        await assembler.assemble(messages, session_key="s", model_name="gpt")

    cycle = exc_info.value.cycle
    # Cycle path: A -> B -> A (repeated tail signals the loop).
    assert cycle == ["A", "B", "A"]


# --------------------------------------------------------------------------- #
# 2. Skill with a missing binary is recorded, pipeline continues               #
# --------------------------------------------------------------------------- #


@pytest.mark.asyncio
async def test_skill_with_missing_bin_records_error_but_continues(
    tmp_path: Path,
) -> None:
    """A skill whose ``requires.bins`` points at a binary not on
    ``$PATH`` must be skipped, not raise. ``skill_errors`` captures the
    operator-facing message; the rest of the pipeline keeps running and
    the final system prompt contains neither the skill body nor any
    hint that injection was attempted.
    """
    assembler = _make_assembler(
        tmp_path,
        agents_body={
            "worker": (
                "name: worker\n"
                "description: wants a broken skill\n"
                "system_prompt: |\n"
                "  Worker authored body.\n"
                "skill_refs:\n"
                "  - broken_skill\n"
            ),
        },
        skills=[
            (
                "broken_skill.md",
                "---\n"
                "name: broken_skill\n"
                "description: requires a binary that will never exist\n"
                "metadata:\n"
                "  openclaw:\n"
                "    requires:\n"
                "      bins:\n"
                "        - nonexistent_xyz_123\n"
                "---\n"
                "This body should never reach the prompt.\n",
            )
        ],
    )

    messages = [{"role": "system", "content": "{{agent.worker}}"}]
    result = await assembler.assemble(
        messages, session_key="s", model_name="gpt"
    )

    body = result.messages[0]["content"]
    assert "This body should never reach the prompt." not in body
    assert "## Skill: broken_skill" not in body
    # Authored body still makes it through — the broken skill is the
    # only thing elided.
    assert "Worker authored body." in body
    # The error message names the missing binary so operators can act.
    assert any("nonexistent_xyz_123" in msg for msg in result.skill_errors)


# --------------------------------------------------------------------------- #
# 3. Depth semantics of the bare-key cascade pass                              #
# --------------------------------------------------------------------------- #


@pytest.mark.asyncio
async def test_multiple_placeholder_passes_respect_max_depth(
    tmp_path: Path,
) -> None:
    """Document the *current* depth behaviour of stage 2.

    Stage 2 uses a single ``re.sub`` pass over the content: if a fixed
    resolver returns text that itself contains a bare ``{{Key}}``
    placeholder, that placeholder is **not** re-scanned in the same
    ``assemble()`` call. The token stays literal — it is not recorded
    as unresolved, because the outer regex never matched it.

    A would-be cycle ``{{A}} -> "{{B}}"`` and ``{{B}} -> "{{A}}"`` is
    therefore incapable of looping: stage 2 substitutes whichever key
    was in the original content once and stops. The test pins both
    behaviours so a future multi-pass implementation has to reason
    about what it is changing, and so the absence-of-hang guarantee
    survives refactors.

    TODO(B3): if/when stage 2 grows recursive expansion, update this
    test — the ``Wrapped`` case should produce ``"outer inner end"`` and
    the cycle case should land ``"A"`` (or ``"B"``) in
    ``unresolved_keys`` with an explicit depth-exceeded log line.
    """
    cascade = _make_cascade(tmp_path)
    cascade.register_fixed("Wrapped", lambda: "outer {{Inner}} end")
    cascade.register_fixed("Inner", lambda: "inner")
    cascade.register_fixed("A", lambda: "{{B}}")
    cascade.register_fixed("B", lambda: "{{A}}")
    assembler = _make_assembler(tmp_path, cascade=cascade)

    # Wrapped case: single-pass, so the inner token stays literal.
    messages = [{"role": "system", "content": "value={{Wrapped}}"}]
    result = await assembler.assemble(
        messages, session_key="s", model_name="gpt"
    )
    assert result.messages[0]["content"] == "value=outer {{Inner}} end"
    # The outer {{Wrapped}} resolved so it is NOT unresolved; the inner
    # {{Inner}} was never matched by stage 2 so it is likewise absent.
    # Stage 4's stub reports no unresolved namespaced keys either.
    assert result.unresolved_keys == []

    # Cycle case: stage 2 substitutes once and exits — no hang, no raise.
    messages = [{"role": "system", "content": "cycle={{A}}"}]
    result = await assembler.assemble(
        messages, session_key="s", model_name="gpt"
    )
    assert result.messages[0]["content"] == "cycle={{B}}"
    assert result.unresolved_keys == []
