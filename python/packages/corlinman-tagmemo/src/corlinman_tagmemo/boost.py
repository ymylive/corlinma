"""Dynamic boost formula for tag-memo activation."""

from __future__ import annotations

import numpy as np


def dynamic_boost(
    logic_depth: float,
    resonance_boost: float = 0.0,
    entropy_penalty: float = 0.0,
    base_tag_boost: float = 1.0,
    boost_range: tuple[float, float] = (0.5, 2.5),
) -> float:
    """Combine logic depth + external signals into a single multiplicative boost.

    Inputs are clamped to their expected ranges so pathological callers
    (e.g. `entropy_penalty = -2`) cannot produce a division by zero or NaN.
    """
    ld = float(np.clip(logic_depth, 0.0, 1.0))
    rb = float(np.clip(resonance_boost, 0.0, 1.0))
    ep = float(np.clip(entropy_penalty, 0.0, 1.0))

    denom = 1.0 + ep * 0.5  # ep in [0,1] => denom in [1, 1.5], never zero.
    factor = ld * (1.0 + rb) / denom
    lo, hi = boost_range
    return float(np.clip(base_tag_boost * factor, lo, hi))
