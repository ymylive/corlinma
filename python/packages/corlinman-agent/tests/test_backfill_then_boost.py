"""B3 cross-workstream contract test (Python side).

Pins the storage-shape contract between the Python backfill job and the
Rust `EpaBoost::prepare` reader: after `EpaBackfiller.run()` completes,
every chunk with a non-NULL vector must have a `chunk_epa` row whose
columns decode to the shapes the Rust side expects — specifically,

    projections BLOB  → packed little-endian f32[] (len % 4 == 0, >0),
    entropy     REAL  → finite float in [0, 1],
    logic_depth REAL  → finite float in [0, 1], with entropy+logic_depth == 1.

The actual boost assertion lives on the Rust side (see
`rust/crates/corlinman-integration-tests/tests/epa_subtree_composition.rs`);
this test deliberately does NOT cross the FFI boundary — it just proves
the Python backfill writes rows the Rust reader can consume, and that
doing so is idempotent (re-running the job against an unchanged corpus
yields byte-identical rows).

See `docs/protocols/b3-contracts.md` for the full B3 contract surface.
"""

from __future__ import annotations

import sqlite3
from pathlib import Path

import numpy as np
import pytest
from corlinman_agent.rag.epa_backfill import BackfillConfig, EpaBackfiller

# The `chunk_epa` DDL is copy-pasted from
# `rust/crates/corlinman-vector/src/sqlite.rs::SCHEMA_SQL`. Keeping it in
# the test file (rather than calling into the Rust opener) means this
# test is self-contained and does not require sqlx/usearch in the Python
# test environment. If the Rust schema drifts, this test is the canary.
_V6_SCHEMA = """
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT UNIQUE NOT NULL,
    diary_name TEXT NOT NULL,
    checksum TEXT NOT NULL,
    mtime INTEGER NOT NULL,
    size INTEGER NOT NULL,
    updated_at INTEGER
);

CREATE TABLE IF NOT EXISTS chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id INTEGER NOT NULL,
    chunk_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    vector BLOB,
    namespace TEXT NOT NULL DEFAULT 'general',
    FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS chunk_epa (
    chunk_id     INTEGER PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
    projections  BLOB    NOT NULL,
    entropy      REAL    NOT NULL,
    logic_depth  REAL    NOT NULL,
    computed_at  INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);
"""


def _seed_corpus(path: Path, *, n_chunks: int = 30, dim: int = 16) -> list[int]:
    """Seed a v6 SQLite DB with `n_chunks` chunks of `dim`-d f32 vectors.

    Vectors use the same little-endian f32 packing as Rust
    `f32_slice_to_blob`, mirroring what the backfill reads back. Returns
    the inserted chunk ids in insertion order.
    """
    conn = sqlite3.connect(path)
    try:
        conn.executescript(_V6_SCHEMA)
        file_id = conn.execute(
            "INSERT INTO files(path, diary_name, checksum, mtime, size) "
            "VALUES (?, ?, ?, ?, ?)",
            ("notes/b3.md", "notes", "h", 0, 0),
        ).lastrowid
        rng = np.random.default_rng(seed=20240701)
        ids: list[int] = []
        for i in range(n_chunks):
            # Known, deterministic per-chunk vector: a seeded normal so the
            # fitted basis is reproducible across runs (idempotency check).
            vec = rng.standard_normal(dim).astype("<f4")
            blob = np.ascontiguousarray(vec).tobytes()
            cur = conn.execute(
                "INSERT INTO chunks(file_id, chunk_index, content, vector, namespace) "
                "VALUES (?, ?, ?, ?, ?)",
                (file_id, i, f"banana chunk {i}", blob, "general"),
            )
            ids.append(int(cur.lastrowid))
        conn.commit()
        return ids
    finally:
        conn.close()


def _read_all_epa(path: Path) -> dict[int, tuple[bytes, float, float]]:
    conn = sqlite3.connect(path)
    try:
        rows = conn.execute(
            "SELECT chunk_id, projections, entropy, logic_depth FROM chunk_epa"
        ).fetchall()
        return {int(r[0]): (r[1], float(r[2]), float(r[3])) for r in rows}
    finally:
        conn.close()


async def test_backfill_writes_rust_compatible_chunk_epa_rows(tmp_path: Path) -> None:
    """After `EpaBackfiller.run()`, every chunk must have a `chunk_epa`
    row whose columns match the shape the Rust `EpaBoost::prepare`
    reader consumes (see `corlinman-vector::sqlite::get_chunk_epa`).
    """
    db = tmp_path / "kb.sqlite"
    ids = _seed_corpus(db, n_chunks=30, dim=16)

    stats = await EpaBackfiller(db, BackfillConfig(k=4)).run()
    assert stats.chunks_processed == 30
    assert stats.chunks_skipped == 0
    assert stats.basis_axes >= 1

    rows = _read_all_epa(db)
    assert set(rows) == set(ids), (
        f"backfill left some chunks without EPA rows: "
        f"missing={set(ids) - set(rows)}"
    )

    # Validate one representative row in detail. All rows are produced by
    # the same code path, so one is enough to pin the column shape.
    first_id = ids[0]
    blob, entropy, logic_depth = rows[first_id]

    # projections: packed little-endian f32[] — non-empty, 4-byte aligned.
    assert len(blob) > 0, "projections blob must not be empty"
    assert len(blob) % 4 == 0, (
        f"projections blob length {len(blob)} is not a multiple of 4 — "
        "Rust `blob_to_f32_vec` would reject it"
    )
    decoded = np.frombuffer(blob, dtype="<f4")
    assert decoded.size == len(blob) // 4
    assert np.all(np.isfinite(decoded)), "projections contain NaN or inf"

    # entropy + logic_depth: finite, in [0, 1], and complementary (the
    # backfill enforces `logic_depth = 1 - entropy`).
    assert np.isfinite(entropy) and 0.0 <= entropy <= 1.0
    assert np.isfinite(logic_depth) and 0.0 <= logic_depth <= 1.0
    assert abs((entropy + logic_depth) - 1.0) < 1e-6


async def test_backfill_is_idempotent_byte_for_byte(tmp_path: Path) -> None:
    """Re-running the backfill against the unchanged corpus must yield
    byte-identical `chunk_epa` rows. The Rust side's `EpaBoost::prepare`
    cache is keyed by `logic_depth`, so any value drift between runs
    would silently shift query rankings across cold restarts. This test
    catches that whole class of regression.
    """
    db = tmp_path / "kb.sqlite"
    ids = _seed_corpus(db, n_chunks=30, dim=16)

    first_stats = await EpaBackfiller(db, BackfillConfig(k=4)).run()
    assert first_stats.chunks_processed == 30
    first_rows = _read_all_epa(db)

    # Row count unchanged after a second run.
    second_stats = await EpaBackfiller(db, BackfillConfig(k=4)).run()
    assert second_stats.chunks_processed == 30
    second_rows = _read_all_epa(db)
    assert set(second_rows) == set(first_rows) == set(ids)

    # Values byte-identical: same projections BLOB, same scalars (modulo
    # float tolerance that a sqlite REAL roundtrip technically guarantees
    # exactly, but we compare with a tight tolerance for safety).
    for cid in ids:
        prev_blob, prev_e, prev_ld = first_rows[cid]
        new_blob, new_e, new_ld = second_rows[cid]
        assert new_blob == prev_blob, f"projections drift for chunk_id={cid}"
        assert new_e == pytest.approx(prev_e, abs=1e-12)
        assert new_ld == pytest.approx(prev_ld, abs=1e-12)
