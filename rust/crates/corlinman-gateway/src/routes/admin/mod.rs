//! `/admin/*` REST endpoints — narrow M6 scope.
//!
//! This module ships three read-only endpoints backed by real state:
//!   - `GET /admin/plugins`       — list registry entries
//!   - `GET /admin/plugins/:name` — manifest + diagnostics for one plugin
//!   - `GET /admin/agents`        — list `.md` files under `server.data_dir/agents/`
//!
//! All routes live behind [`crate::middleware::admin_auth::require_admin`]
//! (HTTP Basic for now — session / JWT lands in M7). Writes, SSE log streaming,
//! live config swap, and the `doctor` subcommand stay behind `not_implemented`
//! until their respective milestones.
//!
//! The legacy [`router`] (no args) still returns 501 for `/admin/*`, so the
//! existing [`crate::routes::router`] stays valid. Callers that can supply
//! real state should use [`router_with_state`] instead.

use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::{routing::any, Router};
use corlinman_core::config::Config;
use corlinman_plugins::registry::PluginRegistry;

use crate::middleware::admin_auth::{require_admin, AdminAuthState};

use super::not_implemented;

pub mod agents;
pub mod plugins;

/// Shared read-only state passed to every admin handler.
///
/// Cloneable because every field is wrapped in `Arc`. Handlers load the
/// current snapshot via `state.plugins.clone()` or `state.config.load()`.
#[derive(Clone)]
pub struct AdminState {
    pub plugins: Arc<PluginRegistry>,
    pub config: Arc<ArcSwap<Config>>,
}

impl AdminState {
    pub fn new(plugins: Arc<PluginRegistry>, config: Arc<ArcSwap<Config>>) -> Self {
        Self { plugins, config }
    }
}

/// Legacy stub — kept so `routes::router()` compiles before a state-bearing
/// caller is wired up. Returns 501 for every `/admin/*` request.
pub fn router() -> Router {
    Router::new().route("/admin/*path", any(|| not_implemented("/admin/*")))
}

/// Production admin router: real handlers + basic-auth guard.
pub fn router_with_state(state: AdminState) -> Router {
    let auth_state = AdminAuthState::new(state.config.clone());

    Router::new()
        .merge(plugins::router(state.clone()))
        .merge(agents::router(state))
        .layer(axum::middleware::from_fn_with_state(
            auth_state,
            require_admin,
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use argon2::password_hash::{PasswordHasher, SaltString};
    use argon2::Argon2;
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use base64::Engine;
    use corlinman_plugins::registry::PluginRegistry;
    use tower::ServiceExt;

    fn hash_password(password: &str) -> String {
        let salt = SaltString::encode_b64(b"corlinman_test_salt_bytes_16").unwrap();
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .unwrap()
            .to_string()
    }

    fn test_app() -> Router {
        let mut cfg = Config::default();
        cfg.admin.username = Some("admin".into());
        cfg.admin.password_hash = Some(hash_password("secret"));
        let state = AdminState::new(
            Arc::new(PluginRegistry::default()),
            Arc::new(ArcSwap::from_pointee(cfg)),
        );
        router_with_state(state)
    }

    fn basic(u: &str, p: &str) -> String {
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("{u}:{p}"))
        )
    }

    #[tokio::test]
    async fn admin_routes_require_auth() {
        let resp = test_app()
            .oneshot(
                Request::builder()
                    .uri("/admin/plugins")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn admin_routes_accept_valid_basic_auth() {
        let resp = test_app()
            .oneshot(
                Request::builder()
                    .uri("/admin/plugins")
                    .header(header::AUTHORIZATION, basic("admin", "secret"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
