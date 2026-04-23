"""Tests for the ``{{角色}}`` agent-card expansion layer.

Covers yaml load, privilege gating, single-agent gate, circular-
reference detection, and the legacy bare-token form.
No gRPC / network — the expander is pure Python.
"""

from __future__ import annotations

from pathlib import Path

import pytest
from corlinman_agent.agents import (
    AgentCardRegistry,
    AgentCircularReferenceError,
    AgentExpander,
)


def _write_card(root: Path, name: str, body: str) -> Path:
    """Helper: dump a yaml string to ``<root>/<name>.yaml`` and return
    the path. The stem is authoritative for the card's name (see
    :mod:`corlinman_agent.agents.registry`), so every test picks a
    stem that matches the body's declared name."""
    path = root / f"{name}.yaml"
    path.write_text(body, encoding="utf-8")
    return path


def _mentor_yaml(name: str = "mentor") -> str:
    """Standard mentor card body used across most tests."""
    return f"""\
name: {name}
description: Thoughtful senior developer mentor
system_prompt: |
  You are a senior developer with 15 years of experience.
  You favor simple, testable code over clever abstractions.
variables:
  expertise: software
  years: 15
tools_allowed:
  - web.search
  - file.read
skill_refs:
  - code_review
"""


# --------------------------------------------------------------------- #
# 1. yaml loading                                                        #
# --------------------------------------------------------------------- #


def test_load_agent_from_yaml(tmp_path: Path) -> None:
    _write_card(tmp_path, "mentor", _mentor_yaml())
    reg = AgentCardRegistry.load_from_dir(tmp_path)
    card = reg.get("mentor")

    assert card is not None
    assert card.name == "mentor"
    assert card.description == "Thoughtful senior developer mentor"
    assert "senior developer" in card.system_prompt
    # yaml may parse "15" as an int; loader must stringify for consistency.
    assert card.variables == {"expertise": "software", "years": "15"}
    assert card.tools_allowed == ["web.search", "file.read"]
    assert card.skill_refs == ["code_review"]
    assert reg.names() == ["mentor"]
    assert "mentor" in reg


# --------------------------------------------------------------------- #
# 2. unknown agent                                                       #
# --------------------------------------------------------------------- #


def test_unknown_agent_is_left_literal(tmp_path: Path) -> None:
    reg = AgentCardRegistry.load_from_dir(tmp_path)  # empty dir
    expander = AgentExpander(reg)
    messages = [{"role": "system", "content": "hi {{agent.nonexistent}} there"}]

    result = expander.expand(messages)

    assert result.expanded_messages[0]["content"] == "hi {{agent.nonexistent}} there"
    assert result.expanded_agent is None
    assert result.muted_agents == []


# --------------------------------------------------------------------- #
# 3. expansion in a system-role turn                                     #
# --------------------------------------------------------------------- #


def test_agent_expanded_in_system_role(tmp_path: Path) -> None:
    _write_card(tmp_path, "mentor", _mentor_yaml())
    reg = AgentCardRegistry.load_from_dir(tmp_path)
    expander = AgentExpander(reg)

    messages = [{"role": "system", "content": "prelude\n{{agent.mentor}}\npostlude"}]
    result = expander.expand(messages)

    body = result.expanded_messages[0]["content"]
    assert "senior developer" in body
    assert "{{agent.mentor}}" not in body
    assert body.startswith("prelude\n")
    assert body.endswith("postlude")
    assert result.expanded_agent == "mentor"


# --------------------------------------------------------------------- #
# 4. not expanded in a regular user message                              #
# --------------------------------------------------------------------- #


def test_agent_not_expanded_in_regular_user_message(tmp_path: Path) -> None:
    _write_card(tmp_path, "mentor", _mentor_yaml())
    reg = AgentCardRegistry.load_from_dir(tmp_path)
    expander = AgentExpander(reg)

    original_content = "please consult {{agent.mentor}} about this"
    messages = [{"role": "user", "content": original_content}]
    result = expander.expand(messages)

    # Non-privileged: passes through untouched.
    assert result.expanded_messages[0]["content"] == original_content
    assert result.expanded_agent is None
    assert result.muted_agents == []


# --------------------------------------------------------------------- #
# 5. System-inject prefix on a user turn IS privileged                   #
# --------------------------------------------------------------------- #


def test_agent_expanded_in_system_inject_prefix_user_message(tmp_path: Path) -> None:
    _write_card(tmp_path, "mentor", _mentor_yaml())
    reg = AgentCardRegistry.load_from_dir(tmp_path)
    expander = AgentExpander(reg)

    messages = [
        {
            "role": "user",
            "content": "[系统提示:] invoke {{agent.mentor}} now",
        }
    ]
    result = expander.expand(messages)

    body = result.expanded_messages[0]["content"]
    assert "senior developer" in body
    assert body.startswith("[系统提示:] invoke ")
    assert result.expanded_agent == "mentor"

    # The sibling marker must also work.
    messages2 = [
        {
            "role": "user",
            "content": "[系统邀请指令:] {{agent.mentor}} take the stage",
        }
    ]
    result2 = expander.expand(messages2)
    assert "senior developer" in result2.expanded_messages[0]["content"]
    assert result2.expanded_agent == "mentor"


# --------------------------------------------------------------------- #
# 6. single-agent gate silences later references                         #
# --------------------------------------------------------------------- #


def test_single_agent_gate_silences_later_references(tmp_path: Path) -> None:
    _write_card(
        tmp_path,
        "a",
        """\
name: a
description: agent A
system_prompt: |
  I AM AGENT A.
""",
    )
    _write_card(
        tmp_path,
        "b",
        """\
name: b
description: agent B
system_prompt: |
  I AM AGENT B.
""",
    )
    reg = AgentCardRegistry.load_from_dir(tmp_path)
    expander = AgentExpander(reg, single_agent_gate=True)

    messages = [
        {
            "role": "system",
            "content": "first {{agent.a}} then {{agent.b}} end",
        }
    ]
    result = expander.expand(messages)

    body = result.expanded_messages[0]["content"]
    assert "I AM AGENT A." in body
    assert "I AM AGENT B." not in body
    # B's placeholder collapsed to empty string, leaving surrounding text.
    assert "then  end" in body
    assert result.expanded_agent == "a"
    assert result.muted_agents == ["b"]


def test_single_agent_gate_silences_across_multiple_system_messages(tmp_path: Path) -> None:
    """Gate state spans the whole message sequence, not a single turn."""
    _write_card(
        tmp_path,
        "a",
        "name: a\ndescription: A\nsystem_prompt: |\n  I AM A.\n",
    )
    _write_card(
        tmp_path,
        "b",
        "name: b\ndescription: B\nsystem_prompt: |\n  I AM B.\n",
    )
    reg = AgentCardRegistry.load_from_dir(tmp_path)
    expander = AgentExpander(reg, single_agent_gate=True)

    messages = [
        {"role": "system", "content": "turn1 {{agent.a}}"},
        {"role": "system", "content": "turn2 {{agent.b}}"},
    ]
    result = expander.expand(messages)

    assert "I AM A." in result.expanded_messages[0]["content"]
    assert "I AM B." not in result.expanded_messages[1]["content"]
    assert result.expanded_messages[1]["content"].strip() == "turn2"
    assert result.muted_agents == ["b"]


# --------------------------------------------------------------------- #
# 7. single-agent gate disabled                                          #
# --------------------------------------------------------------------- #


def test_single_agent_gate_disabled_allows_multiple(tmp_path: Path) -> None:
    _write_card(
        tmp_path,
        "a",
        "name: a\ndescription: A\nsystem_prompt: |\n  I AM A.\n",
    )
    _write_card(
        tmp_path,
        "b",
        "name: b\ndescription: B\nsystem_prompt: |\n  I AM B.\n",
    )
    reg = AgentCardRegistry.load_from_dir(tmp_path)
    expander = AgentExpander(reg, single_agent_gate=False)

    messages = [
        {"role": "system", "content": "{{agent.a}} :: {{agent.b}}"},
    ]
    result = expander.expand(messages)

    body = result.expanded_messages[0]["content"]
    assert "I AM A." in body
    assert "I AM B." in body
    assert result.muted_agents == []


# --------------------------------------------------------------------- #
# 8. circular reference                                                  #
# --------------------------------------------------------------------- #


def test_circular_reference_raises(tmp_path: Path) -> None:
    _write_card(
        tmp_path,
        "a",
        """\
name: a
description: A
system_prompt: |
  A calls {{agent.b}} here.
""",
    )
    _write_card(
        tmp_path,
        "b",
        """\
name: b
description: B
system_prompt: |
  B calls {{agent.a}} here.
""",
    )
    reg = AgentCardRegistry.load_from_dir(tmp_path)
    # Gate OFF — otherwise the second reference is silenced before the
    # cycle can fire, which would hide the bug we're defending against.
    expander = AgentExpander(reg, single_agent_gate=False)

    with pytest.raises(AgentCircularReferenceError) as excinfo:
        expander.expand([{"role": "system", "content": "{{agent.a}}"}])

    cycle = excinfo.value.cycle
    assert cycle[0] == "a"
    assert cycle[-1] == cycle[0] or "a" in cycle
    # Deterministic shape: a -> b -> a.
    assert cycle == ["a", "b", "a"]


# --------------------------------------------------------------------- #
# 9. legacy bare-token form                                              #
# --------------------------------------------------------------------- #


def test_legacy_bare_token_is_agent_name(tmp_path: Path) -> None:
    _write_card(tmp_path, "mentor", _mentor_yaml())
    reg = AgentCardRegistry.load_from_dir(tmp_path)
    expander = AgentExpander(reg)

    messages = [{"role": "system", "content": "intro {{mentor}} outro"}]
    result = expander.expand(messages)

    body = result.expanded_messages[0]["content"]
    assert "senior developer" in body
    assert "{{mentor}}" not in body
    assert result.expanded_agent == "mentor"


def test_unknown_bare_token_is_preserved(tmp_path: Path) -> None:
    """Bare tokens that are *not* registered agents must pass through —
    they belong to the Rust placeholder engine."""
    _write_card(tmp_path, "mentor", _mentor_yaml())
    reg = AgentCardRegistry.load_from_dir(tmp_path)
    expander = AgentExpander(reg)

    messages = [
        {
            "role": "system",
            "content": "keep {{something_else}} and {{var.x}} but expand {{mentor}}",
        }
    ]
    result = expander.expand(messages)

    body = result.expanded_messages[0]["content"]
    assert "{{something_else}}" in body
    assert "{{var.x}}" in body
    assert "senior developer" in body
