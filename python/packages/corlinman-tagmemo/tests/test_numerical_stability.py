"""Numerical-stability edge cases for fit_basis and project."""

from __future__ import annotations

import math

import numpy as np
import pytest
from corlinman_tagmemo import build_pyramid, fit_basis, project


def test_duplicate_vectors_do_not_produce_nan(
    rng: np.random.Generator,
) -> None:
    # 50 identical vectors: every cluster ends up at the same centroid,
    # all SVD singular values should be zero, no NaNs.
    vec = rng.normal(size=(8,))
    vectors = np.tile(vec, (50, 1))
    basis = fit_basis(vectors, k=8)
    assert not np.any(np.isnan(basis.ortho_basis))
    assert not np.any(np.isnan(basis.basis_energies))
    assert float(np.sum(basis.basis_energies)) < 1e-10


def test_zero_weights_raise_clearly(rng: np.random.Generator) -> None:
    vectors = rng.normal(size=(10, 8))
    weights = np.zeros(10)
    with pytest.raises(ValueError):
        fit_basis(vectors, weights=weights, k=4)


def test_negative_weights_rejected(rng: np.random.Generator) -> None:
    vectors = rng.normal(size=(10, 8))
    weights = np.ones(10)
    weights[0] = -1.0
    with pytest.raises(ValueError):
        fit_basis(vectors, weights=weights, k=4)


def test_extreme_projection_magnitudes_do_not_produce_nan(
    synthetic_high_dim: np.ndarray,
) -> None:
    basis = fit_basis(synthetic_high_dim, k=6)
    huge = 1e150 * basis.ortho_basis[0] + basis.basis_mean
    proj = project(basis, huge)
    assert not np.any(np.isnan(proj.probabilities))
    assert math.isfinite(proj.entropy)
    assert math.isfinite(proj.logic_depth)
    # Softmax must still sum to 1 even with overflow-prone scores.
    assert abs(float(np.sum(proj.probabilities)) - 1.0) < 1e-10


def test_pyramid_on_zero_query_behaves(synthetic_low_dim: np.ndarray) -> None:
    basis = fit_basis(synthetic_low_dim, k=8)
    # Query equal to basis_mean => residual == 0 => every coefficient ~0.
    res = build_pyramid(basis, basis.basis_mean, target_explained=0.9)
    for level in res.levels:
        assert abs(level.coefficient) < 1e-10
    assert res.features.tag_memo_activation >= 0.0
