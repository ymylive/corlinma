//! `/admin/scheduler*` — cron job listing + manual trigger + history.
//!
//! Sprint 6 T3. The scheduler runtime itself (tokio-cron-scheduler wiring,
//! `JobContext`) doesn't land until M7 — see
//! `corlinman-scheduler/src/cron.rs` + `src/jobs.rs`. To still offer a real
//! admin surface today, the routes operate on two sources of truth:
//!
//! 1. The static job definitions under `[[scheduler.jobs]]` in the active
//!    config. These are always visible so an operator can review what *will*
//!    run once the cron loop comes online.
//!
//! 2. An `in_memory_history` buffer kept on [`AdminState::scheduler_history`]
//!    — the M7 runtime will push `JobRun` records here; until then manual
//!    trigger attempts append a `source = "manual"` record with the current
//!    timestamp and the outcome `not_wired`.
//!
//! Routes:
//!
//! - `GET /admin/scheduler/jobs` — job definitions. Runtime data (next
//!   fire-time, last status) is marked `null` until the cron runtime lands.
//! - `POST /admin/scheduler/jobs/:name/trigger` — returns 501 with a clear
//!   message. Records the attempt in history so the UI can surface it.
//! - `GET /admin/scheduler/history` — latest 100 history entries
//!   (newest-first).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use serde_json::json;
use std::sync::{Arc, Mutex};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::AdminState;

pub fn router(state: AdminState) -> Router {
    Router::new()
        .route("/admin/scheduler/jobs", get(list_jobs))
        .route("/admin/scheduler/jobs/:name/trigger", post(trigger_job))
        .route("/admin/scheduler/history", get(list_history))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Shared history buffer (also exposed on AdminState)
// ---------------------------------------------------------------------------

/// One history entry. The cron runtime (M7) will push additional entries as
/// jobs fire + complete; for now only `trigger_job` populates it.
#[derive(Debug, Clone, Serialize)]
pub struct SchedulerHistoryEntry {
    pub job: String,
    pub at: String,
    pub source: &'static str,
    pub status: &'static str,
    pub message: String,
}

/// Ring-buffer style history store. Cap bounded to keep memory sane.
#[derive(Debug, Default)]
pub struct SchedulerHistory {
    inner: Mutex<Vec<SchedulerHistoryEntry>>,
}

impl SchedulerHistory {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn push(&self, entry: SchedulerHistoryEntry) {
        if let Ok(mut g) = self.inner.lock() {
            g.push(entry);
            let len = g.len();
            if len > 100 {
                g.drain(0..(len - 100));
            }
        }
    }

    pub fn snapshot(&self) -> Vec<SchedulerHistoryEntry> {
        self.inner.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// GET /admin/scheduler/jobs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct JobOut {
    name: String,
    cron: String,
    timezone: Option<String>,
    action_kind: &'static str,
    next_fire_at: Option<String>,
    last_status: Option<String>,
}

async fn list_jobs(State(state): State<AdminState>) -> Json<Vec<JobOut>> {
    let cfg = state.config.load_full();
    let rows = cfg
        .scheduler
        .jobs
        .iter()
        .map(|j| JobOut {
            name: j.name.clone(),
            cron: j.cron.clone(),
            timezone: j.timezone.clone(),
            action_kind: action_kind(&j.action),
            next_fire_at: None,
            last_status: None,
        })
        .collect();
    Json(rows)
}

fn action_kind(a: &corlinman_core::config::JobAction) -> &'static str {
    match a {
        corlinman_core::config::JobAction::RunAgent { .. } => "run_agent",
        corlinman_core::config::JobAction::RunTool { .. } => "run_tool",
        corlinman_core::config::JobAction::Subprocess { .. } => "subprocess",
    }
}

// ---------------------------------------------------------------------------
// POST /admin/scheduler/jobs/:name/trigger
// ---------------------------------------------------------------------------

async fn trigger_job(State(state): State<AdminState>, Path(name): Path<String>) -> Response {
    let cfg = state.config.load_full();
    let known = cfg.scheduler.jobs.iter().any(|j| j.name == name);
    if !known {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "not_found", "resource": "scheduler_job", "id": name})),
        )
            .into_response();
    }

    // Record the attempt so the UI sees a trail even in the not-wired state.
    let entry = SchedulerHistoryEntry {
        job: name.clone(),
        at: OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "".into()),
        source: "manual",
        status: "not_wired",
        message: "scheduler runtime lands in M7; trigger was recorded but not executed".into(),
    };
    if let Some(h) = state.scheduler_history.as_ref() {
        h.push(entry.clone());
    }

    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "error": "scheduler_not_wired",
            "message": "cron runtime is not yet wired (M7); trigger attempt recorded in history",
            "recorded": entry,
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// GET /admin/scheduler/history
// ---------------------------------------------------------------------------

async fn list_history(State(state): State<AdminState>) -> Json<Vec<SchedulerHistoryEntry>> {
    let Some(h) = state.scheduler_history.as_ref() else {
        return Json(Vec::new());
    };
    // Newest-first.
    let mut snap = h.snapshot();
    snap.reverse();
    Json(snap)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use arc_swap::ArcSwap;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use corlinman_core::config::{Config, JobAction, SchedulerJob};
    use corlinman_plugins::registry::PluginRegistry;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn cfg_with_jobs() -> Config {
        let mut cfg = Config::default();
        cfg.scheduler.jobs = vec![
            SchedulerJob {
                name: "daily-summary".into(),
                cron: "0 0 0 * * *".into(),
                timezone: Some("Asia/Shanghai".into()),
                action: JobAction::RunAgent {
                    prompt: "summarise today".into(),
                },
            },
            SchedulerJob {
                name: "cleanup".into(),
                cron: "0 */10 * * * *".into(),
                timezone: None,
                action: JobAction::RunTool {
                    plugin: "file-ops".into(),
                    tool: "cleanup".into(),
                    args: serde_json::json!({}),
                },
            },
        ];
        cfg
    }

    fn state_with(cfg: Config, history: Option<Arc<SchedulerHistory>>) -> AdminState {
        let mut s = AdminState::new(
            Arc::new(PluginRegistry::default()),
            Arc::new(ArcSwap::from_pointee(cfg)),
        );
        if let Some(h) = history {
            s = s.with_scheduler_history(h);
        }
        s
    }

    async fn body_json(resp: Response) -> serde_json::Value {
        let b = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&b).unwrap()
    }

    #[tokio::test]
    async fn list_jobs_returns_config_defined_jobs() {
        let state = state_with(cfg_with_jobs(), None);
        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/scheduler/jobs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["name"], "daily-summary");
        assert_eq!(arr[0]["action_kind"], "run_agent");
        assert_eq!(arr[1]["action_kind"], "run_tool");
        assert!(arr[0]["next_fire_at"].is_null());
    }

    #[tokio::test]
    async fn trigger_returns_404_for_unknown_job() {
        let state = state_with(cfg_with_jobs(), Some(SchedulerHistory::new()));
        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/scheduler/jobs/nope/trigger")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn trigger_records_history_even_when_not_wired() {
        let history = SchedulerHistory::new();
        let state = state_with(cfg_with_jobs(), Some(history.clone()));
        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/scheduler/jobs/daily-summary/trigger")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_IMPLEMENTED);
        let snap = history.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].job, "daily-summary");
        assert_eq!(snap[0].status, "not_wired");
        assert_eq!(snap[0].source, "manual");
    }

    #[tokio::test]
    async fn history_returns_newest_first() {
        let history = SchedulerHistory::new();
        history.push(SchedulerHistoryEntry {
            job: "a".into(),
            at: "2026-04-20T00:00:00Z".into(),
            source: "manual",
            status: "ok",
            message: "1".into(),
        });
        history.push(SchedulerHistoryEntry {
            job: "b".into(),
            at: "2026-04-20T01:00:00Z".into(),
            source: "cron",
            status: "ok",
            message: "2".into(),
        });
        let state = state_with(cfg_with_jobs(), Some(history));
        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/scheduler/history")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["job"], "b");
        assert_eq!(arr[1]["job"], "a");
    }
}
