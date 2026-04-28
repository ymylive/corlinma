"""First-sight seeder for ``agent_persona_state`` rows.

The seeder is the **only** writer outside the EvolutionLoop, and even it
is deliberately constrained: it inserts a default-shaped row when an
agent has never been seen before, and it leaves existing rows alone.

YAML loading mirrors :mod:`corlinman_agent.agents.registry` — same
``yaml.safe_load`` + dict-shape contract — but we only need the optional
``persona:`` sub-section, so we parse it inline rather than importing
``corlinman-agent`` (would invert the dependency graph; the agent crate
should be allowed to depend on us, not the other way around).
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

import yaml  # type: ignore[import-untyped]

from corlinman_persona.state import RECENT_TOPICS_CAP, PersonaState
from corlinman_persona.store import PersonaStore


class PersonaCardError(RuntimeError):
    """Raised when an agent-card YAML's ``persona:`` block is unparseable.

    Missing ``persona:`` is fine (defaults apply). A *present-but-malformed*
    block is rejected so operators discover typos at seed time rather
    than at prompt-render time.
    """

    def __init__(self, path: Path, reason: str) -> None:
        super().__init__(f"{path}: {reason}")
        self.path = path
        self.reason = reason


def _read_card(path: Path) -> dict[str, Any]:
    """Parse the YAML body and return the top-level mapping.

    Empty / non-mapping files raise :class:`PersonaCardError` for the
    same operator-debugging reasons the agent registry rejects them.
    """
    try:
        raw = yaml.safe_load(path.read_text(encoding="utf-8"))
    except yaml.YAMLError as exc:
        raise PersonaCardError(path, f"yaml parse error: {exc}") from exc
    if raw is None:
        raise PersonaCardError(path, "file is empty")
    if not isinstance(raw, dict):
        raise PersonaCardError(path, "top-level yaml must be a mapping")
    return raw


def _agent_id_from_card(path: Path, card: dict[str, Any]) -> str:
    """Resolve the agent_id for a card.

    Prefers an explicit ``agent_id:`` key; falls back to ``name:``; falls
    back to the filename stem. Mirrors the registry's "stem is
    authoritative" stance loosely — we don't reject mismatches because
    the seeder is best-effort and the registry already does that
    validation upstream.
    """
    explicit = card.get("agent_id")
    if isinstance(explicit, str) and explicit.strip():
        return explicit
    name = card.get("name")
    if isinstance(name, str) and name.strip():
        return name
    return path.stem


def _parse_persona_section(path: Path, persona: object) -> dict[str, Any]:
    """Validate the optional ``persona:`` sub-mapping.

    Returns a normalised dict with ``initial_mood`` / ``initial_fatigue``
    / ``initial_topics`` keys (any may be missing). A present-but-wrong
    type for a sub-key raises :class:`PersonaCardError`; the absent /
    null section yields an empty dict and the caller falls back to
    :class:`PersonaState` defaults.
    """
    if persona is None:
        return {}
    if not isinstance(persona, dict):
        raise PersonaCardError(path, "persona must be a mapping")
    out: dict[str, Any] = {}
    if "initial_mood" in persona:
        mood = persona["initial_mood"]
        if not isinstance(mood, str) or not mood.strip():
            raise PersonaCardError(path, "persona.initial_mood must be a non-empty string")
        out["initial_mood"] = mood
    if "initial_fatigue" in persona:
        fatigue = persona["initial_fatigue"]
        if not isinstance(fatigue, int | float) or isinstance(fatigue, bool):
            raise PersonaCardError(path, "persona.initial_fatigue must be a number")
        fatigue_f = float(fatigue)
        if not (0.0 <= fatigue_f <= 1.0):
            raise PersonaCardError(path, "persona.initial_fatigue must be in [0.0, 1.0]")
        out["initial_fatigue"] = fatigue_f
    if "initial_topics" in persona:
        topics = persona["initial_topics"]
        if not isinstance(topics, list):
            raise PersonaCardError(path, "persona.initial_topics must be a list of strings")
        normalised: list[str] = []
        for entry in topics:
            if not isinstance(entry, str):
                raise PersonaCardError(path, "persona.initial_topics entries must be strings")
            normalised.append(entry)
        # Cap at write time so a generous YAML doesn't smuggle a 50-entry
        # list past the dataclass invariant.
        out["initial_topics"] = normalised[-RECENT_TOPICS_CAP:]
    return out


async def seed_from_card(store: PersonaStore, card_path: Path) -> bool:
    """Insert a fresh persona row from ``card_path`` if absent.

    Returns ``True`` when a new row was created, ``False`` when the
    agent_id already had state (the existing row is **never** mutated —
    that path goes through the EvolutionLoop's ``agent_card`` kind).

    Raises :class:`PersonaCardError` if the YAML is malformed or carries
    a structurally invalid ``persona:`` block. Missing ``persona:`` is
    explicitly fine and yields a defaults-only row.
    """
    card = _read_card(card_path)
    agent_id = _agent_id_from_card(card_path, card)

    existing = await store.get(agent_id)
    if existing is not None:
        return False

    persona = _parse_persona_section(card_path, card.get("persona"))

    state = PersonaState(
        agent_id=agent_id,
        mood=persona.get("initial_mood", "neutral"),
        fatigue=persona.get("initial_fatigue", 0.0),
        recent_topics=list(persona.get("initial_topics", [])),
        # ``upsert`` will fill updated_at with "now" because we pass 0.
        updated_at_ms=0,
        state_json={},
    )
    await store.upsert(state)
    return True


__all__ = ["PersonaCardError", "seed_from_card"]
