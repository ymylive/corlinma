//! `KindSimulator` trait + per-kind implementations.
//!
//! A simulator takes one [`EvalCase`] and a path to a tempdir copy of
//! `kb.sqlite` that the [`crate::runner::ShadowRunner`] has already
//! seeded with `case.kb_seed`. It must:
//!
//! 1. Read pre-state from the tempdir DB → `output.baseline`.
//! 2. Apply `case.proposal.target`'s operation to the tempdir DB only.
//! 3. Read post-state → `output.shadow`.
//! 4. Compare against `case.expected` → set `output.passed`.
//! 5. Return [`SimulatorOutput`].
//!
//! The runner aggregates per-case `baseline` maps into the proposal's
//! `baseline_metrics_json` column and per-case `shadow` maps into the
//! `shadow_metrics` column. The split gives the operator a measured
//! delta to review, not just the post-change snapshot.
//!
//! **Sandbox invariant**: simulators never touch any path other than
//! `kb_path`. The runner hands them a tempdir; the prod `kb.sqlite` is
//! never opened. Violations are runner-policy bugs, not simulator
//! ones.

use std::path::Path;
use std::str::FromStr;
use std::time::Instant;

use async_trait::async_trait;
use corlinman_evolution::EvolutionKind;
use serde_json::{json, Map, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use crate::eval::{EvalCase, ExpectedOutcome};

/// Errors a simulator can surface to the runner. The runner downgrades
/// these into a failed [`SimulatorOutput`] (with `passed = false` +
/// `error = Some(...)`) rather than aborting the whole shadow run, so
/// one bad case doesn't poison the rest of the eval set.
#[derive(Debug, thiserror::Error)]
pub enum SimulatorError {
    /// `case.proposal.target` could not be parsed (e.g. `merge_chunks:`
    /// missing ids, or unknown operation name).
    #[error("invalid target {target:?}: {reason}")]
    InvalidTarget { target: String, reason: String },

    /// Fixture seed or simulated mutation failed against the tempdir DB.
    #[error("sqlite error in {step}: {source}")]
    Sqlite {
        step: &'static str,
        #[source]
        source: sqlx::Error,
    },

    /// Catch-all for unanticipated runtime conditions.
    #[error("simulator runtime: {0}")]
    Runtime(String),
}

/// Outcome of running one [`EvalCase`] through a simulator.
///
/// `baseline` and `shadow` are kept as free-form `serde_json::Map`s so
/// each kind decides its own metric vocabulary (memory_op uses
/// `chunks_total` / `target_chunk_ids`; future kinds like skill_update
/// will use `success_rate` / `p95_latency_ms`). The runner aggregates
/// across cases without inspecting the keys.
#[derive(Debug, Clone)]
pub struct SimulatorOutput {
    pub case_name: String,
    /// True iff post-state matches `case.expected`. Defines the "did
    /// this case pass" bit the runner aggregates into pass_rate.
    pub passed: bool,
    /// Measurements taken *before* applying the proposal. Feeds
    /// `baseline_metrics_json`.
    pub baseline: serde_json::Map<String, serde_json::Value>,
    /// Measurements taken *after* applying the proposal. Feeds
    /// `shadow_metrics`.
    pub shadow: serde_json::Map<String, serde_json::Value>,
    /// Wall-clock simulator latency. The runner uses this to compute
    /// p95 / mean across the eval set.
    pub latency_ms: u64,
    /// Set when the simulator hit `SimulatorError`; `passed` is false
    /// in that case and `baseline` / `shadow` may be empty.
    pub error: Option<String>,
}

/// Pluggable per-kind simulator. The runner holds a registry keyed by
/// [`EvolutionKind`] and dispatches at run time.
#[async_trait]
pub trait KindSimulator: Send + Sync {
    /// Which kind this simulator handles. Must match the
    /// `EvolutionKind` discriminator in the proposals it runs against.
    fn kind(&self) -> EvolutionKind;

    /// Run one case against a sandboxed kb at `kb_path`.
    ///
    /// The runner has already (a) created the tempdir, (b) opened the
    /// SQLite at `kb_path`, (c) replayed `case.kb_seed`. The simulator
    /// only owns steps 1-5 in the module doc.
    async fn simulate(
        &self,
        case: &EvalCase,
        kb_path: &Path,
    ) -> Result<SimulatorOutput, SimulatorError>;
}

// ---------------------------------------------------------------------------
// MemoryOpSimulator
// ---------------------------------------------------------------------------

/// Max chars copied from `chunks.content` into baseline/shadow metrics.
/// Keeps the per-case JSON small enough that the runner can fan-in many
/// cases into one proposal row without blowing past sqlite's TEXT
/// practicality.
const CONTENT_PREVIEW_CHARS: usize = 200;

/// Parse a `merge_chunks:<id>,<id>[,<id>...]` target into chunk ids.
///
/// Rejects: missing prefix, fewer than 2 ids, non-integer ids, duplicate
/// ids. Each rejection becomes a `SimulatorError::InvalidTarget` so the
/// runner can downgrade the case to a reportable failure.
fn parse_merge_target(target: &str) -> Result<Vec<i64>, SimulatorError> {
    let Some(rest) = target.strip_prefix("merge_chunks:") else {
        return Err(SimulatorError::InvalidTarget {
            target: target.to_string(),
            reason: "expected prefix 'merge_chunks:'".to_string(),
        });
    };

    let mut ids: Vec<i64> = Vec::new();
    for raw in rest.split(',') {
        let trimmed = raw.trim();
        let id = i64::from_str(trimmed).map_err(|_| SimulatorError::InvalidTarget {
            target: target.to_string(),
            reason: format!("non-integer id '{trimmed}'"),
        })?;
        ids.push(id);
    }

    if ids.len() < 2 {
        return Err(SimulatorError::InvalidTarget {
            target: target.to_string(),
            reason: "merge needs at least 2 chunk ids".to_string(),
        });
    }

    // O(n^2) is fine — N <= a handful of ids in practice.
    for (i, a) in ids.iter().enumerate() {
        if ids[i + 1..].iter().any(|b| a == b) {
            return Err(SimulatorError::InvalidTarget {
                target: target.to_string(),
                reason: format!("duplicate id {a}"),
            });
        }
    }

    Ok(ids)
}

/// Truncate `s` to at most `CONTENT_PREVIEW_CHARS` Unicode scalar values.
/// Char-based (not byte-based) so we never split a UTF-8 codepoint.
fn preview(s: &str) -> String {
    s.chars().take(CONTENT_PREVIEW_CHARS).collect()
}

/// Simulator for `memory_op` proposals: collapses a set of chunk rows
/// into the lowest-id surviving row by deleting the rest, all within the
/// runner's tempdir SQLite. The W1-A scope is purely the deterministic
/// data op — Jaccard / similarity-based "should we even merge?" lives
/// upstream in `EvolutionEngine`. NoOp here means "target ids don't all
/// exist" (parse-or-prep short-circuit), not "content too dissimilar".
pub struct MemoryOpSimulator;

#[async_trait]
impl KindSimulator for MemoryOpSimulator {
    fn kind(&self) -> EvolutionKind {
        EvolutionKind::MemoryOp
    }

    async fn simulate(
        &self,
        case: &EvalCase,
        kb_path: &Path,
    ) -> Result<SimulatorOutput, SimulatorError> {
        let started = Instant::now();

        // Parse-time failures are per-case, not infra: surface as a
        // failed SimulatorOutput so the runner can record + continue.
        let parsed_ids = match parse_merge_target(&case.proposal.target) {
            Ok(ids) => ids,
            Err(e) => {
                return Ok(SimulatorOutput {
                    case_name: case.name.clone(),
                    passed: false,
                    baseline: Map::new(),
                    shadow: Map::new(),
                    latency_ms: started.elapsed().as_millis() as u64,
                    error: Some(e.to_string()),
                });
            }
        };

        let pool = open_pool(kb_path).await?;

        let baseline = capture_baseline(&pool, &parsed_ids).await?;
        let existing_ids = parsed_existing_ids(&baseline);
        let surviving_id = *parsed_ids.iter().min().expect("parse_merge_target ensures len>=2");

        // NoOp short-circuit: if any target id is missing from the DB we
        // refuse to apply a partial merge. This matches the runner's
        // "deterministic" contract: shadow only ever runs ops that the
        // source data fully supports.
        let all_present = existing_ids.len() == parsed_ids.len();

        let (rows_merged, surviving_content) = if all_present {
            apply_merge(&pool, surviving_id, &parsed_ids).await?
        } else {
            // No mutation; surviving_content reflects whatever the
            // surviving row is right now (or empty if it too is absent).
            let content = fetch_content(&pool, surviving_id).await?.unwrap_or_default();
            (0u32, content)
        };

        let shadow = capture_shadow(&pool, surviving_id, rows_merged, &surviving_content).await?;

        let passed = match &case.expected {
            ExpectedOutcome::Merged {
                rows_merged: expected_rows,
                surviving_chunk_id: expected_surv,
                ..
            } => rows_merged == *expected_rows && surviving_id == *expected_surv,
            ExpectedOutcome::NoOp { .. } => rows_merged == 0,
        };

        Ok(SimulatorOutput {
            case_name: case.name.clone(),
            passed,
            baseline,
            shadow,
            latency_ms: started.elapsed().as_millis() as u64,
            error: None,
        })
    }
}

/// Open the runner-prepared tempdir DB. `create_if_missing(false)` is a
/// guardrail: if the runner forgot to seed, we want a hard error here,
/// not a silent empty DB that "passes" every case.
async fn open_pool(kb_path: &Path) -> Result<SqlitePool, SimulatorError> {
    let opts = SqliteConnectOptions::new()
        .filename(kb_path)
        .create_if_missing(false);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .map_err(|e| SimulatorError::Sqlite {
            step: "open_pool",
            source: e,
        })
}

async fn capture_baseline(
    pool: &SqlitePool,
    parsed_ids: &[i64],
) -> Result<Map<String, Value>, SimulatorError> {
    let chunks_total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks")
        .fetch_one(pool)
        .await
        .map_err(|e| SimulatorError::Sqlite {
            step: "baseline.count",
            source: e,
        })?;

    let mut existing_ids: Vec<i64> = Vec::new();
    let mut target_contents: Map<String, Value> = Map::new();
    for id in parsed_ids {
        if let Some(content) = fetch_content(pool, *id).await? {
            existing_ids.push(*id);
            target_contents.insert(id.to_string(), Value::String(preview(&content)));
        }
    }

    let surviving_id_candidate = *parsed_ids.iter().min().expect("len>=2");

    let mut baseline = Map::new();
    baseline.insert("chunks_total".into(), json!(chunks_total));
    baseline.insert(
        "target_chunk_ids".into(),
        Value::Array(existing_ids.iter().map(|i| json!(i)).collect()),
    );
    baseline.insert("target_contents".into(), Value::Object(target_contents));
    baseline.insert("surviving_id_candidate".into(), json!(surviving_id_candidate));
    Ok(baseline)
}

/// Pull the existing-id list back out of a baseline map. We round-trip
/// through the map (rather than passing a separate Vec) so the captured
/// metric and the dispatch decision can never disagree.
fn parsed_existing_ids(baseline: &Map<String, Value>) -> Vec<i64> {
    baseline
        .get("target_chunk_ids")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
        .unwrap_or_default()
}

async fn fetch_content(pool: &SqlitePool, id: i64) -> Result<Option<String>, SimulatorError> {
    let row = sqlx::query("SELECT content FROM chunks WHERE id = ?1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| SimulatorError::Sqlite {
            step: "fetch_content",
            source: e,
        })?;
    Ok(row.map(|r| r.get::<String, _>(0)))
}

/// Delete every parsed id except `surviving_id`. Returns
/// `(rows_merged, surviving_content)`. `rows_merged` is `N-1` on a clean
/// run; the actual `rows_affected` from sqlite is what we report so a
/// drift between baseline and apply (someone else mutating the tempdir
/// mid-run, schema oddity) shows up in the shadow metric.
async fn apply_merge(
    pool: &SqlitePool,
    surviving_id: i64,
    parsed_ids: &[i64],
) -> Result<(u32, String), SimulatorError> {
    let to_delete: Vec<i64> = parsed_ids
        .iter()
        .copied()
        .filter(|id| *id != surviving_id)
        .collect();

    // Build "?,?,?" placeholders dynamically — sqlx doesn't expand Vec
    // bindings for IN(...).
    let placeholders = to_delete.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("DELETE FROM chunks WHERE id IN ({placeholders})");
    let mut q = sqlx::query(&sql);
    for id in &to_delete {
        q = q.bind(id);
    }
    let result = q.execute(pool).await.map_err(|e| SimulatorError::Sqlite {
        step: "apply_merge.delete",
        source: e,
    })?;

    let rows_merged = result.rows_affected() as u32;
    let surviving_content = fetch_content(pool, surviving_id).await?.unwrap_or_default();
    Ok((rows_merged, surviving_content))
}

async fn capture_shadow(
    pool: &SqlitePool,
    surviving_id: i64,
    rows_merged: u32,
    surviving_content: &str,
) -> Result<Map<String, Value>, SimulatorError> {
    let chunks_total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chunks")
        .fetch_one(pool)
        .await
        .map_err(|e| SimulatorError::Sqlite {
            step: "shadow.count",
            source: e,
        })?;

    let mut shadow = Map::new();
    shadow.insert("chunks_total".into(), json!(chunks_total));
    shadow.insert("surviving_chunk_id".into(), json!(surviving_id));
    shadow.insert("rows_merged".into(), json!(rows_merged));
    shadow.insert(
        "surviving_content".into(),
        Value::String(preview(surviving_content)),
    );
    Ok(shadow)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::ProposalSpec;
    use corlinman_evolution::EvolutionRisk;
    use sqlx::sqlite::SqliteConnectOptions;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // ---- parse_merge_target ----

    #[test]
    fn parse_merge_target_happy_path() {
        let ids = parse_merge_target("merge_chunks:1,2,3").unwrap();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn parse_merge_target_rejects_missing_prefix() {
        let err = parse_merge_target("not_a_merge:1,2").unwrap_err();
        assert!(matches!(err, SimulatorError::InvalidTarget { .. }), "got {err:?}");
    }

    #[test]
    fn parse_merge_target_rejects_single_id() {
        let err = parse_merge_target("merge_chunks:1").unwrap_err();
        assert!(matches!(err, SimulatorError::InvalidTarget { .. }), "got {err:?}");
    }

    #[test]
    fn parse_merge_target_rejects_non_integer() {
        let err = parse_merge_target("merge_chunks:1,abc").unwrap_err();
        assert!(matches!(err, SimulatorError::InvalidTarget { .. }), "got {err:?}");
    }

    #[test]
    fn parse_merge_target_rejects_duplicates() {
        let err = parse_merge_target("merge_chunks:1,1").unwrap_err();
        assert!(matches!(err, SimulatorError::InvalidTarget { .. }), "got {err:?}");
    }

    // ---- simulate ----

    /// Build a tempdir SQLite with the v0.3 chunks/files schema (minus
    /// FTS triggers — simulator never touches FTS) and seed it via the
    /// caller's SQL list. Returns `(tmp, kb_path)`; the tmp guard must
    /// outlive the test.
    async fn make_kb(seed: &[&str]) -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("kb.sqlite");
        let opts = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();

        let bootstrap = [
            "CREATE TABLE files (id INTEGER PRIMARY KEY, path TEXT, diary_name TEXT, checksum TEXT, mtime INTEGER, size INTEGER);",
            "CREATE TABLE chunks (id INTEGER PRIMARY KEY, file_id INTEGER, chunk_index INTEGER, content TEXT, namespace TEXT DEFAULT 'general');",
            "INSERT INTO files VALUES (1, 'fx.md', 'fixture', 'h', 0, 0);",
        ];
        for s in bootstrap.iter().chain(seed.iter()) {
            sqlx::query(s).execute(&pool).await.unwrap();
        }
        pool.close().await;
        (tmp, path)
    }

    fn case(name: &str, target: &str, expected: ExpectedOutcome) -> EvalCase {
        EvalCase {
            name: name.to_string(),
            kind: Some(EvolutionKind::MemoryOp),
            description: "test".into(),
            kb_seed: vec![],
            proposal: ProposalSpec {
                target: target.to_string(),
                reasoning: "test".into(),
                risk: EvolutionRisk::High,
                signal_ids: vec![],
            },
            expected,
        }
    }

    #[tokio::test]
    async fn simulate_returns_merged_for_existing_chunks() {
        let (_tmp, kb) = make_kb(&[
            "INSERT INTO chunks(id, file_id, chunk_index, content, namespace) VALUES (1, 1, 0, 'alpha', 'general');",
            "INSERT INTO chunks(id, file_id, chunk_index, content, namespace) VALUES (2, 1, 1, 'beta', 'general');",
        ])
        .await;

        let c = case(
            "merged",
            "merge_chunks:1,2",
            ExpectedOutcome::Merged {
                rows_merged: 1,
                surviving_chunk_id: 1,
                latency_ms_max: 500,
            },
        );
        let out = MemoryOpSimulator.simulate(&c, &kb).await.unwrap();
        assert!(out.passed, "expected pass; out={out:?}");
        assert_eq!(out.shadow.get("rows_merged").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(
            out.shadow.get("surviving_chunk_id").and_then(|v| v.as_i64()),
            Some(1)
        );
        assert!(out.error.is_none());
    }

    #[tokio::test]
    async fn simulate_returns_noop_when_target_missing() {
        let (_tmp, kb) = make_kb(&[
            "INSERT INTO chunks(id, file_id, chunk_index, content, namespace) VALUES (1, 1, 0, 'only', 'general');",
        ])
        .await;

        let c = case(
            "noop",
            "merge_chunks:1,99",
            ExpectedOutcome::NoOp { latency_ms_max: 500 },
        );
        let out = MemoryOpSimulator.simulate(&c, &kb).await.unwrap();
        assert!(out.passed, "expected pass; out={out:?}");
        assert_eq!(out.shadow.get("rows_merged").and_then(|v| v.as_u64()), Some(0));
    }

    #[tokio::test]
    async fn simulate_invalid_target_marks_failed() {
        let (_tmp, kb) = make_kb(&[
            "INSERT INTO chunks(id, file_id, chunk_index, content, namespace) VALUES (1, 1, 0, 'x', 'general');",
        ])
        .await;

        let c = case(
            "bad",
            "not_a_merge:1,2",
            ExpectedOutcome::NoOp { latency_ms_max: 500 },
        );
        let out = MemoryOpSimulator.simulate(&c, &kb).await.unwrap();
        assert!(!out.passed);
        assert!(out.error.is_some(), "expected error string, got {out:?}");
        assert!(out.baseline.is_empty());
        assert!(out.shadow.is_empty());
    }

    #[tokio::test]
    async fn simulate_records_baseline_and_shadow_keys() {
        let (_tmp, kb) = make_kb(&[
            "INSERT INTO chunks(id, file_id, chunk_index, content, namespace) VALUES (1, 1, 0, 'a', 'general');",
            "INSERT INTO chunks(id, file_id, chunk_index, content, namespace) VALUES (2, 1, 1, 'b', 'general');",
        ])
        .await;

        let c = case(
            "keys",
            "merge_chunks:1,2",
            ExpectedOutcome::Merged {
                rows_merged: 1,
                surviving_chunk_id: 1,
                latency_ms_max: 500,
            },
        );
        let out = MemoryOpSimulator.simulate(&c, &kb).await.unwrap();
        for k in ["chunks_total", "target_chunk_ids", "target_contents", "surviving_id_candidate"] {
            assert!(out.baseline.contains_key(k), "baseline missing {k}");
        }
        for k in ["chunks_total", "surviving_chunk_id", "rows_merged", "surviving_content"] {
            assert!(out.shadow.contains_key(k), "shadow missing {k}");
        }
    }
}
