//! Basic-auth guard for `/admin/*`.
//!
//! M6 narrow scope: only HTTP Basic (`Authorization: Basic base64(user:pass)`)
//! verified against `config.admin.username` + `password_hash` (argon2id).
//! Session cookies / JWT land in a later milestone — see TODO below.
//!
//! The layer is constructed with an [`AdminAuthState`] that clones cheaply
//! (it holds `Arc<ArcSwap<Config>>`), so each request loads the *current*
//! config snapshot and re-verifies — rotating the admin password at runtime
//! takes effect on the next request without restarting the gateway.

use std::sync::Arc;

use arc_swap::ArcSwap;
use argon2::{password_hash::PasswordHash, Argon2, PasswordVerifier};
use axum::{
    body::Body,
    extract::State,
    http::{header, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine;
use corlinman_core::config::Config;
use serde_json::json;

/// Cloneable bundle of state that the admin auth middleware + admin handlers
/// need. `config` is shared with the rest of the gateway; verifying per
/// request lets admins rotate credentials without a restart.
#[derive(Clone)]
pub struct AdminAuthState {
    pub config: Arc<ArcSwap<Config>>,
}

impl AdminAuthState {
    pub fn new(config: Arc<ArcSwap<Config>>) -> Self {
        Self { config }
    }
}

/// Response payload for a 401. Matches the shape used by the rest of the
/// gateway so the UI's `CorlinmanApiError` parser can display a useful
/// message.
fn unauthorized(reason: &'static str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, r#"Basic realm="corlinman-admin""#)],
        Json(json!({
            "error": "unauthorized",
            "reason": reason,
        })),
    )
        .into_response()
}

/// Parse `Authorization: Basic <base64>` → `(user, pass)`. Returns `None`
/// for any malformed or non-Basic header.
fn parse_basic(header_value: &str) -> Option<(String, String)> {
    let rest = header_value.strip_prefix("Basic ")?.trim();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(rest)
        .ok()?;
    let s = String::from_utf8(decoded).ok()?;
    let (user, pass) = s.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

/// Verify `password` against an argon2id hash string. Any parse / verify
/// failure yields `false`; we never distinguish "wrong password" from
/// "malformed stored hash" in the response to avoid leaking hash shape.
fn argon2_verify(password: &str, stored_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored_hash) else {
        tracing::warn!("admin.password_hash is not a valid argon2 PHC string");
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// Axum middleware function. Attach via
/// `Router::layer(from_fn_with_state(state, require_admin))`.
pub async fn require_admin(
    State(state): State<AdminAuthState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let cfg = state.config.load();
    let Some(expected_user) = cfg.admin.username.as_deref() else {
        // No admin credentials configured at all — fail closed.
        return unauthorized("admin_not_configured");
    };
    let Some(expected_hash) = cfg.admin.password_hash.as_deref() else {
        return unauthorized("admin_not_configured");
    };

    let Some(auth_header) = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return unauthorized("missing_authorization");
    };

    let Some((user, pass)) = parse_basic(auth_header) else {
        return unauthorized("malformed_authorization");
    };

    if user != expected_user {
        return unauthorized("invalid_credentials");
    }
    if !argon2_verify(&pass, expected_hash) {
        return unauthorized("invalid_credentials");
    }

    next.run(req).await
}

// TODO(M7): add `POST /admin/login` + DashMap session store + `Set-Cookie`
// so the UI can avoid re-sending Basic credentials on every request.

#[cfg(test)]
mod tests {
    use super::*;
    use arc_swap::ArcSwap;
    use argon2::password_hash::{PasswordHasher, SaltString};
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use axum::Router;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn hash_password(password: &str) -> String {
        let salt = SaltString::encode_b64(b"corlinman_test_salt_bytes_16").unwrap();
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .unwrap()
            .to_string()
    }

    fn state_with(user: Option<&str>, password: Option<&str>) -> AdminAuthState {
        let mut cfg = Config::default();
        cfg.admin.username = user.map(str::to_string);
        cfg.admin.password_hash = password.map(hash_password);
        AdminAuthState::new(Arc::new(ArcSwap::from_pointee(cfg)))
    }

    fn app(state: AdminAuthState) -> Router {
        Router::new()
            .route("/ping", get(|| async { "pong" }))
            .layer(axum::middleware::from_fn_with_state(state, require_admin))
    }

    fn basic_header(user: &str, pass: &str) -> String {
        let raw = format!("{user}:{pass}");
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(raw)
        )
    }

    #[tokio::test]
    async fn missing_header_is_401() {
        let app = app(state_with(Some("admin"), Some("secret")));
        let res = app
            .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert!(res.headers().contains_key(header::WWW_AUTHENTICATE));
    }

    #[tokio::test]
    async fn wrong_password_is_401() {
        let app = app(state_with(Some("admin"), Some("secret")));
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/ping")
                    .header(header::AUTHORIZATION, basic_header("admin", "WRONG"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_username_is_401() {
        let app = app(state_with(Some("admin"), Some("secret")));
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/ping")
                    .header(header::AUTHORIZATION, basic_header("root", "secret"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn correct_credentials_pass_through() {
        let app = app(state_with(Some("admin"), Some("secret")));
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/ping")
                    .header(header::AUTHORIZATION, basic_header("admin", "secret"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn no_admin_configured_is_401() {
        let app = app(state_with(None, None));
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/ping")
                    .header(header::AUTHORIZATION, basic_header("admin", "secret"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn parse_basic_accepts_well_formed() {
        let h = basic_header("alice", "hunter2");
        let (u, p) = parse_basic(&h).unwrap();
        assert_eq!(u, "alice");
        assert_eq!(p, "hunter2");
    }

    #[test]
    fn parse_basic_rejects_non_basic() {
        assert!(parse_basic("Bearer xyz").is_none());
        assert!(parse_basic("Basic @@@not-base64@@@").is_none());
    }
}
