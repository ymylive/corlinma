//! Phase 4 W1 4-1A integration test: with `[tenants].enabled = true`
//! and an explicit `allowed_tenants` set on `AdminState`, the
//! tenant-scoping middleware satisfies the Wave 1 acceptance line —
//! "an operator scoped to tenant A can't see tenant B's proposals."
//!
//! Per-handler routing through `TenantPool` is a deliberate Wave 2+
//! refinement; this test asserts the *security boundary* the middleware
//! enforces:
//!
//! - `?tenant=acme` and `?tenant=bravo` are allowed → 200
//! - `?tenant=charlie` is **not** in the allowlist → 403
//!   `tenant_not_allowed` (the operator literally cannot fetch the
//!   route — they see a forbidden response, not an empty list)
//! - `?tenant=BAD!!` fails the slug regex → 400 `invalid_tenant_slug`
//! - No `?tenant=` query → falls back to `[tenants].default`, 200
//!
//! For comparison, a `[tenants].enabled = false` boot is also exercised
//! to prove pre-Phase-4 deployments behave byte-for-byte as before:
//! every `?tenant=...` query is silently ignored and the request
//! resolves to `default`.
//!
//! The test drives the real `routes::admin::router_with_state` Router
//! via `tower::ServiceExt::oneshot`; no TCP socket bound. Auth is the
//! existing HTTP Basic fallback using the same `argon2id` hash the
//! `admin_routes_accept_valid_basic_auth` unit test uses.

use std::collections::BTreeSet;
use std::sync::Arc;

use arc_swap::ArcSwap;
use argon2::password_hash::{PasswordHasher, SaltString};
use argon2::Argon2;
use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use base64::Engine;
use corlinman_core::config::Config;
use corlinman_gateway::routes::admin::{router_with_state, AdminState};
use corlinman_plugins::registry::PluginRegistry;
use corlinman_tenant::TenantId;
use tower::ServiceExt;

fn hash_password(password: &str) -> String {
    let salt = SaltString::encode_b64(b"corlinman_test_salt_bytes_16").unwrap();
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn basic(u: &str, p: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{u}:{p}"))
    )
}

fn make_app(tenants_enabled: bool, allowed: &[&str]) -> axum::Router {
    let mut cfg = Config::default();
    cfg.admin.username = Some("admin".into());
    cfg.admin.password_hash = Some(hash_password("secret"));
    cfg.tenants.enabled = tenants_enabled;
    cfg.tenants.default = "default".into();
    cfg.tenants.allowed = allowed.iter().map(|s| (*s).into()).collect();

    let mut allowed_set = BTreeSet::new();
    allowed_set.insert(TenantId::legacy_default());
    for slug in allowed {
        allowed_set.insert(TenantId::new(*slug).expect("test slug must validate"));
    }

    let state = AdminState::new(
        Arc::new(PluginRegistry::default()),
        Arc::new(ArcSwap::from_pointee(cfg)),
    )
    .with_allowed_tenants(allowed_set);

    router_with_state(state)
}

async fn get(app: axum::Router, uri: &str, auth: Option<&str>) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().uri(uri);
    if let Some(a) = auth {
        builder = builder.header(header::AUTHORIZATION, a);
    }
    let resp = app
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let body =
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or(serde_json::Value::Null);
    (status, body)
}

// ---------------------------------------------------------------------------
// `tenants.enabled = true`: the security boundary
// ---------------------------------------------------------------------------

#[tokio::test]
async fn enabled_allows_known_tenant_query() {
    let app = make_app(true, &["acme", "bravo"]);
    let (status, _) = get(
        app,
        "/admin/plugins?tenant=acme",
        Some(&basic("admin", "secret")),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "?tenant=acme should pass the middleware when 'acme' is in allowed set"
    );
}

#[tokio::test]
async fn enabled_allows_each_listed_tenant_independently() {
    let app = make_app(true, &["acme", "bravo"]);
    let (status, _) = get(
        app,
        "/admin/plugins?tenant=bravo",
        Some(&basic("admin", "secret")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn enabled_rejects_unlisted_tenant_with_403() {
    // Wave 1 acceptance: an operator pointing `?tenant=` at a slug
    // they have not been granted access to gets a 403, not a silently
    // empty list. `charlie` is well-formed but absent from the
    // allowlist.
    let app = make_app(true, &["acme", "bravo"]);
    let (status, body) = get(
        app,
        "/admin/plugins?tenant=charlie",
        Some(&basic("admin", "secret")),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "tenant_not_allowed");
    assert_eq!(body["slug"], "charlie");
}

#[tokio::test]
async fn enabled_rejects_malformed_slug_with_400() {
    let app = make_app(true, &["acme"]);
    let (status, body) = get(
        app,
        "/admin/plugins?tenant=BAD!!",
        Some(&basic("admin", "secret")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_tenant_slug");
    assert_eq!(body["slug"], "BAD!!");
}

#[tokio::test]
async fn enabled_falls_back_to_default_when_query_absent() {
    // No `?tenant=` query at all: the middleware resolves to the
    // configured fallback (`[tenants].default = "default"`) and the
    // request continues. The handler doesn't see a 4xx.
    let app = make_app(true, &["acme"]);
    let (status, _) = get(app, "/admin/plugins", Some(&basic("admin", "secret"))).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn auth_failure_short_circuits_before_tenant_check() {
    // Anonymous calls see 401 before the tenant_scope middleware
    // even runs. This proves layer ordering: tenant_scope is *inside*
    // require_admin, so authentication errors take precedence over
    // authorisation errors.
    let app = make_app(true, &["acme"]);
    let (status, _) = get(app, "/admin/plugins?tenant=charlie", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// `tenants.enabled = false`: legacy single-tenant mode
// ---------------------------------------------------------------------------

#[tokio::test]
async fn disabled_ignores_unknown_tenant_query() {
    // With multi-tenant enforcement off, the middleware short-circuits
    // and resolves every request to `TenantId::legacy_default()`.
    // `?tenant=charlie` is not validated against any allowlist; the
    // request passes through unchanged. This guarantees pre-Phase-4
    // deployments observe no behaviour change after this commit.
    let app = make_app(false, &[]);
    let (status, _) = get(
        app,
        "/admin/plugins?tenant=charlie",
        Some(&basic("admin", "secret")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn disabled_ignores_malformed_slug() {
    // Same: the middleware doesn't even parse the slug when scoping
    // is disabled. A malformed slug doesn't 400.
    let app = make_app(false, &[]);
    let (status, _) = get(
        app,
        "/admin/plugins?tenant=BAD!!",
        Some(&basic("admin", "secret")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}
