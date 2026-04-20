//! `GET /admin/plugins` + `GET /admin/plugins/:name`.
//!
//! Read-only views onto the plugin registry. The UI consumes these on the
//! Plugins page (list table → row detail drawer).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use corlinman_plugins::manifest::PluginType;
use corlinman_plugins::registry::{Diagnostic, PluginEntry};
use serde::Serialize;
use serde_json::{json, Value};

use super::AdminState;

/// One row in the admin plugins table.
///
/// Field names are deliberately chosen to match the existing
/// `ui/lib/api.ts::PluginSummary` surface so the UI does not need a
/// migration on M6 cutover.
#[derive(Debug, Serialize)]
pub struct PluginSummaryOut {
    pub name: String,
    pub version: String,
    pub status: &'static str,
    pub plugin_type: &'static str,
    pub origin: &'static str,
    pub tool_count: usize,
    pub manifest_path: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub shadowed_count: usize,
}

impl From<&PluginEntry> for PluginSummaryOut {
    fn from(entry: &PluginEntry) -> Self {
        let m = &entry.manifest;
        Self {
            name: m.name.clone(),
            version: m.version.clone(),
            // Status is always "loaded" for M6 — the registry only stores
            // successfully-parsed manifests. Disabled / error states arrive
            // once we track per-plugin health + config-driven disables.
            status: "loaded",
            plugin_type: plugin_type_str(m.plugin_type),
            origin: entry.origin.as_str(),
            tool_count: m.capabilities.tools.len(),
            manifest_path: entry.manifest_path.to_string_lossy().into_owned(),
            description: m.description.clone(),
            capabilities: m
                .capabilities
                .tools
                .iter()
                .map(|t| t.name.clone())
                .collect(),
            shadowed_count: entry.shadowed_count,
        }
    }
}

fn plugin_type_str(t: PluginType) -> &'static str {
    match t {
        PluginType::Sync => "sync",
        PluginType::Async => "async",
        PluginType::Service => "service",
    }
}

/// Sub-router for `/admin/plugins*`. Consumes [`AdminState`] via axum's
/// typed `State` extractor.
pub fn router(state: AdminState) -> Router {
    Router::new()
        .route("/admin/plugins", get(list_plugins))
        .route("/admin/plugins/:name", get(get_plugin))
        .with_state(state)
}

async fn list_plugins(State(state): State<AdminState>) -> Json<Vec<PluginSummaryOut>> {
    let rows: Vec<PluginSummaryOut> = state.plugins.list().iter().map(|e| (*e).into()).collect();
    Json(rows)
}

/// Response for `GET /admin/plugins/:name`.
#[derive(Debug, Serialize)]
struct PluginDetail {
    summary: PluginSummaryOut,
    /// Full TOML-decoded manifest (serialised back out as JSON so the UI can
    /// render arbitrary schemas without a typed client).
    manifest: Value,
    diagnostics: Vec<Value>,
}

async fn get_plugin(
    State(state): State<AdminState>,
    Path(name): Path<String>,
) -> axum::response::Response {
    let Some(entry) = state.plugins.get(&name) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "not_found",
                "resource": "plugin",
                "id": name,
            })),
        )
            .into_response();
    };

    let manifest_json = match serde_json::to_value(&*entry.manifest) {
        Ok(v) => v,
        Err(err) => {
            tracing::error!(error = %err, plugin = %name, "manifest -> json failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "manifest_serialise_failed",
                    "detail": err.to_string(),
                })),
            )
                .into_response();
        }
    };

    // Only surface diagnostics that mention this plugin; keeps the payload
    // small and avoids leaking unrelated collisions.
    let diagnostics: Vec<Value> = state
        .plugins
        .diagnostics()
        .iter()
        .filter_map(|d| diagnostic_for(&name, d))
        .collect();

    Json(PluginDetail {
        summary: entry.into(),
        manifest: manifest_json,
        diagnostics,
    })
    .into_response()
}

fn diagnostic_for(plugin: &str, d: &Diagnostic) -> Option<Value> {
    match d {
        Diagnostic::ParseError {
            path,
            origin,
            message,
        } => {
            // Path-based match: the UI wants to see parse failures for plugins
            // whose directory name matches, even though the registry never
            // successfully created an entry for them.
            let matches = path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy() == plugin)
                .unwrap_or(false);
            matches.then(|| {
                json!({
                    "kind": "parse_error",
                    "path": path.to_string_lossy(),
                    "origin": origin.as_str(),
                    "message": message,
                })
            })
        }
        Diagnostic::NameCollision {
            name,
            winner,
            winner_origin,
            loser,
            loser_origin,
        } => (name == plugin).then(|| {
            json!({
                "kind": "name_collision",
                "winner": winner.to_string_lossy(),
                "winner_origin": winner_origin.as_str(),
                "loser": loser.to_string_lossy(),
                "loser_origin": loser_origin.as_str(),
            })
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arc_swap::ArcSwap;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use corlinman_core::config::Config;
    use corlinman_plugins::discovery::{Origin, SearchRoot};
    use corlinman_plugins::registry::PluginRegistry;
    use std::fs;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn manifest_body(name: &str, version: &str) -> String {
        format!(
            "name = \"{name}\"\n\
             version = \"{version}\"\n\
             description = \"scratch plugin\"\n\
             plugin_type = \"sync\"\n\
             [entry_point]\n\
             command = \"true\"\n\
             [[capabilities.tools]]\n\
             name = \"echo\"\n\
             description = \"echo its input\"\n"
        )
    }

    fn scratch_registry() -> (tempfile::TempDir, Arc<PluginRegistry>) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("alpha");
        fs::create_dir_all(&p).unwrap();
        fs::write(
            p.join("plugin-manifest.toml"),
            manifest_body("alpha", "1.2.3"),
        )
        .unwrap();

        let reg = PluginRegistry::from_roots(vec![SearchRoot::new(dir.path(), Origin::Workspace)]);
        (dir, Arc::new(reg))
    }

    fn app(registry: Arc<PluginRegistry>) -> Router {
        let state = AdminState::new(registry, Arc::new(ArcSwap::from_pointee(Config::default())));
        router(state)
    }

    async fn body_json(resp: axum::response::Response) -> Value {
        let b = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&b).unwrap()
    }

    #[tokio::test]
    async fn list_returns_registry_entries() {
        let (_dir, reg) = scratch_registry();
        let resp = app(reg)
            .oneshot(
                Request::builder()
                    .uri("/admin/plugins")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "alpha");
        assert_eq!(arr[0]["version"], "1.2.3");
        assert_eq!(arr[0]["plugin_type"], "sync");
        assert_eq!(arr[0]["origin"], "workspace");
        assert_eq!(arr[0]["tool_count"], 1);
        assert_eq!(arr[0]["capabilities"], json!(["echo"]));
        assert_eq!(arr[0]["status"], "loaded");
    }

    #[tokio::test]
    async fn detail_returns_manifest_and_summary() {
        let (_dir, reg) = scratch_registry();
        let resp = app(reg)
            .oneshot(
                Request::builder()
                    .uri("/admin/plugins/alpha")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["summary"]["name"], "alpha");
        assert_eq!(v["manifest"]["name"], "alpha");
        assert_eq!(v["manifest"]["capabilities"]["tools"][0]["name"], "echo");
        assert!(v["diagnostics"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn detail_returns_404_for_unknown_plugin() {
        let (_dir, reg) = scratch_registry();
        let resp = app(reg)
            .oneshot(
                Request::builder()
                    .uri("/admin/plugins/nope")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let v = body_json(resp).await;
        assert_eq!(v["error"], "not_found");
        assert_eq!(v["id"], "nope");
    }

    #[test]
    fn diagnostic_filter_matches_by_dir_name() {
        let d = Diagnostic::ParseError {
            path: "/tmp/plugins/alpha/plugin-manifest.toml".into(),
            origin: Origin::Workspace,
            message: "bad".into(),
        };
        assert!(diagnostic_for("alpha", &d).is_some());
        assert!(diagnostic_for("beta", &d).is_none());
    }
}
