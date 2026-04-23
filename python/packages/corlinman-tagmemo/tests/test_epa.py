"""EPA basis + projection tests."""

from __future__ import annotations

import numpy as np
import pytest
from corlinman_tagmemo import EpaBasis, fit_basis, project


def test_fit_basis_orthonormal_axes(synthetic_low_dim: np.ndarray) -> None:
    basis = fit_basis(synthetic_low_dim, k=8)
    gram = basis.ortho_basis @ basis.ortho_basis.T
    np.testing.assert_allclose(
        gram, np.eye(gram.shape[0]), atol=1e-5
    )


def test_fit_basis_energies_descending(synthetic_high_dim: np.ndarray) -> None:
    basis = fit_basis(synthetic_high_dim, k=6)
    diffs = np.diff(basis.basis_energies)
    assert np.all(diffs <= 1e-10), f"energies not descending: {basis.basis_energies}"
    assert np.all(basis.basis_energies >= 0.0)


def test_project_unit_axis_returns_unit_coefficient(
    synthetic_low_dim: np.ndarray,
) -> None:
    basis = fit_basis(synthetic_low_dim, k=8)
    axis0 = basis.ortho_basis[0]
    # Query = basis_mean + axis0, so centered == axis0, so projection == 1 on
    # axis 0 and ~0 elsewhere.
    query = basis.basis_mean + axis0
    proj = project(basis, query)
    assert abs(proj.projections[0] - 1.0) < 1e-8
    for i in range(1, basis.ortho_basis.shape[0]):
        assert abs(proj.projections[i]) < 1e-8


def test_labels_override_length_mismatch_raises(
    synthetic_low_dim: np.ndarray,
) -> None:
    with pytest.raises(ValueError):
        fit_basis(synthetic_low_dim, k=8, labels=["only", "two"])


def _make_basis(k: int, d: int) -> EpaBasis:
    # Deterministic hand-built basis for entropy math.
    axes = np.eye(d)[:k]
    energies = np.array([float(k - i) for i in range(k)])
    return EpaBasis(
        ortho_basis=axes,
        basis_mean=np.zeros(d),
        basis_energies=energies,
        basis_labels=[f"axis_{i}" for i in range(k)],
    )


def test_entropy_uniform_distribution_is_one() -> None:
    # Equal energies + equal |projections| => uniform softmax => entropy 1.
    k, d = 6, 6
    basis = EpaBasis(
        ortho_basis=np.eye(d)[:k],
        basis_mean=np.zeros(d),
        basis_energies=np.ones(k),
        basis_labels=[f"a{i}" for i in range(k)],
    )
    # Centered query projecting equally onto every axis.
    query = basis.ortho_basis.sum(axis=0)
    proj = project(basis, query)
    np.testing.assert_allclose(proj.probabilities, np.full(k, 1.0 / k), atol=1e-10)
    assert abs(proj.entropy - 1.0) < 1e-10


def test_entropy_delta_distribution_is_zero() -> None:
    # One axis dominates by 1e6 scale -> softmax is ~delta -> entropy ~ 0.
    k, d = 5, 5
    basis = _make_basis(k, d)
    # Project mostly onto axis 0.
    query = 1e6 * basis.ortho_basis[0]
    proj = project(basis, query)
    assert proj.entropy < 1e-6


def test_logic_depth_is_one_minus_entropy(synthetic_low_dim: np.ndarray) -> None:
    basis = fit_basis(synthetic_low_dim, k=8)
    query = synthetic_low_dim[0]
    proj = project(basis, query)
    assert abs(proj.logic_depth - (1.0 - proj.entropy)) < 1e-12


def test_dominant_axes_sorted_by_absolute_contribution() -> None:
    k, d = 5, 5
    basis = _make_basis(k, d)
    # Construct coefficients: projections=[0.1, 0.2, 0.05, 0.4, 0.0],
    # energies=[5,4,3,2,1] -> |contrib|=[0.5,0.8,0.15,0.8,0]. Top-3 picks
    # indices 1 and 3 (tied), then 0.
    coeffs = np.array([0.1, 0.2, 0.05, 0.4, 0.0])
    query = basis.ortho_basis.T @ coeffs  # centered query
    proj = project(basis, query)
    contribs = [abs(d.projection) * d.energy for d in proj.dominant_axes]
    assert len(proj.dominant_axes) == 3
    assert contribs == sorted(contribs, reverse=True)


def test_project_high_dim_shape(synthetic_high_dim: np.ndarray) -> None:
    basis = fit_basis(synthetic_high_dim, k=6)
    proj = project(basis, synthetic_high_dim[10])
    assert proj.projections.shape == (basis.ortho_basis.shape[0],)
    assert proj.probabilities.shape == (basis.ortho_basis.shape[0],)
    assert abs(float(np.sum(proj.probabilities)) - 1.0) < 1e-10
