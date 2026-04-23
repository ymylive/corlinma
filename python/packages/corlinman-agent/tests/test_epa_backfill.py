"""Unit tests for :mod:`corlinman_agent.rag.epa_backfill`.

The backfill job is offline maintenance: it reads chunk vectors out of
an existing SQLite knowledge-base file and writes per-chunk EPA rows to
the `chunk_epa` table. These tests seed a throwaway schema-v6 DB via
raw DDL (mirroring `corlinman-vector::sqlite::SCHEMA_SQL`) and check the
job's outputs are well-formed, idempotent, and namespace-aware.
"""

from __future__ import annotations

import sqlite3
from pathlib import Path

import numpy as np
import pytest
from corlinman_agent.rag.epa_backfill import (
    BackfillConfig,
    EpaBackfiller,
)

# Minimal subset of the Rust v6 schema needed for the backfill's queries.
# We don't need the FTS5 virtual table or the pending_approvals gate here,
# just `chunks` (with the `vector` BLOB + `namespace` column) and
# `chunk_epa`. The `chunk_epa` DDL is copied verbatim from
# `rust/crates/corlinman-vector/src/sqlite.rs::SCHEMA_SQL`.
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


def _seed_db(
    path: Path,
    *,
    n_chunks: int = 50,
    dim: int = 16,
    namespace: str = "general",
    null_vector_ids: tuple[int, ...] = (),
    extra_namespace: str | None = None,
    extra_count: int = 0,
) -> list[int]:
    """Create a schema-v6 DB at `path` and insert `n_chunks` chunks.

    Returns the list of chunk ids in insertion order. Chunks whose
    insertion-order index is in `null_vector_ids` get a NULL vector so
    the backfill must skip them. When `extra_namespace` is set, the
    fixture additionally inserts `extra_count` chunks in that namespace.
    """
    conn = sqlite3.connect(path)
    try:
        conn.executescript(_V6_SCHEMA)
        file_id = conn.execute(
            "INSERT INTO files(path, diary_name, checksum, mtime, size) "
            "VALUES (?, ?, ?, ?, ?)",
            ("notes/a.md", "notes", "h", 0, 0),
        ).lastrowid

        rng = np.random.default_rng(seed=123)
        ids: list[int] = []
        for i in range(n_chunks):
            vec = rng.standard_normal(dim).astype("<f4")
            blob: bytes | None = (
                None if i in null_vector_ids else np.ascontiguousarray(vec).tobytes()
            )
            cur = conn.execute(
                "INSERT INTO chunks(file_id, chunk_index, content, vector, namespace) "
                "VALUES (?, ?, ?, ?, ?)",
                (file_id, i, f"chunk {i}", blob, namespace),
            )
            ids.append(int(cur.lastrowid))

        if extra_namespace is not None and extra_count > 0:
            for j in range(extra_count):
                vec = rng.standard_normal(dim).astype("<f4")
                blob = np.ascontiguousarray(vec).tobytes()
                cur = conn.execute(
                    "INSERT INTO chunks(file_id, chunk_index, content, vector, namespace) "
                    "VALUES (?, ?, ?, ?, ?)",
                    (file_id, n_chunks + j, f"extra {j}", blob, extra_namespace),
                )
                ids.append(int(cur.lastrowid))

        conn.commit()
        return ids
    finally:
        conn.close()


def _read_epa(path: Path, chunk_id: int) -> tuple[bytes, float, float] | None:
    conn = sqlite3.connect(path)
    try:
        row = conn.execute(
            "SELECT projections, entropy, logic_depth FROM chunk_epa WHERE chunk_id = ?",
            (chunk_id,),
        ).fetchone()
        if row is None:
            return None
        return (row[0], float(row[1]), float(row[2]))
    finally:
        conn.close()


def _count_epa(path: Path, namespace: str | None = None) -> int:
    conn = sqlite3.connect(path)
    try:
        if namespace is None:
            return int(conn.execute("SELECT COUNT(*) FROM chunk_epa").fetchone()[0])
        return int(
            conn.execute(
                "SELECT COUNT(*) FROM chunk_epa ce JOIN chunks c ON c.id = ce.chunk_id "
                "WHERE c.namespace = ?",
                (namespace,),
            ).fetchone()[0]
        )
    finally:
        conn.close()


# ---------------------------------------------------------------------------
# tests


async def test_backfill_populates_empty_chunk_epa(tmp_path: Path) -> None:
    db = tmp_path / "kb.sqlite"
    ids = _seed_db(db, n_chunks=50, dim=16)

    job = EpaBackfiller(db, BackfillConfig(k=4))
    stats = await job.run()

    assert stats.chunks_processed == 50
    assert stats.chunks_skipped == 0
    assert stats.basis_axes >= 1
    assert _count_epa(db) == 50

    # Every row must have non-NaN scalars + a non-empty projections blob.
    for chunk_id in ids:
        row = _read_epa(db, chunk_id)
        assert row is not None, f"missing EPA row for chunk_id={chunk_id}"
        projections_blob, entropy, logic_depth = row
        assert len(projections_blob) > 0 and len(projections_blob) % 4 == 0
        assert np.isfinite(entropy) and 0.0 <= entropy <= 1.0
        assert np.isfinite(logic_depth) and 0.0 <= logic_depth <= 1.0
        # logic_depth = 1 - entropy, sanity-check the identity.
        assert abs((entropy + logic_depth) - 1.0) < 1e-6


async def test_backfill_skips_chunks_without_vectors(tmp_path: Path) -> None:
    db = tmp_path / "kb.sqlite"
    # First 5 chunks have NULL vectors; the rest carry real vectors.
    null_idx = (0, 1, 2, 3, 4)
    ids = _seed_db(db, n_chunks=20, dim=8, null_vector_ids=null_idx)

    job = EpaBackfiller(db, BackfillConfig(k=3))
    stats = await job.run()

    # Only the 15 chunks with vectors should have been processed.
    assert stats.chunks_processed == 15
    assert stats.chunks_skipped == 5
    assert _count_epa(db) == 15

    # Explicitly verify the skipped ids have no EPA row.
    for i in null_idx:
        assert _read_epa(db, ids[i]) is None, (
            f"chunk at null_idx={i} (id={ids[i]}) should not have an EPA row"
        )
    # ... and the vectorised ids do.
    for i in range(len(null_idx), len(ids)):
        assert _read_epa(db, ids[i]) is not None


async def test_backfill_is_idempotent(tmp_path: Path) -> None:
    db = tmp_path / "kb.sqlite"
    ids = _seed_db(db, n_chunks=30, dim=12)

    job = EpaBackfiller(db, BackfillConfig(k=4))
    await job.run()

    # Snapshot every row's content after the first run.
    snapshot: dict[int, tuple[bytes, float, float]] = {}
    for chunk_id in ids:
        row = _read_epa(db, chunk_id)
        assert row is not None
        snapshot[chunk_id] = row

    # Re-run on the unchanged corpus. Because KMeans is seeded (random_state=42
    # inside fit_basis) and the corpus didn't change, the basis is the same
    # and every projection is byte-identical.
    await EpaBackfiller(db, BackfillConfig(k=4)).run()

    for chunk_id in ids:
        row = _read_epa(db, chunk_id)
        assert row is not None
        prev_blob, prev_e, prev_ld = snapshot[chunk_id]
        new_blob, new_e, new_ld = row
        assert new_blob == prev_blob, f"projections drift for chunk_id={chunk_id}"
        assert new_e == pytest.approx(prev_e, abs=1e-9)
        assert new_ld == pytest.approx(prev_ld, abs=1e-9)


async def test_backfill_namespace_filter(tmp_path: Path) -> None:
    db = tmp_path / "kb.sqlite"
    # 20 chunks in "docs", 15 chunks in "diary:a".
    _seed_db(
        db,
        n_chunks=20,
        dim=8,
        namespace="docs",
        extra_namespace="diary:a",
        extra_count=15,
    )

    job = EpaBackfiller(db, BackfillConfig(k=3, target_namespace="docs"))
    stats = await job.run()

    assert stats.chunks_processed == 20
    # "diary:a" chunks are outside the target namespace → counted as skipped.
    assert stats.chunks_skipped == 15
    assert _count_epa(db, namespace="docs") == 20
    assert _count_epa(db, namespace="diary:a") == 0
