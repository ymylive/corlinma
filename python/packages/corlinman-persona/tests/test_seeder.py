"""Tests for :mod:`corlinman_persona.seeder`.

Three core scenarios per the task spec:

1. YAML carries an explicit ``persona:`` block — values seed the row.
2. YAML omits ``persona:`` — defaults seed the row.
3. Row already exists for the agent_id — seeder is a no-op.

Plus malformed-YAML rejection so operators discover typos early.
"""

from __future__ import annotations

from pathlib import Path

import pytest
from corlinman_persona.seeder import PersonaCardError, seed_from_card
from corlinman_persona.state import RECENT_TOPICS_CAP, PersonaState
from corlinman_persona.store import PersonaStore


@pytest.fixture
def db_path(tmp_path: Path) -> Path:
    return tmp_path / "agent_state.sqlite"


def _write_card(path: Path, body: str) -> Path:
    path.write_text(body, encoding="utf-8")
    return path


async def test_seed_with_full_persona_section(db_path: Path, tmp_path: Path) -> None:
    card = _write_card(
        tmp_path / "mentor.yaml",
        """
name: mentor
agent_id: mentor
persona:
  initial_mood: focused
  initial_fatigue: 0.25
  initial_topics:
    - kickoff
    - retro
""",
    )
    async with PersonaStore(db_path) as store:
        created = await seed_from_card(store, card)
        assert created is True
        got = await store.get("mentor")
    assert got is not None
    assert got.mood == "focused"
    assert got.fatigue == pytest.approx(0.25)
    assert got.recent_topics == ["kickoff", "retro"]


async def test_seed_without_persona_section_uses_defaults(
    db_path: Path, tmp_path: Path
) -> None:
    card = _write_card(
        tmp_path / "scribe.yaml",
        """
name: scribe
agent_id: scribe
description: "writes things"
""",
    )
    async with PersonaStore(db_path) as store:
        created = await seed_from_card(store, card)
        assert created is True
        got = await store.get("scribe")
    assert got is not None
    assert got.mood == "neutral"
    assert got.fatigue == pytest.approx(0.0)
    assert got.recent_topics == []
    assert got.state_json == {}


async def test_seed_skips_when_row_already_exists(db_path: Path, tmp_path: Path) -> None:
    card = _write_card(
        tmp_path / "mentor.yaml",
        """
name: mentor
persona:
  initial_mood: focused
  initial_fatigue: 0.5
""",
    )
    async with PersonaStore(db_path) as store:
        # Pre-existing row — seeder must NOT overwrite.
        await store.upsert(
            PersonaState(
                agent_id="mentor",
                mood="tired",
                fatigue=0.9,
                recent_topics=["existing"],
                updated_at_ms=1,
            )
        )
        created = await seed_from_card(store, card)
        assert created is False
        got = await store.get("mentor")
    assert got is not None
    # Existing values preserved verbatim.
    assert got.mood == "tired"
    assert got.fatigue == pytest.approx(0.9)
    assert got.recent_topics == ["existing"]


async def test_agent_id_falls_back_to_filename_stem(
    db_path: Path, tmp_path: Path
) -> None:
    """If the YAML omits both ``agent_id`` and ``name``, the filename
    stem is used (matches the registry's authoritative-stem rule)."""
    card = _write_card(
        tmp_path / "wanderer.yaml",
        """
description: "loose card with no name field"
""",
    )
    async with PersonaStore(db_path) as store:
        created = await seed_from_card(store, card)
        assert created is True
        got = await store.get("wanderer")
    assert got is not None


async def test_seed_rejects_non_mapping_persona(db_path: Path, tmp_path: Path) -> None:
    card = _write_card(
        tmp_path / "broken.yaml",
        """
name: broken
persona:
  - not
  - a
  - mapping
""",
    )
    async with PersonaStore(db_path) as store:
        with pytest.raises(PersonaCardError, match="persona must be a mapping"):
            await seed_from_card(store, card)


async def test_seed_rejects_invalid_fatigue_range(
    db_path: Path, tmp_path: Path
) -> None:
    card = _write_card(
        tmp_path / "bad.yaml",
        """
name: bad
persona:
  initial_fatigue: 1.5
""",
    )
    async with PersonaStore(db_path) as store:
        with pytest.raises(PersonaCardError, match=r"\[0.0, 1.0\]"):
            await seed_from_card(store, card)


async def test_seed_rejects_non_numeric_fatigue(db_path: Path, tmp_path: Path) -> None:
    card = _write_card(
        tmp_path / "bad.yaml",
        """
name: bad
persona:
  initial_fatigue: "lots"
""",
    )
    async with PersonaStore(db_path) as store:
        with pytest.raises(PersonaCardError, match="must be a number"):
            await seed_from_card(store, card)


async def test_seed_rejects_non_string_topic_entry(
    db_path: Path, tmp_path: Path
) -> None:
    card = _write_card(
        tmp_path / "bad.yaml",
        """
name: bad
persona:
  initial_topics:
    - "ok"
    - 123
""",
    )
    async with PersonaStore(db_path) as store:
        with pytest.raises(PersonaCardError, match="entries must be strings"):
            await seed_from_card(store, card)


async def test_seed_rejects_empty_yaml(db_path: Path, tmp_path: Path) -> None:
    card = _write_card(tmp_path / "empty.yaml", "")
    async with PersonaStore(db_path) as store:
        with pytest.raises(PersonaCardError, match="file is empty"):
            await seed_from_card(store, card)


async def test_seed_rejects_yaml_parse_error(db_path: Path, tmp_path: Path) -> None:
    card = _write_card(tmp_path / "bad.yaml", "name: foo\n  : bad indent\n")
    async with PersonaStore(db_path) as store:
        with pytest.raises(PersonaCardError, match="yaml parse error"):
            await seed_from_card(store, card)


async def test_seed_caps_initial_topics(db_path: Path, tmp_path: Path) -> None:
    """A YAML smuggling a 30-entry list must still respect the 20-cap."""
    topics = "\n".join(f"  - t{i}" for i in range(RECENT_TOPICS_CAP + 10))
    card = _write_card(
        tmp_path / "verbose.yaml",
        f"""
name: verbose
persona:
  initial_topics:
{topics}
""",
    )
    async with PersonaStore(db_path) as store:
        await seed_from_card(store, card)
        got = await store.get("verbose")
    assert got is not None
    assert len(got.recent_topics) == RECENT_TOPICS_CAP
    # Most recent kept — last entry must be the highest-indexed one.
    assert got.recent_topics[-1] == f"t{RECENT_TOPICS_CAP + 9}"
