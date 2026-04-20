//! `/admin/models*` — model routing / alias management.
//!
//! Sprint 6 T5. Two routes:
//!
//! - `GET /admin/models` — one snapshot of the active `[providers.*]` slots
//!   (with secrets redacted) plus the `[models]` `default` + `aliases` map.
//!   The UI renders a providers table (enabled toggle is read-only here;
//!   flipping it lives behind the config editor) and an alias grid.
//!
//! - `POST /admin/models/aliases` — CRUD for `models.aliases`. Body is the
//!   full desired map:
//!   ```json
//!   { "aliases": {"smart": "claude-opus-4-7", "fast": "claude-haiku"}, "default": "claude-sonnet-4-5" }
//!   ```
//!   Handler validates that every aliased target + `default` are non-empty
//!   strings, then clones the current [`Config`], swaps the `models`
//!   sub-section, writes the file atomically (shared helper with
//!   `admin/config.rs`), and stores the new snapshot in `ArcSwap`.
//!
//!   When `config_path` is missing (stripped-down test harness) the write
//!   stage returns 503 `config_path_unset`, identical to `POST /admin/config`.
//!
//! All routes live behind the shared admin auth middleware mounted in
//! [`super::router_with_state`].

use std::collections::HashMap;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use corlinman_core::config::{Config, ProviderEntry, SecretRef};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::AdminState;

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router(state: AdminState) -> Router {
    Router::new()
        .route("/admin/models", get(list_models))
        .route("/admin/models/aliases", post(update_aliases))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// GET /admin/models
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ProviderRow {
    name: &'static str,
    enabled: bool,
    has_api_key: bool,
    api_key_kind: Option<&'static str>,
    base_url: Option<String>,
}

impl ProviderRow {
    fn from_entry(name: &'static str, entry: &ProviderEntry) -> Self {
        let (has_api_key, api_key_kind) = match entry.api_key.as_ref() {
            None => (false, None),
            Some(SecretRef::EnvVar { .. }) => (true, Some("env")),
            Some(SecretRef::Literal { .. }) => (true, Some("literal")),
        };
        Self {
            name,
            enabled: entry.enabled,
            has_api_key,
            api_key_kind,
            base_url: entry.base_url.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ModelsResponse {
    default: String,
    aliases: HashMap<String, String>,
    providers: Vec<ProviderRow>,
}

async fn list_models(State(state): State<AdminState>) -> Json<ModelsResponse> {
    let cfg = state.config.load_full();
    let providers: Vec<ProviderRow> = cfg
        .providers
        .iter()
        .map(|(n, e)| ProviderRow::from_entry(n, e))
        .collect();
    Json(ModelsResponse {
        default: cfg.models.default.clone(),
        aliases: cfg.models.aliases.clone(),
        providers,
    })
}

// ---------------------------------------------------------------------------
// POST /admin/models/aliases
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AliasesBody {
    /// Full desired alias map (replaces, not merges — drop an entry by
    /// omitting it, add by including it).
    pub aliases: HashMap<String, String>,
    /// If provided, overrides `models.default`. Empty string rejected.
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Serialize)]
struct AliasesOut {
    status: &'static str,
    default: String,
    aliases: HashMap<String, String>,
}

async fn update_aliases(
    State(state): State<AdminState>,
    Json(body): Json<AliasesBody>,
) -> Response {
    // Validate: no empty names/targets.
    for (k, v) in &body.aliases {
        if k.is_empty() || v.is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "invalid_alias",
                    "message": "alias name and target must be non-empty",
                })),
            )
                .into_response();
        }
    }
    if let Some(d) = body.default.as_ref() {
        if d.is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "invalid_default",
                    "message": "default model must be non-empty",
                })),
            )
                .into_response();
        }
    }

    // Build new config snapshot.
    let mut new_cfg: Config = (*state.config.load_full()).clone();
    new_cfg.models.aliases = body.aliases.clone();
    if let Some(d) = body.default.clone() {
        new_cfg.models.default = d;
    }

    // Persist — mirrors the atomic write in `admin/config.rs`.
    let Some(path) = state.config_path.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "config_path_unset",
                "message": "gateway booted without a config file path",
            })),
        )
            .into_response();
    };

    let serialised = match toml::to_string_pretty(&new_cfg) {
        Ok(s) => s,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "serialise_failed",
                    "message": err.to_string(),
                })),
            )
                .into_response();
        }
    };

    if let Err(err) = atomic_write(path, &serialised).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "write_failed",
                "message": err.to_string(),
            })),
        )
            .into_response();
    }

    state.config.store(std::sync::Arc::new(new_cfg.clone()));
    Json(AliasesOut {
        status: "ok",
        default: new_cfg.models.default,
        aliases: new_cfg.models.aliases,
    })
    .into_response()
}

async fn atomic_write(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut tmp = path.to_path_buf();
    tmp.as_mut_os_string().push(".new");
    tokio::fs::write(&tmp, contents).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
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
    use corlinman_core::config::{Config, ProviderEntry, SecretRef};
    use corlinman_plugins::registry::PluginRegistry;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tower::ServiceExt;

    fn base_state(path: Option<std::path::PathBuf>) -> AdminState {
        let mut cfg = Config::default();
        cfg.providers.anthropic = Some(ProviderEntry {
            api_key: Some(SecretRef::EnvVar {
                env: "ANTHROPIC_API_KEY".into(),
            }),
            base_url: None,
            enabled: true,
        });
        cfg.providers.openai = Some(ProviderEntry {
            api_key: None,
            base_url: Some("https://openai.example".into()),
            enabled: false,
        });
        cfg.models
            .aliases
            .insert("smart".into(), "claude-opus-4-7".into());

        let mut state = AdminState::new(
            Arc::new(PluginRegistry::default()),
            Arc::new(ArcSwap::from_pointee(cfg)),
        );
        if let Some(p) = path {
            state = state.with_config_path(p);
        }
        state
    }

    async fn body_json(resp: Response) -> serde_json::Value {
        let b = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&b).unwrap()
    }

    #[tokio::test]
    async fn list_returns_providers_and_aliases() {
        let state = base_state(None);
        let app = router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/admin/models")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["default"], "claude-sonnet-4-5");
        assert_eq!(v["aliases"]["smart"], "claude-opus-4-7");
        let providers = v["providers"].as_array().unwrap();
        assert!(providers
            .iter()
            .any(|p| p["name"] == "anthropic" && p["enabled"] == true));
        assert!(providers
            .iter()
            .any(|p| p["name"] == "openai" && p["has_api_key"] == false));
    }

    #[tokio::test]
    async fn update_aliases_writes_config_and_swaps_snapshot() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        let state = base_state(Some(path.clone()));
        let app = router(state.clone());
        let body = serde_json::to_string(&serde_json::json!({
            "aliases": {"fast": "claude-haiku", "smart": "claude-opus-4-7"},
            "default": "claude-opus-4-7",
        }))
        .unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/models/aliases")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["default"], "claude-opus-4-7");
        assert_eq!(v["aliases"]["fast"], "claude-haiku");

        // Snapshot updated.
        let live = state.config.load();
        assert_eq!(live.models.default, "claude-opus-4-7");
        assert_eq!(live.models.aliases.get("fast").unwrap(), "claude-haiku");
        // File persisted.
        assert!(path.exists());
        let text = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(text.contains("claude-haiku"));
    }

    #[tokio::test]
    async fn update_aliases_rejects_empty_alias() {
        let tmp = TempDir::new().unwrap();
        let state = base_state(Some(tmp.path().join("config.toml")));
        let app = router(state);
        let body = serde_json::to_string(&serde_json::json!({
            "aliases": {"": "x"},
        }))
        .unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/models/aliases")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn update_aliases_returns_503_without_config_path() {
        let state = base_state(None);
        let app = router(state);
        let body = serde_json::to_string(&serde_json::json!({
            "aliases": {"a": "b"},
        }))
        .unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/models/aliases")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
