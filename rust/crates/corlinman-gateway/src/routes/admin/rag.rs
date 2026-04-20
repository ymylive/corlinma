//! `/admin/rag*` — RAG corpus stats + debug query + rebuild.
//!
//! Sprint 6 T1. Three routes:
//!
//! - `GET /admin/rag/stats` — total files / chunks / distinct-tags and a
//!   boolean `ready` flag. Backed by the in-process [`SqliteStore`] handle
//!   attached to [`AdminState::rag_store`]. When no store is attached the
//!   route returns 503 `rag_disabled` — identical pattern to
//!   `/admin/approvals*` (see [`super::approvals`]).
//!
//! - `GET /admin/rag/query?q=&k=10` — BM25 debug query. Returns the top-k
//!   chunks by FTS5 `bm25()` so an operator can sanity-check the corpus
//!   without standing up the full embedding + HNSW pipeline. Dense + hybrid
//!   search requires an embedder; the UI page makes the BM25-only scope
//!   explicit.
//!
//! - `POST /admin/rag/rebuild` — rebuild the `chunks_fts` virtual table
//!   (`INSERT INTO chunks_fts(chunks_fts) VALUES('rebuild')`). The FTS5
//!   rebuild is an in-process synchronous command that completes quickly
//!   enough for an admin action (seconds at most on a desktop corpus).
//!   Returns `{status: "ok"}` on success, 500 otherwise. The "async job"
//!   framing in the spec is deferred until a full background-job runtime
//!   lands in M7 — the current implementation is still real, just
//!   synchronous.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use corlinman_vector::SqliteStore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

use super::AdminState;

pub fn router(state: AdminState) -> Router {
    Router::new()
        .route("/admin/rag/stats", get(stats))
        .route("/admin/rag/query", get(query))
        .route("/admin/rag/rebuild", post(rebuild))
        .with_state(state)
}

fn rag_disabled() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "error": "rag_disabled",
            "message": "RAG store is not attached to this gateway",
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// GET /admin/rag/stats
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct StatsOut {
    ready: bool,
    files: i64,
    chunks: i64,
    tags: i64,
}

async fn stats(State(state): State<AdminState>) -> Response {
    let Some(store) = state.rag_store.as_ref() else {
        return rag_disabled();
    };
    match gather_stats(store.clone()).await {
        Ok(out) => Json(out).into_response(),
        Err(err) => {
            tracing::warn!(error = %err, "admin/rag/stats failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "storage_error",
                    "message": err.to_string(),
                })),
            )
                .into_response()
        }
    }
}

async fn gather_stats(store: Arc<SqliteStore>) -> anyhow::Result<StatsOut> {
    let files = store.count_files().await?;
    let chunks = store.count_chunks().await?;
    let tags = store.count_tags().await?;
    Ok(StatsOut {
        ready: true,
        files,
        chunks,
        tags,
    })
}

// ---------------------------------------------------------------------------
// GET /admin/rag/query
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct QueryParams {
    q: String,
    #[serde(default = "default_k")]
    k: usize,
}

fn default_k() -> usize {
    10
}

#[derive(Debug, Serialize)]
struct QueryHitOut {
    chunk_id: i64,
    score: f32,
    content_preview: String,
}

#[derive(Debug, Serialize)]
struct QueryResponseOut {
    backend: &'static str,
    q: String,
    k: usize,
    hits: Vec<QueryHitOut>,
}

const PREVIEW_LEN: usize = 240;

async fn query(State(state): State<AdminState>, Query(params): Query<QueryParams>) -> Response {
    let Some(store) = state.rag_store.as_ref() else {
        return rag_disabled();
    };
    if params.q.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "invalid_query",
                "message": "q must be non-empty",
            })),
        )
            .into_response();
    }
    let k = params.k.clamp(1, 100);
    match run_bm25(store.clone(), &params.q, k).await {
        Ok(hits) => Json(QueryResponseOut {
            backend: "bm25",
            q: params.q,
            k,
            hits,
        })
        .into_response(),
        Err(err) => {
            tracing::warn!(error = %err, "admin/rag/query failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "storage_error",
                    "message": err.to_string(),
                })),
            )
                .into_response()
        }
    }
}

async fn run_bm25(store: Arc<SqliteStore>, q: &str, k: usize) -> anyhow::Result<Vec<QueryHitOut>> {
    let raw = store.search_bm25(q, k).await?;
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<i64> = raw.iter().map(|(id, _)| *id).collect();
    let chunks = store.query_chunks_by_ids(&ids).await?;
    let by_id: std::collections::HashMap<i64, String> =
        chunks.into_iter().map(|c| (c.id, c.content)).collect();
    Ok(raw
        .into_iter()
        .map(|(id, score)| {
            let content = by_id.get(&id).cloned().unwrap_or_default();
            let preview = truncate_preview(&content);
            QueryHitOut {
                chunk_id: id,
                score,
                content_preview: preview,
            }
        })
        .collect())
}

fn truncate_preview(s: &str) -> String {
    if s.chars().count() <= PREVIEW_LEN {
        return s.to_string();
    }
    let mut out: String = s.chars().take(PREVIEW_LEN).collect();
    out.push('…');
    out
}

// ---------------------------------------------------------------------------
// POST /admin/rag/rebuild
// ---------------------------------------------------------------------------

async fn rebuild(State(state): State<AdminState>) -> Response {
    let Some(store) = state.rag_store.as_ref() else {
        return rag_disabled();
    };
    match store.rebuild_fts().await {
        Ok(()) => Json(json!({"status": "ok", "target": "chunks_fts"})).into_response(),
        Err(err) => {
            tracing::error!(error = %err, "admin/rag/rebuild failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "rebuild_failed",
                    "message": err.to_string(),
                })),
            )
                .into_response()
        }
    }
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
    use corlinman_core::config::Config;
    use corlinman_plugins::registry::PluginRegistry;
    use corlinman_vector::SqliteStore;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tower::ServiceExt;

    async fn empty_store() -> (Arc<SqliteStore>, TempDir) {
        let tmp = TempDir::new().unwrap();
        let store = SqliteStore::open(&tmp.path().join("kb.sqlite"))
            .await
            .unwrap();
        corlinman_vector::migration::ensure_schema(&store)
            .await
            .unwrap();
        (Arc::new(store), tmp)
    }

    fn app(rag: Option<Arc<SqliteStore>>) -> Router {
        let mut state = AdminState::new(
            Arc::new(PluginRegistry::default()),
            Arc::new(ArcSwap::from_pointee(Config::default())),
        );
        if let Some(s) = rag {
            state = state.with_rag_store(s);
        }
        router(state)
    }

    async fn body_json(resp: Response) -> serde_json::Value {
        let b = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&b).unwrap()
    }

    #[tokio::test]
    async fn stats_returns_503_without_store() {
        let app = app(None);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/rag/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn stats_returns_zeroes_on_fresh_store() {
        let (store, _tmp) = empty_store().await;
        let app = app(Some(store));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/rag/stats")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["ready"], true);
        assert_eq!(v["files"], 0);
        assert_eq!(v["chunks"], 0);
        assert_eq!(v["tags"], 0);
    }

    #[tokio::test]
    async fn query_rejects_empty_q() {
        let (store, _tmp) = empty_store().await;
        let app = app(Some(store));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/rag/query?q=&k=5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn query_returns_empty_hits_on_fresh_store() {
        let (store, _tmp) = empty_store().await;
        let app = app(Some(store));
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/rag/query?q=hello&k=5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["backend"], "bm25");
        assert_eq!(v["k"], 5);
        assert_eq!(v["hits"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn rebuild_returns_ok_on_empty_store() {
        let (store, _tmp) = empty_store().await;
        let app = app(Some(store));
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/rag/rebuild")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["status"], "ok");
    }

    #[tokio::test]
    async fn rebuild_without_store_returns_503() {
        let app = app(None);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/rag/rebuild")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
