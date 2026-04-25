//! Authoritative SQLite schema for the EvolutionLoop. Cross-language
//! contract — Python engine and Rust observer/API both bind to these tables.
//!
//! Applied idempotently via `CREATE … IF NOT EXISTS`, so re-running on a
//! populated DB is a no-op. New columns must land via ALTER TABLE in a
//! versioned migration (see `docs/migration/`).

pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS evolution_signals (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    event_kind   TEXT NOT NULL,
    target       TEXT,
    severity     TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    trace_id     TEXT,
    session_id   TEXT,
    observed_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_evol_signals_kind_target
    ON evolution_signals(event_kind, target);

CREATE INDEX IF NOT EXISTS idx_evol_signals_observed
    ON evolution_signals(observed_at);

CREATE TABLE IF NOT EXISTS evolution_proposals (
    id              TEXT PRIMARY KEY,
    kind            TEXT NOT NULL,
    target          TEXT NOT NULL,
    diff            TEXT NOT NULL,
    reasoning       TEXT NOT NULL,
    risk            TEXT NOT NULL,
    budget_cost     INTEGER NOT NULL DEFAULT 1,
    status          TEXT NOT NULL,
    shadow_metrics  TEXT,
    signal_ids      TEXT NOT NULL,
    trace_ids       TEXT NOT NULL,
    created_at      INTEGER NOT NULL,
    decided_at      INTEGER,
    decided_by      TEXT,
    applied_at      INTEGER,
    rollback_of     TEXT REFERENCES evolution_proposals(id)
);

CREATE INDEX IF NOT EXISTS idx_evol_proposals_status
    ON evolution_proposals(status);

CREATE INDEX IF NOT EXISTS idx_evol_proposals_created
    ON evolution_proposals(created_at);

CREATE TABLE IF NOT EXISTS evolution_history (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    proposal_id      TEXT NOT NULL REFERENCES evolution_proposals(id),
    kind             TEXT NOT NULL,
    target           TEXT NOT NULL,
    before_sha       TEXT NOT NULL,
    after_sha        TEXT NOT NULL,
    inverse_diff     TEXT NOT NULL,
    metrics_baseline TEXT NOT NULL,
    applied_at       INTEGER NOT NULL,
    rolled_back_at   INTEGER,
    rollback_reason  TEXT
);

CREATE INDEX IF NOT EXISTS idx_evol_history_proposal
    ON evolution_history(proposal_id);

CREATE INDEX IF NOT EXISTS idx_evol_history_applied
    ON evolution_history(applied_at);
"#;
