"""Tests for ``TagRebalanceHandler`` — symbolic merge_tag proposal flow."""

from __future__ import annotations

import json
import sqlite3
import time
from pathlib import Path

from corlinman_evolution_engine.engine import EngineConfig, EvolutionEngine
from corlinman_evolution_engine.tag_rebalance import KIND_TAG_REBALANCE

from .conftest import insert_signal


def _all_proposals(db_path: Path) -> list[dict[str, object]]:
    conn = sqlite3.connect(db_path)
    try:
        conn.row_factory = sqlite3.Row
        rows = conn.execute(
            """SELECT id, kind, target, diff, reasoning, risk, budget_cost,
                      status, signal_ids, trace_ids, created_at
               FROM evolution_proposals ORDER BY id ASC"""
        ).fetchall()
    finally:
        conn.close()
    return [dict(r) for r in rows]


def _seed_recall_drop_cluster(
    db_path: Path,
    *,
    target: str,
    count: int,
) -> list[int]:
    """Insert ``count`` ``tag.recall.dropped`` signals on ``target``."""
    now_ms = int(time.time() * 1_000)
    ids: list[int] = []
    for i in range(count):
        sid = insert_signal(
            db_path,
            event_kind="tag.recall.dropped",
            target=target,
            severity="warn",
            payload_json='{"reason": "no_chunk_recall"}',
            trace_id=f"trace-{target}-{i}",
            session_id="sess-tag",
            observed_at=now_ms - 60_000 + i,
        )
        ids.append(sid)
    return ids


async def test_propose_emits_one_proposal_per_cluster(
    evolution_db: Path, kb_db: Path
) -> None:
    """Happy path: 3 signals on one path → 1 merge_tag proposal."""
    _seed_recall_drop_cluster(evolution_db, target="coding/python/asyncio", count=3)

    config = EngineConfig(
        db_path=evolution_db,
        kb_path=kb_db,
        lookback_days=1,
        min_cluster_size=3,
        # Only the tag handler runs — keep the assertion narrow.
        enabled_kinds=(KIND_TAG_REBALANCE,),
    )
    summary = await EvolutionEngine(config).run_once()

    assert summary.proposals_written == 1
    assert summary.proposals_by_kind == {KIND_TAG_REBALANCE: 1}

    proposals = _all_proposals(evolution_db)
    assert len(proposals) == 1
    p = proposals[0]
    assert p["kind"] == KIND_TAG_REBALANCE
    assert p["target"] == "merge_tag:coding/python/asyncio"
    assert p["risk"] == "medium"
    assert p["budget_cost"] == 1
    assert p["diff"] == ""
    assert p["status"] == "pending"
    sig_ids = json.loads(str(p["signal_ids"]))
    assert sig_ids == [1, 2, 3]
    trace_ids = json.loads(str(p["trace_ids"]))
    assert trace_ids == [
        "trace-coding/python/asyncio-0",
        "trace-coding/python/asyncio-1",
        "trace-coding/python/asyncio-2",
    ]


async def test_propose_below_threshold_yields_no_proposal(
    evolution_db: Path, kb_db: Path
) -> None:
    """N-1 signals — clustering drops the bucket, handler sees nothing."""
    _seed_recall_drop_cluster(evolution_db, target="coding/python", count=2)

    config = EngineConfig(
        db_path=evolution_db,
        kb_path=kb_db,
        lookback_days=1,
        min_cluster_size=3,
        enabled_kinds=(KIND_TAG_REBALANCE,),
    )
    summary = await EvolutionEngine(config).run_once()

    assert summary.signals_loaded == 2
    assert summary.clusters_found == 0
    assert summary.proposals_written == 0
    assert _all_proposals(evolution_db) == []


async def test_propose_dedups_against_existing_target(
    evolution_db: Path, kb_db: Path
) -> None:
    """Pre-seeded proposal with the same target → handler skips it."""
    _seed_recall_drop_cluster(evolution_db, target="coding/rust", count=3)

    config = EngineConfig(
        db_path=evolution_db,
        kb_path=kb_db,
        lookback_days=1,
        min_cluster_size=3,
        enabled_kinds=(KIND_TAG_REBALANCE,),
    )

    s1 = await EvolutionEngine(config).run_once()
    assert s1.proposals_written == 1

    # Re-run — same cluster, already filed.
    s2 = await EvolutionEngine(config).run_once()
    assert s2.proposals_written == 0
    assert s2.skipped_existing == 1
    assert len(_all_proposals(evolution_db)) == 1


async def test_propose_multi_target_emits_one_proposal_per_path(
    evolution_db: Path, kb_db: Path
) -> None:
    """Two distinct paths → two independent proposals."""
    _seed_recall_drop_cluster(evolution_db, target="coding/python", count=3)
    _seed_recall_drop_cluster(evolution_db, target="design/typography", count=4)

    config = EngineConfig(
        db_path=evolution_db,
        kb_path=kb_db,
        lookback_days=1,
        min_cluster_size=3,
        enabled_kinds=(KIND_TAG_REBALANCE,),
    )
    summary = await EvolutionEngine(config).run_once()

    assert summary.proposals_written == 2
    proposals = _all_proposals(evolution_db)
    targets = sorted(str(p["target"]) for p in proposals)
    assert targets == ["merge_tag:coding/python", "merge_tag:design/typography"]


async def test_propose_ignores_other_event_kinds(
    evolution_db: Path, kb_db: Path
) -> None:
    """Cluster on a different event_kind shouldn't trigger this handler."""
    now_ms = int(time.time() * 1_000)
    for i in range(3):
        insert_signal(
            evolution_db,
            event_kind="tool.call.failed",  # not tag.recall.dropped
            target="coding/python",
            severity="error",
            payload_json="{}",
            trace_id=f"t-{i}",
            session_id="s",
            observed_at=now_ms - 60_000 + i,
        )

    config = EngineConfig(
        db_path=evolution_db,
        kb_path=kb_db,
        lookback_days=1,
        min_cluster_size=3,
        enabled_kinds=(KIND_TAG_REBALANCE,),
    )
    summary = await EvolutionEngine(config).run_once()

    assert summary.clusters_found == 1
    assert summary.proposals_written == 0
