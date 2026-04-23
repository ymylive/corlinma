"""Residual pyramid: iterative Gram-Schmidt projection of a query onto an EPA basis.

We keep peeling energy off the residual axis-by-axis until we've explained the
target fraction (default 0.90) of basis energy, or we've used every axis.
"""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np

from corlinman_tagmemo.epa import EpaBasis


@dataclass(frozen=True)
class PyramidLevel:
    axis_label: str
    coefficient: float
    explained_energy: float
    cumulative_explained: float


@dataclass(frozen=True)
class PyramidFeatures:
    tag_memo_activation: float  # sum(|coeff_i|) / sum_of_energies_used
    coverage: float  # levels_used / K


@dataclass(frozen=True)
class PyramidResult:
    levels: list[PyramidLevel]
    total_explained_energy: float
    features: PyramidFeatures


def build_pyramid(
    basis: EpaBasis,
    query: np.ndarray,
    target_explained: float = 0.90,
) -> PyramidResult:
    """Peel query energy onto the basis until target_explained is reached."""
    query = np.asarray(query, dtype=np.float64).reshape(-1)
    if query.shape[0] != basis.basis_mean.shape[0]:
        raise ValueError(
            f"query dim {query.shape[0]} != basis dim "
            f"{basis.basis_mean.shape[0]}"
        )
    target = float(np.clip(target_explained, 0.0, 1.0))

    total_energy = float(np.sum(basis.basis_energies))
    k = basis.ortho_basis.shape[0]

    residual = query - basis.basis_mean
    levels: list[PyramidLevel] = []
    cumulative_explained_sq = 0.0
    # For `tag_memo_activation` we need both |coefficients| sum and the sum of
    # the axis energies we actually visited.
    sum_abs_coeff = 0.0
    sum_energies_used = 0.0

    for i in range(k):
        axis = basis.ortho_basis[i]
        energy = float(basis.basis_energies[i])
        coeff = float(np.dot(residual, axis))
        # Subtract projection from residual (Gram-Schmidt peel).
        residual = residual - coeff * axis

        explained = (coeff * energy) ** 2
        cumulative_explained_sq += explained
        sum_abs_coeff += abs(coeff)
        sum_energies_used += energy

        cumulative_ratio = (
            cumulative_explained_sq / total_energy
            if total_energy > 0.0
            else 0.0
        )

        levels.append(
            PyramidLevel(
                axis_label=basis.basis_labels[i],
                coefficient=coeff,
                explained_energy=explained,
                cumulative_explained=float(cumulative_ratio),
            )
        )

        if total_energy > 0.0 and cumulative_ratio >= target:
            break

    total_explained = levels[-1].cumulative_explained if levels else 0.0

    activation = (
        sum_abs_coeff / sum_energies_used if sum_energies_used > 0.0 else 0.0
    )
    coverage = len(levels) / k if k > 0 else 0.0

    return PyramidResult(
        levels=levels,
        total_explained_energy=float(total_explained),
        features=PyramidFeatures(
            tag_memo_activation=float(activation),
            coverage=float(np.clip(coverage, 0.0, 1.0)),
        ),
    )
