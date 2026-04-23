"""Shared fixtures for tagmemo tests."""

from __future__ import annotations

import numpy as np
import pytest


@pytest.fixture
def rng() -> np.random.Generator:
    return np.random.default_rng(seed=123)


@pytest.fixture
def synthetic_low_dim(rng: np.random.Generator) -> np.ndarray:
    # 200 samples in d=8, mix of a few gaussian clusters.
    centers = rng.normal(size=(5, 8)) * 3.0
    samples = np.vstack(
        [center + rng.normal(scale=0.5, size=(40, 8)) for center in centers]
    )
    return samples


@pytest.fixture
def synthetic_high_dim(rng: np.random.Generator) -> np.ndarray:
    centers = rng.normal(size=(6, 256)) * 2.0
    samples = np.vstack(
        [center + rng.normal(scale=0.4, size=(30, 256)) for center in centers]
    )
    return samples
