//! `GET /admin/agents` — filesystem scan of `<data_dir>/agents/*.md`.
//!
//! M6 narrow scope: metadata only (name, relative path, size, mtime). We
//! deliberately skip frontmatter / content parsing — a future endpoint can
//! stream full content on demand once the UI needs it.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use axum::{
    extract::{Path as AxPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use walkdir::WalkDir;

use super::AdminState;

#[derive(Debug, Serialize)]
pub struct AgentSummaryOut {
    pub name: String,
    pub file_path: String,
    pub bytes: u64,
    pub last_modified: Option<String>,
}

pub fn router(state: AdminState) -> Router {
    Router::new()
        .route("/admin/agents", get(list_agents))
        .route("/admin/agents/:name", get(get_agent).post(save_agent))
        .with_state(state)
}

async fn list_agents(State(state): State<AdminState>) -> Json<Vec<AgentSummaryOut>> {
    let cfg = state.config.load();
    let agents_dir = cfg.server.data_dir.join("agents");
    Json(scan_agents(&agents_dir))
}

/// Walk the `agents/` directory (shallow) and return each `*.md` as an
/// [`AgentSummaryOut`]. Missing directory → empty list (not an error), so a
/// fresh install that hasn't created any agents yet still renders an empty
/// admin table cleanly.
fn scan_agents(dir: &Path) -> Vec<AgentSummaryOut> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for entry in WalkDir::new(dir)
        .max_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        if p.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let Some(name) = p.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        out.push(AgentSummaryOut {
            name: name.to_string(),
            file_path: rel_path_str(dir, p),
            bytes: meta.len(),
            last_modified: meta.modified().ok().and_then(system_time_to_rfc3339),
        });
    }
    // Deterministic order so the UI table is stable across refreshes.
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Render `full` as a path relative to `base` when possible, falling back to
/// the absolute path. The UI prefers the short form for display.
fn rel_path_str(base: &Path, full: &Path) -> String {
    full.strip_prefix(base)
        .map(|p| PathBuf::from("agents").join(p))
        .unwrap_or_else(|_| full.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn system_time_to_rfc3339(t: SystemTime) -> Option<String> {
    OffsetDateTime::from(t).format(&Rfc3339).ok()
}

// ---------------------------------------------------------------------------
// GET /admin/agents/:name  — Sprint 6 T6.
// ---------------------------------------------------------------------------

/// Full content payload for a single agent markdown file.
#[derive(Debug, Serialize)]
struct AgentContent {
    name: String,
    file_path: String,
    bytes: u64,
    last_modified: Option<String>,
    /// Raw UTF-8 body (frontmatter + markdown). The UI's Monaco editor
    /// consumes this verbatim.
    content: String,
}

async fn get_agent(State(state): State<AdminState>, AxPath(name): AxPath<String>) -> Response {
    let cfg = state.config.load();
    let agents_dir = cfg.server.data_dir.join("agents");
    let path = match resolve_agent_path(&agents_dir, &name) {
        Ok(p) => p,
        Err(err) => return *err,
    };
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let content = match String::from_utf8(bytes.clone()) {
                Ok(s) => s,
                Err(err) => {
                    return (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(json!({
                            "error": "not_utf8",
                            "message": err.to_string(),
                        })),
                    )
                        .into_response();
                }
            };
            let meta = tokio::fs::metadata(&path).await.ok();
            Json(AgentContent {
                name: name.clone(),
                file_path: rel_path_str(&agents_dir, &path),
                bytes: meta
                    .as_ref()
                    .map(|m| m.len())
                    .unwrap_or(content.len() as u64),
                last_modified: meta
                    .and_then(|m| m.modified().ok())
                    .and_then(system_time_to_rfc3339),
                content,
            })
            .into_response()
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "not_found", "resource": "agent", "id": name})),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "read_failed", "message": err.to_string()})),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// POST /admin/agents/:name — save edited agent markdown.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SaveAgentBody {
    /// Full replacement body. We don't support partial / frontmatter-only
    /// patches — the UI posts the whole file.
    pub content: String,
}

async fn save_agent(
    State(state): State<AdminState>,
    AxPath(name): AxPath<String>,
    Json(body): Json<SaveAgentBody>,
) -> Response {
    let cfg = state.config.load();
    let agents_dir = cfg.server.data_dir.join("agents");
    // Allow creating new agents via POST (convenient for tests + onboard flow).
    let path = match agent_path_or_build(&agents_dir, &name) {
        Ok(p) => p,
        Err(err) => return *err,
    };
    if let Err(err) = tokio::fs::create_dir_all(&agents_dir).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "mkdir_failed", "message": err.to_string()})),
        )
            .into_response();
    }
    // Atomic write: .new + rename.
    let mut tmp = path.clone();
    tmp.as_mut_os_string().push(".new");
    if let Err(err) = tokio::fs::write(&tmp, body.content.as_bytes()).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "write_failed", "message": err.to_string()})),
        )
            .into_response();
    }
    if let Err(err) = tokio::fs::rename(&tmp, &path).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "rename_failed", "message": err.to_string()})),
        )
            .into_response();
    }
    let meta = tokio::fs::metadata(&path).await.ok();
    Json(json!({
        "status": "ok",
        "name": name,
        "file_path": rel_path_str(&agents_dir, &path),
        "bytes": meta.as_ref().map(|m| m.len()).unwrap_or(body.content.len() as u64),
        "last_modified": meta
            .and_then(|m| m.modified().ok())
            .and_then(system_time_to_rfc3339),
    }))
    .into_response()
}

/// Construct `<agents_dir>/<name>.md` and reject any attempt at directory
/// traversal or a name carrying its own extension. `Response` is boxed so
/// the `Result` stays cheap to move around (clippy::result_large_err).
fn agent_path_or_build(agents_dir: &Path, name: &str) -> Result<PathBuf, Box<Response>> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(Box::new(
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "invalid_name",
                    "message": "agent name must be a bare stem without path separators or '..'",
                })),
            )
                .into_response(),
        ));
    }
    Ok(agents_dir.join(format!("{name}.md")))
}

/// Like [`agent_path_or_build`] but also asserts the file already exists.
fn resolve_agent_path(agents_dir: &Path, name: &str) -> Result<PathBuf, Box<Response>> {
    let p = agent_path_or_build(agents_dir, name)?;
    if !p.is_file() {
        return Err(Box::new(
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "not_found", "resource": "agent", "id": name})),
            )
                .into_response(),
        ));
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arc_swap::ArcSwap;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use corlinman_core::config::Config;
    use corlinman_plugins::registry::PluginRegistry;
    use serde_json::Value;
    use std::fs;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn app_with_data_dir(data_dir: &Path) -> Router {
        let mut cfg = Config::default();
        cfg.server.data_dir = data_dir.to_path_buf();
        let state = AdminState::new(
            Arc::new(PluginRegistry::default()),
            Arc::new(ArcSwap::from_pointee(cfg)),
        );
        router(state)
    }

    async fn get_json(app: Router) -> Value {
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/agents")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let b = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&b).unwrap()
    }

    #[tokio::test]
    async fn missing_agents_dir_returns_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        // Note: no `agents/` subdir created.
        let v = get_json(app_with_data_dir(dir.path())).await;
        assert_eq!(v, serde_json::json!([]));
    }

    #[tokio::test]
    async fn lists_md_files_sorted_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let agents = dir.path().join("agents");
        fs::create_dir_all(&agents).unwrap();
        fs::write(agents.join("Beta.md"), "# beta\n").unwrap();
        fs::write(agents.join("Alpha.md"), "hello world").unwrap();
        // Non-md files should be ignored.
        fs::write(agents.join("README.txt"), "skip me").unwrap();

        let v = get_json(app_with_data_dir(dir.path())).await;
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["name"], "Alpha");
        assert_eq!(arr[1]["name"], "Beta");
        assert_eq!(arr[0]["bytes"], 11); // "hello world"
        assert!(arr[0]["file_path"].as_str().unwrap().contains("Alpha.md"));
        assert!(arr[0]["last_modified"].is_string());
    }

    #[tokio::test]
    async fn get_agent_returns_file_content() {
        let dir = tempfile::tempdir().unwrap();
        let agents = dir.path().join("agents");
        fs::create_dir_all(&agents).unwrap();
        fs::write(agents.join("Alpha.md"), "---\ntitle: hi\n---\nbody").unwrap();
        let app = app_with_data_dir(dir.path());
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/admin/agents/Alpha")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let b = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
        assert_eq!(v["name"], "Alpha");
        assert!(v["content"].as_str().unwrap().contains("title: hi"));
    }

    #[tokio::test]
    async fn get_agent_returns_404_for_missing() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with_data_dir(dir.path());
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/admin/agents/Nope")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn save_agent_writes_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with_data_dir(dir.path());
        let body = serde_json::to_string(&serde_json::json!({
            "content": "---\ntitle: new\n---\nhello",
        }))
        .unwrap();
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/admin/agents/Brand")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let path = dir.path().join("agents").join("Brand.md");
        assert!(path.is_file());
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("title: new"));
    }

    #[tokio::test]
    async fn save_agent_rejects_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with_data_dir(dir.path());
        let body = serde_json::to_string(&serde_json::json!({"content": "x"})).unwrap();
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/admin/agents/..%2Fevil")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn ignores_nested_subdirectories() {
        let dir = tempfile::tempdir().unwrap();
        let agents = dir.path().join("agents");
        let nested = agents.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(agents.join("Top.md"), "x").unwrap();
        fs::write(nested.join("Inner.md"), "y").unwrap();

        let v = get_json(app_with_data_dir(dir.path())).await;
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "Top");
    }
}
