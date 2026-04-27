"""Tests for Phase 3 W1-C budget enforcement.

The engine respects ``BudgetConfig.enabled`` as a master switch; when on,
the per-week / per-kind caps gate inserts before they hit
``EvolutionStore.insert_proposal``. Skips emit one
``evolution.budget.exceeded`` signal per run summarising the damage so the
Rust ``EvolutionObserver`` can surface it like any other warn-level signal.
"""

from __future__ import annotations

import json
import sqlite3
import time
from pathlib import Path

from corlinman_evolution_engine.engine import (
    BudgetConfig,
    EngineConfig,
    EvolutionEngine,
    _iso_week_start_ms,
)
from corlinman_evolution_engine.proposals import (
    EvolutionProposal,
    ProposalContext,
)

from .conftest import insert_signal


def _all_proposals(db_path: Path) -> list[dict[str, object]]:
    conn = sqlite3.connect(db_path)
    try:
        conn.row_factory = sqlite3.Row
        rows = conn.execute(
            "SELECT id, kind, target FROM evolution_proposals ORDER BY id ASC"
        ).fetchall()
    finally:
        conn.close()
    return [dict(r) for r in rows]


def _budget_signals(db_path: Path) -> list[dict[str, object]]:
    conn = sqlite3.connect(db_path)
    try:
        conn.row_factory = sqlite3.Row
        rows = conn.execute(
            """SELECT event_kind, target, severity, payload_json, observed_at
               FROM evolution_signals
               WHERE event_kind = 'evolution.budget.exceeded'
               ORDER BY id ASC"""
        ).fetchall()
    finally:
        conn.close()
    return [dict(r) for r in rows]


def _seed_failure_cluster(db_path: Path, *, count: int = 5) -> None:
    """Seed a 5-signal cluster so clustering produces a trigger."""
    now_ms = int(time.time() * 1_000)
    for i in range(count):
        insert_signal(
            db_path,
            event_kind="tool.call.failed",
            target="web_search",
            severity="error",
            payload_json='{"reason": "timeout"}',
            trace_id=f"trace-{i}",
            session_id="sess-1",
            observed_at=now_ms - 60_000 + i,
        )


class _FixedKindHandler:
    """Emits N candidates of a fixed kind/target pattern.

    Used to drive the budget gate deterministically without depending on
    the kb-scanning behavior of ``MemoryOpHandler``. Targets are unique so
    the engine's existing-target dedup never trips.
    """

    def __init__(self, kind: str, count: int, *, target_prefix: str = "t") -> None:
        self._kind = kind
        self._count = count
        self._prefix = target_prefix

    @property
    def kind(self) -> str:
        return self._kind

    async def existing_targets(self, conn: object) -> set[str]:
        return set()

    async def propose(self, ctx: ProposalContext) -> list[EvolutionProposal]:
        return [
            EvolutionProposal(
                kind=self._kind,
                target=f"{self._prefix}-{i}",
                diff="",
                reasoning="fixture",
                risk="low",
                budget_cost=0,
                signal_ids=[c.signal_ids[0] for c in ctx.clusters][:1],
                trace_ids=[],
            )
            for i in range(self._count)
        ]


# ---------------------------------------------------------------------------


async def test_budget_disabled_inserts_all(
    evolution_db: Path, kb_db: Path
) -> None:
    """``enabled=false`` → no gating, identical to pre-W1-C behavior."""
    _seed_failure_cluster(evolution_db, count=3)
    handler = _FixedKindHandler("memory_op", count=5)

    config = EngineConfig(
        db_path=evolution_db,
        kb_path=kb_db,
        min_cluster_size=3,
        max_proposals_per_run=10,
        enabled_kinds=("memory_op",),
        budget=BudgetConfig(enabled=False, weekly_total=2),  # ignored
    )
    summary = await EvolutionEngine(config, handlers=[handler]).run_once()

    assert summary.proposals_written == 5
    assert summary.proposals_skipped_budget == 0
    assert summary.budget_skips_by_kind == {}
    assert len(_all_proposals(evolution_db)) == 5
    assert _budget_signals(evolution_db) == []


async def test_budget_weekly_total_caps_total(
    evolution_db: Path, kb_db: Path
) -> None:
    """``weekly_total=2`` with 5 candidates → 2 inserted, 3 skipped, 1 signal."""
    _seed_failure_cluster(evolution_db, count=3)
    handler = _FixedKindHandler("memory_op", count=5)

    config = EngineConfig(
        db_path=evolution_db,
        kb_path=kb_db,
        min_cluster_size=3,
        max_proposals_per_run=10,
        enabled_kinds=("memory_op",),
        budget=BudgetConfig(enabled=True, weekly_total=2),
    )
    summary = await EvolutionEngine(config, handlers=[handler]).run_once()

    assert summary.proposals_written == 2
    assert summary.proposals_skipped_budget == 3
    assert summary.budget_skips_by_kind == {"memory_op": 3}
    assert len(_all_proposals(evolution_db)) == 2

    signals = _budget_signals(evolution_db)
    assert len(signals) == 1
    assert signals[0]["target"] == "memory_op"
    assert signals[0]["severity"] == "warn"


async def test_budget_per_kind_cap_isolates_kinds(
    evolution_db: Path, kb_db: Path
) -> None:
    """``per_kind={memory_op: 1}`` with 3 memory_op candidates → 1 in, 2 skipped."""
    _seed_failure_cluster(evolution_db, count=3)
    handler = _FixedKindHandler("memory_op", count=3)

    config = EngineConfig(
        db_path=evolution_db,
        kb_path=kb_db,
        min_cluster_size=3,
        max_proposals_per_run=10,
        enabled_kinds=("memory_op",),
        budget=BudgetConfig(
            enabled=True,
            weekly_total=99,  # not the gate
            per_kind={"memory_op": 1},
        ),
    )
    summary = await EvolutionEngine(config, handlers=[handler]).run_once()

    assert summary.proposals_written == 1
    assert summary.proposals_skipped_budget == 2
    assert summary.budget_skips_by_kind == {"memory_op": 2}
    assert len(_all_proposals(evolution_db)) == 1


async def test_budget_signal_payload_shape(
    evolution_db: Path, kb_db: Path
) -> None:
    """The single emitted signal carries the documented payload shape."""
    _seed_failure_cluster(evolution_db, count=3)
    handler = _FixedKindHandler("memory_op", count=4)

    config = EngineConfig(
        db_path=evolution_db,
        kb_path=kb_db,
        min_cluster_size=3,
        max_proposals_per_run=10,
        enabled_kinds=("memory_op",),
        budget=BudgetConfig(enabled=True, weekly_total=1),
    )
    summary = await EvolutionEngine(config, handlers=[handler]).run_once()

    assert summary.proposals_skipped_budget == 3
    signals = _budget_signals(evolution_db)
    assert len(signals) == 1
    sig = signals[0]
    assert sig["event_kind"] == "evolution.budget.exceeded"
    assert sig["severity"] == "warn"
    assert sig["target"] == "memory_op"
    payload = json.loads(str(sig["payload_json"]))
    assert payload == {
        "weekly_total_used": 3,
        "per_kind_skips": {"memory_op": 3},
    }


# ---------------------------------------------------------------------------
# Helper coverage — lock the ISO-week boundary so Rust + Python stay aligned.
# ---------------------------------------------------------------------------


def test_iso_week_start_ms_monday_anchor() -> None:
    # 2026-04-29 14:30:00 UTC is a Wednesday → Monday 2026-04-27 00:00 UTC.
    wed_ms = 1_777_473_000_000  # 2026-04-29T14:30:00Z
    monday_ms = _iso_week_start_ms(wed_ms)
    # 2026-04-27T00:00:00Z
    assert monday_ms == 1_777_248_000_000
