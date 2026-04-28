"""``{{persona.*}}`` placeholder resolver.

We expose the resolver class only — wiring it into the Rust
``context_assembler`` placeholder engine is intentionally deferred (W3-B
took the same posture for ``{{user.*}}``). Keeping the surface narrow
means unit tests don't have to spin up the assembler, and the assembler
can adopt the resolver behind any IPC shape it prefers later.

Resolution rules (mirrors the W3-C task spec):

- ``{{persona.mood}}`` — raw mood string.
- ``{{persona.fatigue}}`` — categorical bucket label (``"rested"`` /
  ``"fresh"`` / ``"mild fatigue"`` / ``"tired"``). We deliberately do
  not surface the float so the prompt doesn't carry the implementation
  detail of the [0.0, 1.0] range.
- ``{{persona.recent_topics}}`` — comma-separated string of the 5
  most-recent topics. Empty if none.
- ``{{persona.<custom>}}`` — looked up in ``state_json``; missing keys
  resolve to ``""`` rather than raising, so a typo in a prompt template
  doesn't kill the whole render.

Unknown ``agent_id`` always resolves to ``""`` for the same reason —
prompt rendering is a hot path and noisy errors there are worse than
empty placeholders.
"""

from __future__ import annotations

from corlinman_persona.state import PersonaState
from corlinman_persona.store import PersonaStore

# Number of topics surfaced to the prompt. The store retains up to 20;
# the prompt only ever sees the freshest 5 to keep token usage bounded.
RECENT_TOPICS_VISIBLE: int = 5

# Fatigue bucket boundaries — categorical so the prompt isn't shaped by
# a number that's only meaningful internally. Inclusive lower bounds.
_FATIGUE_BUCKETS: tuple[tuple[float, str], ...] = (
    (0.75, "tired"),
    (0.4, "mild fatigue"),
    (0.15, "fresh"),
    (0.0, "rested"),
)


def _bucket_fatigue(value: float) -> str:
    """Return the categorical label for ``value``.

    Boundaries are inclusive on the lower side — ``0.4`` itself reads as
    ``"mild fatigue"``, not ``"fresh"``. Out-of-range values clamp to
    the nearest bucket; we never raise on bad input here because this
    runs during prompt expansion.
    """
    clamped = max(0.0, min(1.0, value))
    for threshold, label in _FATIGUE_BUCKETS:
        if clamped >= threshold:
            return label
    # Unreachable while the table covers >= 0.0, but keeps the type
    # checker happy without an `else: assert False`.
    return "rested"


def _format_topics(topics: list[str]) -> str:
    """Comma-join the freshest ``RECENT_TOPICS_VISIBLE`` topics.

    Topics are stored oldest-first; we take the tail and reverse so the
    prompt sees newest-first (``"deploys, retries, latency"``). Empty
    list produces empty string, not ``"[]"`` or ``"None"``.
    """
    if not topics:
        return ""
    tail = topics[-RECENT_TOPICS_VISIBLE:]
    tail_newest_first = list(reversed(tail))
    return ", ".join(tail_newest_first)


class PersonaResolver:
    """Read-only resolver for ``{{persona.*}}`` placeholder keys.

    Holds a reference to a :class:`PersonaStore` and answers one lookup
    at a time. The resolver does not cache — callers that want
    per-render memoisation should wrap us; we'd rather keep this layer
    obviously consistent with the DB.
    """

    def __init__(self, store: PersonaStore) -> None:
        self._store = store

    async def resolve(self, key: str, agent_id: str) -> str:
        """Return the placeholder value for ``key`` against ``agent_id``.

        ``key`` is the suffix after ``persona.`` (e.g. ``"mood"``,
        ``"recent_topics"``, or any custom key for ``state_json``).
        Unknown keys / agents always return ``""`` — see the module
        docstring for the rationale.
        """
        state = await self._store.get(agent_id)
        if state is None:
            return ""
        return _resolve_against_state(state, key)


def _resolve_against_state(state: PersonaState, key: str) -> str:
    """Pure helper so tests can exercise the lookup table without sqlite.

    Splitting it out also makes the resolver trivially mockable — a
    cached layer can call this without re-implementing the dispatch.
    """
    if key == "mood":
        return state.mood
    if key == "fatigue":
        return _bucket_fatigue(state.fatigue)
    if key == "recent_topics":
        return _format_topics(state.recent_topics)
    # ``state_json`` extension keys. ``str(value)`` keeps us type-safe
    # while still surfacing ints / floats / bools the operator stuck in
    # there. Missing keys fall through to the empty string.
    raw = state.state_json.get(key)
    if raw is None:
        return ""
    return str(raw)


__all__ = ["RECENT_TOPICS_VISIBLE", "PersonaResolver"]
