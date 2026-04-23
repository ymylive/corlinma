//! B5-BE1 — Canvas Host endpoint integration tests.
//!
//! All tests boot a real `axum::serve` on `127.0.0.1:0` and talk HTTP via
//! `reqwest`. SSE tests consume the raw `Content-Type: text/event-stream`
//! body as a chunked byte stream because we need ordering + timing
//! guarantees that `tower::ServiceExt::oneshot` can't give us for
//! long-lived responses.
//!
//! The tests share a small harness (`spawn_gateway`) that:
//!   * builds a minimal admin config with known Basic-auth credentials,
//!   * constructs a `CanvasState` + `AdminAuthState`,
//!   * merges them into an axum Router,
//!   * binds a random local port and spawns `axum::serve` with a graceful
//!     shutdown handle so the test can tear it down at the end.
//!
//! Keeping the harness inline (no shared test-utils crate) mirrors the
//! pattern used by `approval_gate_e2e.rs` and `chat_plugin_e2e.rs`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use axum::Router;
use base64::Engine;
use corlinman_core::config::Config;
use corlinman_gateway::middleware::admin_auth::AdminAuthState;
use corlinman_gateway::routes::canvas::{self, CanvasState};
use futures::StreamExt;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

const ADMIN_USER: &str = "admin";
const ADMIN_PASS: &str = "secret";

fn hash_password(password: &str) -> String {
    use argon2::password_hash::{PasswordHasher, SaltString};
    let salt = SaltString::encode_b64(b"corlinman_test_salt_bytes_16").unwrap();
    argon2::Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn basic_auth_header() -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{ADMIN_USER}:{ADMIN_PASS}"))
    )
}

/// Build a `Config` with admin credentials set and the canvas endpoint
/// toggled via `enabled`.
fn make_config(enabled: bool) -> Config {
    let mut cfg = Config::default();
    cfg.admin.username = Some(ADMIN_USER.into());
    cfg.admin.password_hash = Some(hash_password(ADMIN_PASS));
    cfg.canvas.host_endpoint_enabled = enabled;
    // Default TTL stays at 1800; individual tests override via body when
    // they need a shorter horizon.
    cfg
}

struct Gateway {
    addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Gateway {
    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.handle.take() {
            let _ = h.await;
        }
    }
}

async fn spawn_gateway(config: Config) -> Gateway {
    let config_handle = Arc::new(ArcSwap::from_pointee(config));
    let canvas_state = CanvasState::new(config_handle.clone());
    let auth_state = AdminAuthState::new(config_handle);
    let router: Router = canvas::router(canvas_state, auth_state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await;
    });
    Gateway {
        addr,
        shutdown: Some(tx),
        handle: Some(handle),
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        // Keep the connection alive for SSE tests (default is fine; set
        // explicit timeouts to avoid hangs on failed assertions).
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

/// Post a JSON body with admin auth and return the response.
async fn post_json(client: &reqwest::Client, url: &str, body: Value) -> reqwest::Response {
    client
        .post(url)
        .header("authorization", basic_auth_header())
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .unwrap()
}

/// Create a session on an already-enabled gateway and return its id.
async fn create_session_ok(gw: &Gateway, c: &reqwest::Client, ttl_secs: Option<u64>) -> String {
    let mut body = json!({
        "title": "t",
        "initial_state": {},
    });
    if let Some(t) = ttl_secs {
        body["ttl_secs"] = json!(t);
    }
    let resp = post_json(c, &gw.url("/canvas/session"), body).await;
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::CREATED,
        "create session"
    );
    let v: Value = resp.json().await.unwrap();
    v["session_id"].as_str().unwrap().to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn disabled_endpoints_return_503() {
    let gw = spawn_gateway(make_config(false)).await;
    let c = client();

    for (method, path, body) in [
        ("POST", "/canvas/session", Some(json!({}))),
        (
            "POST",
            "/canvas/frame",
            Some(json!({"session_id": "cs_00000000", "kind": "a2ui_push"})),
        ),
        ("GET", "/canvas/session/cs_00000000/events", None),
    ] {
        let url = gw.url(path);
        let mut req = match method {
            "POST" => c.post(&url).body(body.unwrap().to_string()),
            "GET" => c.get(&url),
            _ => unreachable!(),
        };
        req = req
            .header("authorization", basic_auth_header())
            .header("content-type", "application/json");
        let resp = req.send().await.unwrap();
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            "{method} {path}",
        );
        let v: Value = resp.json().await.unwrap();
        assert_eq!(v["error"], "canvas_host_disabled");
    }

    gw.shutdown().await;
}

#[tokio::test]
async fn create_session_returns_id_and_expiry() {
    let gw = spawn_gateway(make_config(true)).await;
    let c = client();

    let resp = post_json(
        &c,
        &gw.url("/canvas/session"),
        json!({
            "title": "Topology snapshot",
            "initial_state": {"foo": "bar"},
            "ttl_secs": 600,
        }),
    )
    .await;
    assert_eq!(resp.status(), reqwest::StatusCode::CREATED);
    let v: Value = resp.json().await.unwrap();
    let id = v["session_id"].as_str().unwrap();
    assert!(id.starts_with("cs_"), "session_id must be cs_-prefixed");
    assert_eq!(id.len(), 3 + 8, "session_id must be 3 + 8 chars");
    let created = v["created_at_ms"].as_u64().unwrap();
    let expires = v["expires_at_ms"].as_u64().unwrap();
    assert!(
        expires > created,
        "expires_at_ms must be after created_at_ms"
    );
    // 600_000 ms TTL (allow a generous jitter window for slow CI).
    assert!(
        (expires - created).abs_diff(600_000) < 5_000,
        "TTL window drifted: created={created} expires={expires}",
    );

    gw.shutdown().await;
}

#[tokio::test]
async fn post_frame_fans_out_to_sse_subscribers() {
    let gw = spawn_gateway(make_config(true)).await;
    let c = client();
    let id = create_session_ok(&gw, &c, Some(600)).await;

    // Open the SSE stream first.
    let stream_resp = c
        .get(gw.url(&format!("/canvas/session/{id}/events")))
        .header("authorization", basic_auth_header())
        .header("accept", "text/event-stream")
        .send()
        .await
        .unwrap();
    assert_eq!(stream_resp.status(), reqwest::StatusCode::OK);
    assert_eq!(
        stream_resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or(""),
        "text/event-stream"
    );

    // Spawn a reader that collects until we see our canvas frame.
    let reader = tokio::spawn(async move {
        let mut bytes = stream_resp.bytes_stream();
        let mut buf = String::new();
        while let Some(chunk) = bytes.next().await {
            let chunk = chunk.unwrap();
            buf.push_str(std::str::from_utf8(&chunk).unwrap());
            if buf.contains("event: canvas") && buf.contains("\n\n") {
                break;
            }
        }
        buf
    });

    // Give the subscriber a moment to actually register its broadcast rx
    // before we post the frame. 50ms is plenty on localhost and keeps the
    // test deterministic without a polling loop.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let frame_resp = post_json(
        &c,
        &gw.url("/canvas/frame"),
        json!({
            "session_id": id,
            "kind": "a2ui_push",
            "payload": {"op": "set", "path": "/root", "value": 1},
        }),
    )
    .await;
    assert_eq!(frame_resp.status(), reqwest::StatusCode::ACCEPTED);
    let fv: Value = frame_resp.json().await.unwrap();
    assert!(fv["event_id"].is_string());

    let buf = tokio::time::timeout(Duration::from_secs(3), reader)
        .await
        .expect("sse reader timed out")
        .unwrap();
    assert!(buf.contains("event: canvas"), "sse payload: {buf}");
    assert!(buf.contains("\"kind\":\"a2ui_push\""), "sse payload: {buf}");
    assert!(buf.contains(&format!("\"session_id\":\"{id}\"")));

    gw.shutdown().await;
}

#[tokio::test]
async fn invalid_frame_kind_rejected_400() {
    let gw = spawn_gateway(make_config(true)).await;
    let c = client();
    let id = create_session_ok(&gw, &c, Some(600)).await;

    let resp = post_json(
        &c,
        &gw.url("/canvas/frame"),
        json!({
            "session_id": id,
            "kind": "delete_all_the_things",
            "payload": {},
        }),
    )
    .await;
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
    let v: Value = resp.json().await.unwrap();
    assert_eq!(v["error"], "invalid_frame_kind");
    assert!(v["allowed"].is_array());

    gw.shutdown().await;
}

#[tokio::test]
async fn unknown_session_returns_404() {
    let gw = spawn_gateway(make_config(true)).await;
    let c = client();

    // POST frame to a session that was never created.
    let resp = post_json(
        &c,
        &gw.url("/canvas/frame"),
        json!({
            "session_id": "cs_deadbeef",
            "kind": "a2ui_push",
            "payload": {},
        }),
    )
    .await;
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);

    // GET events for a session that was never created.
    let resp = c
        .get(gw.url("/canvas/session/cs_deadbeef/events"))
        .header("authorization", basic_auth_header())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);

    gw.shutdown().await;
}

#[tokio::test]
async fn sse_stream_closes_on_session_expiry() {
    let gw = spawn_gateway(make_config(true)).await;
    let c = client();
    // ttl_secs is clamped to ≥1; use the smallest legal window so the
    // janitor tick after creation reaps it quickly.
    let id = create_session_ok(&gw, &c, Some(1)).await;

    let stream_resp = c
        .get(gw.url(&format!("/canvas/session/{id}/events")))
        .header("authorization", basic_auth_header())
        .header("accept", "text/event-stream")
        .send()
        .await
        .unwrap();
    assert_eq!(stream_resp.status(), reqwest::StatusCode::OK);

    // Read until we see `event: end`; the janitor runs once per second so
    // 4s gives it two chances to reap. The body stream ends when the task
    // returns — collect everything until EOF or we see the marker.
    let buf = tokio::time::timeout(Duration::from_secs(5), async {
        let mut bytes = stream_resp.bytes_stream();
        let mut buf = String::new();
        while let Some(chunk) = bytes.next().await {
            let chunk = chunk.unwrap();
            buf.push_str(std::str::from_utf8(&chunk).unwrap());
            if buf.contains("event: end") {
                break;
            }
        }
        buf
    })
    .await
    .expect("sse never signalled end");
    assert!(buf.contains("event: end"), "buf: {buf}");
    assert!(buf.contains("\"expired\""), "buf: {buf}");

    gw.shutdown().await;
}

#[tokio::test]
async fn auth_token_required_for_all_three_routes() {
    let gw = spawn_gateway(make_config(true)).await;
    let c = client();

    // POST /canvas/session without auth.
    let resp = c
        .post(gw.url("/canvas/session"))
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);

    // POST /canvas/frame without auth.
    let resp = c
        .post(gw.url("/canvas/frame"))
        .header("content-type", "application/json")
        .body(r#"{"session_id":"cs_00000000","kind":"a2ui_push"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);

    // GET /canvas/session/:id/events without auth.
    let resp = c
        .get(gw.url("/canvas/session/cs_00000000/events"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);

    gw.shutdown().await;
}
