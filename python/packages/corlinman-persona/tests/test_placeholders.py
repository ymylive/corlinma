"""Tests for :mod:`corlinman_persona.placeholders`.

Covers each of the documented placeholder keys, the empty-string
fallbacks for unknown agents / keys, and the ``state_json`` extension
key path.
"""

from __future__ import annotations

from pathlib import Path

import pytest
from corlinman_persona.placeholders import PersonaResolver
from corlinman_persona.state import PersonaState
from corlinman_persona.store import PersonaStore


@pytest.fixture
def db_path(tmp_path: Path) -> Path:
    return tmp_path / "agent_state.sqlite"


async def test_resolve_mood(db_path: Path) -> None:
    async with PersonaStore(db_path) as store:
        await store.upsert(
            PersonaState(agent_id="mentor", mood="curious", updated_at_ms=1)
        )
        resolver = PersonaResolver(store)
        assert await resolver.resolve("mood", "mentor") == "curious"


async def test_resolve_fatigue_buckets(db_path: Path) -> None:
    """Each fatigue bucket maps to its label, with no float leaking."""
    async with PersonaStore(db_path) as store:
        resolver = PersonaResolver(store)

        cases = [
            (0.0, "rested"),
            (0.1, "rested"),
            (0.15, "fresh"),
            (0.3, "fresh"),
            (0.4, "mild fatigue"),
            (0.6, "mild fatigue"),
            (0.75, "tired"),
            (0.95, "tired"),
        ]
        for fatigue, label in cases:
            await store.upsert(
                PersonaState(agent_id="probe", fatigue=fatigue, updated_at_ms=1)
            )
            assert await resolver.resolve("fatigue", "probe") == label


async def test_resolve_recent_topics_newest_first_capped_at_five(db_path: Path) -> None:
    async with PersonaStore(db_path) as store:
        await store.upsert(
            PersonaState(
                agent_id="mentor",
                # Stored oldest-first; resolver renders newest-first.
                recent_topics=["t1", "t2", "t3", "t4", "t5", "t6", "t7"],
                updated_at_ms=1,
            )
        )
        resolver = PersonaResolver(store)
        out = await resolver.resolve("recent_topics", "mentor")
    # Last 5 (t3..t7), reversed → t7, t6, t5, t4, t3.
    assert out == "t7, t6, t5, t4, t3"


async def test_resolve_recent_topics_empty_yields_empty_string(
    db_path: Path,
) -> None:
    async with PersonaStore(db_path) as store:
        await store.upsert(
            PersonaState(agent_id="mentor", recent_topics=[], updated_at_ms=1)
        )
        resolver = PersonaResolver(store)
        assert await resolver.resolve("recent_topics", "mentor") == ""


async def test_resolve_custom_state_json_key(db_path: Path) -> None:
    async with PersonaStore(db_path) as store:
        await store.upsert(
            PersonaState(
                agent_id="mentor",
                updated_at_ms=1,
                state_json={"trust_in_user": 0.83, "tone": "warm"},
            )
        )
        resolver = PersonaResolver(store)
        assert await resolver.resolve("trust_in_user", "mentor") == "0.83"
        assert await resolver.resolve("tone", "mentor") == "warm"


async def test_resolve_unknown_agent_yields_empty_string(db_path: Path) -> None:
    """Unknown agent must not crash prompt rendering."""
    async with PersonaStore(db_path) as store:
        resolver = PersonaResolver(store)
        for key in ("mood", "fatigue", "recent_topics", "trust_in_user"):
            assert await resolver.resolve(key, "nobody") == ""


async def test_resolve_unknown_custom_key_yields_empty_string(db_path: Path) -> None:
    """Missing key in ``state_json`` must not raise."""
    async with PersonaStore(db_path) as store:
        await store.upsert(
            PersonaState(agent_id="mentor", updated_at_ms=1, state_json={"k": "v"})
        )
        resolver = PersonaResolver(store)
        assert await resolver.resolve("missing_key", "mentor") == ""
