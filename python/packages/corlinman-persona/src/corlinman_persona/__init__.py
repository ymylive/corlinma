"""Persona persistence — runtime agent state across sessions.

See :class:`PersonaStore` for the SQLite-backed store, :class:`PersonaState`
for the row dataclass, :func:`apply_decay` for the pure-function decay
helper used by the hourly cron job, :func:`seed_from_card` for the
first-sight YAML seeder, and :class:`PersonaResolver` for the read-only
``{{persona.*}}`` placeholder lookup used at prompt-render time.
"""

from __future__ import annotations

from corlinman_persona.decay import DecayConfig, apply_decay
from corlinman_persona.placeholders import PersonaResolver
from corlinman_persona.seeder import seed_from_card
from corlinman_persona.state import PersonaState
from corlinman_persona.store import PersonaStore

__all__ = [
    "DecayConfig",
    "PersonaResolver",
    "PersonaState",
    "PersonaStore",
    "apply_decay",
    "seed_from_card",
]
