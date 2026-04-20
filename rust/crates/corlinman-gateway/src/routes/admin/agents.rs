//! `GET /admin/agents` — filesystem scan of `<data_dir>/agents/*.md`.
//!
//! M6 narrow scope: metadata only (name, relative path, size, mtime). We
//! deliberately skip frontmatter / content parsing — a future endpoint can
//! stream full content on demand once the UI needs it.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;
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
