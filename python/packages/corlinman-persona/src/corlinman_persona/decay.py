"""Pure-function persona decay.

The decay job is deterministic and additive — given a ``PersonaState``
plus a wall-clock delta, it returns a new state. No I/O, no randomness.
The CLI wires this up to the actual store; tests can drive it directly
with a synthetic clock.

We don't decay the ``mood`` string itself — it's a categorical label.
Instead we let ``fatigue`` recover and use a single threshold rule to
flip a ``"tired"`` mood back to ``"neutral"`` once the agent is rested.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, replace

from corlinman_persona.state import PersonaState


@dataclass(frozen=True)
class DecayConfig:
    """Tunables for :func:`apply_decay`. All defaults match the values
    in ``docs/design/phase3-roadmap.md`` §6 ``[persona]``.
    """

    # Recovery rate for fatigue, per hour of elapsed wall time.
    fatigue_recovery_per_hour: float = 0.1
    # Threshold below which a ``"tired"`` mood auto-flips to ``"neutral"``.
    # The number is intentionally generous (0.3) so the agent recovers
    # before fatigue hits 0 — mood is a coarser signal than the float.
    tired_to_neutral_below: float = 0.3
    # Number of recent_topics dropped per full day elapsed. The roadmap
    # asks for "1 per day" — we keep it as an int knob for clarity.
    recent_topics_decay_per_day: int = 1
    # Reserved for future use (mood numerics). Wired into :class:`DecayConfig`
    # now so the cron-job TOML can carry it without a schema bump later.
    mood_decay_per_hour: float = 0.05


def apply_decay(
    state: PersonaState,
    hours_elapsed: float,
    config: DecayConfig,
) -> PersonaState:
    """Return a new ``PersonaState`` with decay applied.

    Pure function — does not mutate the input. Negative or zero
    ``hours_elapsed`` is a no-op (the caller may pass timestamps that
    haven't advanced; we don't want to "advance into the past").

    Rules (mirrors roadmap §5):
      - ``fatigue``: ``max(0.0, fatigue - hours_elapsed * recovery_per_hour)``.
      - ``mood``: if it was ``"tired"`` and the new fatigue dropped
        below :attr:`DecayConfig.tired_to_neutral_below`, flip to
        ``"neutral"``. Other mood labels are left alone.
      - ``recent_topics``: drop ``floor(hours_elapsed / 24) *
        recent_topics_decay_per_day`` of the oldest entries. Drop is
        clamped at the list length (over-aged states bottom out at empty).
      - ``updated_at_ms`` is left to the store layer; this function does
        not invent timestamps.
    """
    if hours_elapsed <= 0:
        return state

    new_fatigue = max(0.0, state.fatigue - hours_elapsed * config.fatigue_recovery_per_hour)

    new_mood = state.mood
    if state.mood == "tired" and new_fatigue < config.tired_to_neutral_below:
        new_mood = "neutral"

    days_elapsed = math.floor(hours_elapsed / 24.0)
    drop_count = days_elapsed * config.recent_topics_decay_per_day
    if drop_count <= 0:
        new_topics = list(state.recent_topics)
    elif drop_count >= len(state.recent_topics):
        new_topics = []
    else:
        # Oldest entries live at the head of the list (push_recent_topic
        # appends to the tail), so slicing from ``drop_count:`` ages them
        # out from the front.
        new_topics = list(state.recent_topics[drop_count:])

    return replace(
        state,
        mood=new_mood,
        fatigue=new_fatigue,
        recent_topics=new_topics,
    )


__all__ = ["DecayConfig", "apply_decay"]
