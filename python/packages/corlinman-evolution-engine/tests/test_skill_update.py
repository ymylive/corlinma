"""Tests for ``SkillUpdateHandler`` — append-marker diff proposal flow."""

from __future__ import annotations

import json
import sqlite3
import time
from pathlib import Path

from corlinman_evolution_engine.engine import EngineConfig, EvolutionEngine
from corlinman_evolution_engine.skill_update import KIND_SKILL_UPDATE

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


def _seed_skill_failure_cluster(
    db_path: Path,
    *,
    skill: str,
    count: int,
) -> list[int]:
    """Insert ``count`` ``skill.invocation.failed`` signals on ``skill``."""
    now_ms = int(time.time() * 1_000)
    ids: list[int] = []
    for i in range(count):
        sid = insert_signal(
            db_path,
            event_kind="skill.invocation.failed",
            target=skill,
            severity="error",
            payload_json='{"reason": "connection timeout"}',
            trace_id=f"trace-{skill}-{i}",
            session_id="sess-skill",
            observed_at=now_ms - 60_000 + i,
        )
        ids.append(sid)
    return ids


async def test_propose_emits_one_proposal_per_skill(
    evolution_db: Path, kb_db: Path
) -> None:
    """Happy path: 4 failures on web_search → 1 skill_update proposal."""
    _seed_skill_failure_cluster(evolution_db, skill="web_search", count=4)

    config = EngineConfig(
        db_path=evolution_db,
        kb_path=kb_db,
        lookback_days=1,
        min_cluster_size=3,
        enabled_kinds=(KIND_SKILL_UPDATE,),
    )
    summary = await EvolutionEngine(config).run_once()

    assert summary.proposals_written == 1
    assert summary.proposals_by_kind == {KIND_SKILL_UPDATE: 1}

    proposals = _all_proposals(evolution_db)
    assert len(proposals) == 1
    p = proposals[0]
    assert p["kind"] == KIND_SKILL_UPDATE
    assert p["target"] == "skills/web_search.md"
    assert p["risk"] == "medium"
    assert p["budget_cost"] == 2
    assert p["status"] == "pending"
    sig_ids = json.loads(str(p["signal_ids"]))
    assert sig_ids == [1, 2, 3, 4]


async def test_propose_diff_is_valid_unified_diff_prefix(
    evolution_db: Path, kb_db: Path
) -> None:
    """The diff string starts with the conventional unified-diff header."""
    _seed_skill_failure_cluster(evolution_db, skill="memory", count=3)

    config = EngineConfig(
        db_path=evolution_db,
        kb_path=kb_db,
        lookback_days=1,
        min_cluster_size=3,
        enabled_kinds=(KIND_SKILL_UPDATE,),
    )
    await EvolutionEngine(config).run_once()

    proposals = _all_proposals(evolution_db)
    diff = str(proposals[0]["diff"])

    # Unified-diff prefix — applier verifies these headers verbatim.
    assert diff.startswith("--- a/skills/memory.md\n")
    assert "+++ b/skills/memory.md\n" in diff
    # The marker line itself + the count from the cluster.
    assert "<!-- evolution-" in diff
    assert "3 failures" in diff
    assert "'memory'" in diff


async def test_propose_below_threshold_yields_no_proposal(
    evolution_db: Path, kb_db: Path
) -> None:
    """N-1 signals — cluster dropped, handler emits nothing."""
    _seed_skill_failure_cluster(evolution_db, skill="web_search", count=2)

    config = EngineConfig(
        db_path=evolution_db,
        kb_path=kb_db,
        lookback_days=1,
        min_cluster_size=3,
        enabled_kinds=(KIND_SKILL_UPDATE,),
    )
    summary = await EvolutionEngine(config).run_once()

    assert summary.signals_loaded == 2
    assert summary.clusters_found == 0
    assert summary.proposals_written == 0
    assert _all_proposals(evolution_db) == []


async def test_propose_dedups_against_existing_target(
    evolution_db: Path, kb_db: Path
) -> None:
    """Pre-seeded proposal on same skill → handler skips."""
    _seed_skill_failure_cluster(evolution_db, skill="code_review", count=3)

    config = EngineConfig(
        db_path=evolution_db,
        kb_path=kb_db,
        lookback_days=1,
        min_cluster_size=3,
        enabled_kinds=(KIND_SKILL_UPDATE,),
    )
    s1 = await EvolutionEngine(config).run_once()
    assert s1.proposals_written == 1

    s2 = await EvolutionEngine(config).run_once()
    assert s2.proposals_written == 0
    assert s2.skipped_existing == 1
    assert len(_all_proposals(evolution_db)) == 1


async def test_propose_multi_target_emits_one_proposal_per_skill(
    evolution_db: Path, kb_db: Path
) -> None:
    """Two skills failing independently → two proposals."""
    _seed_skill_failure_cluster(evolution_db, skill="web_search", count=3)
    _seed_skill_failure_cluster(evolution_db, skill="memory", count=4)

    config = EngineConfig(
        db_path=evolution_db,
        kb_path=kb_db,
        lookback_days=1,
        min_cluster_size=3,
        enabled_kinds=(KIND_SKILL_UPDATE,),
    )
    summary = await EvolutionEngine(config).run_once()

    assert summary.proposals_written == 2
    proposals = _all_proposals(evolution_db)
    targets = sorted(str(p["target"]) for p in proposals)
    assert targets == ["skills/memory.md", "skills/web_search.md"]


async def test_propose_ignores_other_event_kinds(
    evolution_db: Path, kb_db: Path
) -> None:
    """Cluster on a different event_kind shouldn't trigger this handler."""
    now_ms = int(time.time() * 1_000)
    for i in range(3):
        insert_signal(
            evolution_db,
            event_kind="tool.call.failed",  # not skill.invocation.failed
            target="web_search",
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
        enabled_kinds=(KIND_SKILL_UPDATE,),
    )
    summary = await EvolutionEngine(config).run_once()

    assert summary.clusters_found == 1
    assert summary.proposals_written == 0
