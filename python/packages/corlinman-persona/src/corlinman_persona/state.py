"""PersonaState dataclass — the in-memory projection of one
``agent_persona_state`` row.

Mutations on the state happen through :class:`~corlinman_persona.store.PersonaStore`
or pure functions in :mod:`~corlinman_persona.decay`. The dataclass itself
is intentionally lightweight; it carries no I/O.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

# Hard cap on ``recent_topics`` length. The roadmap asks for "last 20" and
# we enforce it at every write site rather than relying on callers.
RECENT_TOPICS_CAP: int = 20


@dataclass
class PersonaState:
    """Single agent's persisted runtime state.

    Attributes
    ----------
    agent_id
        Stable agent identifier (matches ``agents/<name>.yaml`` stem).
    mood
        Free-form mood label. The decay layer treats a small set of
        well-known values (``"tired"``, ``"neutral"``) but does not
        constrain the vocabulary.
    fatigue
        ``[0.0, 1.0]`` — 0 means well-rested, 1 means maxed-out tired.
        Recovers over time via :func:`~corlinman_persona.decay.apply_decay`.
    recent_topics
        Most-recent-last list of topic strings, capped at
        :data:`RECENT_TOPICS_CAP`. Same topic appearing again moves to the
        end (de-duplication keeps the newest position).
    updated_at_ms
        Unix milliseconds — last write time, used by the decay job to
        compute elapsed hours.
    state_json
        Free-form extension dict serialised as JSON in the row. Used by
        ``{{persona.<custom>}}`` lookups for fields the schema doesn't
        promote to columns.
    """

    agent_id: str
    mood: str = "neutral"
    fatigue: float = 0.0
    recent_topics: list[str] = field(default_factory=list)
    updated_at_ms: int = 0
    state_json: dict[str, Any] = field(default_factory=dict)


__all__ = ["RECENT_TOPICS_CAP", "PersonaState"]
