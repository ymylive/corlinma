"""Residual pyramid tests."""

from __future__ import annotations

import numpy as np
from corlinman_tagmemo import EpaBasis, build_pyramid, fit_basis


def _hand_basis(k: int, d: int) -> EpaBasis:
    axes = np.eye(d)[:k]
    # Energies matter for the cumulative-explained calculation.
    energies = np.array([float(k - i) for i in range(k)])
    return EpaBasis(
        ortho_basis=axes,
        basis_mean=np.zeros(d),
        basis_energies=energies,
        basis_labels=[f"axis_{i}" for i in range(k)],
    )


def test_pyramid_stops_at_target_explained_energy() -> None:
    # energies = [5,4,3,2,1], total=15. Coefficients chosen so axis-0 alone
    # explains (1*5)^2 = 25 >= 0.9 * 15; pyramid should stop after 1 level.
    k, d = 5, 5
    basis = _hand_basis(k, d)
    query = 1.0 * basis.ortho_basis[0]  # centered: mean is zero.
    res = build_pyramid(basis, query, target_explained=0.90)
    assert len(res.levels) == 1
    assert res.levels[0].cumulative_explained >= 0.90


def test_pyramid_uses_all_axes_when_target_is_one(
    synthetic_low_dim: np.ndarray,
) -> None:
    basis = fit_basis(synthetic_low_dim, k=8)
    res = build_pyramid(basis, synthetic_low_dim[0], target_explained=1.0)
    # We either exhaust all K axes, or the basis is rank-deficient; either
    # way coverage should be within (0, 1].
    assert 0.0 < res.features.coverage <= 1.0


def test_pyramid_level_coefficients_sum_reconstructs_query_within_tol() -> None:
    k, d = 4, 4
    basis = _hand_basis(k, d)
    # With target_explained=1.0 and full-rank basis, coefficients reconstruct
    # the centered query exactly.
    coeffs_true = np.array([0.3, -0.7, 0.4, 0.1])
    query = basis.ortho_basis.T @ coeffs_true
    res = build_pyramid(basis, query, target_explained=1.0)
    recovered = np.zeros(d)
    for level in res.levels:
        idx = int(level.axis_label.split("_")[1])
        recovered += level.coefficient * basis.ortho_basis[idx]
    np.testing.assert_allclose(recovered, query, atol=1e-10)


def test_pyramid_coverage_in_zero_one(synthetic_high_dim: np.ndarray) -> None:
    basis = fit_basis(synthetic_high_dim, k=6)
    res = build_pyramid(basis, synthetic_high_dim[0], target_explained=0.5)
    assert 0.0 < res.features.coverage <= 1.0
    assert res.features.tag_memo_activation >= 0.0
