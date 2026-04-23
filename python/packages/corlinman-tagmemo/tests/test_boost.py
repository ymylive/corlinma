"""Boost formula tests."""

from __future__ import annotations

import math

from corlinman_tagmemo import dynamic_boost


def test_boost_clips_to_range() -> None:
    # logic_depth=1, resonance_boost=1 -> factor = 2.0, *base=1 => 2.0
    assert dynamic_boost(1.0, resonance_boost=1.0) == 2.0
    # Extreme base boost should clip to the upper bound.
    assert dynamic_boost(1.0, base_tag_boost=100.0) == 2.5
    # Zero logic depth -> factor 0, clipped to lower bound.
    assert dynamic_boost(0.0) == 0.5


def test_boost_handles_zero_denominator_gracefully() -> None:
    # entropy_penalty = -2 is out of spec; implementation must clamp so we
    # never divide by zero or negative. Resulting boost must be finite.
    result = dynamic_boost(0.8, entropy_penalty=-2.0)
    assert math.isfinite(result)
    # Clamping ep to [0,1] yields denom=1, factor=0.8, clipped to [0.5, 2.5].
    assert result == 0.8


def test_boost_monotonic_in_resonance() -> None:
    lo = dynamic_boost(0.5, resonance_boost=0.0)
    hi = dynamic_boost(0.5, resonance_boost=1.0)
    assert hi >= lo
