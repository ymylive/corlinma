//! Sprint 2 T3 — approval gate end-to-end.
//!
//! These tests drive the full chat path with an `ApprovalGate` attached,
//! verifying that:
//!
//!   1. a `Prompt` rule parks the tool call, surfaces it in
//!      `GET /admin/approvals`, and a `POST /admin/approvals/:id/decide`
//!      with `{"approve":true}` lets the plugin execute (the scripted
//!      backend observes a non-error `ToolResult`).
//!   2. without an operator decision the call times out within the
//!      gate's configured deadline and the chat response carries a
//!      structured `approval_timeout` tool result (no plugin ever runs).
//!
//! The setup mirrors `chat_plugin_e2e.rs` (scripted backend emitting a
//! single `ToolCall` frame followed by `Done`) but wires the admin
//! router in addition to the chat router so the tests can poll and
//! resolve without spinning a real HTTP server.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use base64::Engine;
use corlinman_core::config::{ApprovalMode, ApprovalRule, Config};
use corlinman_core::CorlinmanError;
use corlinman_gateway::middleware::approval::ApprovalGate;
use corlinman_gateway::routes::admin::{router_with_state as admin_router, AdminState};
use corlinman_gateway::routes::chat::{BackendRx, ChatBackend, ChatState};
use corlinman_gateway::routes::router_with_chat_state;
use corlinman_plugins::{Origin, PluginRegistry, SearchRoot};
use corlinman_proto::v1::{
    client_frame, server_frame, ChatStart, ClientFrame, Done, ServerFrame, ToolCall as PbToolCall,
};
use corlinman_vector::SqliteStore;
use futures::{stream, Stream};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Scripted backend — identical in spirit to the one in `chat_plugin_e2e.rs`.
// Duplicated here to keep test files standalone (no shared test-utils crate
// yet; copy-paste is cheaper than a single-use util module).
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ScriptedBackend {
    frames: Arc<tokio::sync::Mutex<Vec<ServerFrame>>>,
    inbound_rx: Arc<tokio::sync::Mutex<Option<mpsc::Receiver<ClientFrame>>>>,
}

impl ScriptedBackend {
    fn new(frames: Vec<ServerFrame>) -> Self {
        Self {
            frames: Arc::new(tokio::sync::Mutex::new(frames)),
            inbound_rx: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    async fn drain_inbound(&self) -> Vec<ClientFrame> {
        let mut out = Vec::new();
        let mut guard = self.inbound_rx.lock().await;
        if let Some(rx) = guard.as_mut() {
            while let Some(frame) = rx.recv().await {
                out.push(frame);
            }
        }
        out
    }
}

#[async_trait]
impl ChatBackend for ScriptedBackend {
    async fn start(
        &self,
        _start: ChatStart,
    ) -> Result<(mpsc::Sender<ClientFrame>, BackendRx), CorlinmanError> {
        let (tx, rx) = mpsc::channel::<ClientFrame>(16);
        *self.inbound_rx.lock().await = Some(rx);

        let frames: Vec<_> = std::mem::take(&mut *self.frames.lock().await)
            .into_iter()
            .map(Ok)
            .collect();
        let out: BackendRx = Box::pin(stream::iter(frames))
            as Pin<Box<dyn Stream<Item = Result<ServerFrame, CorlinmanError>> + Send>>;
        Ok((tx, out))
    }
}

fn scratch_echo_plugin(root: &std::path::Path) {
    let plugin_dir = root.join("echo");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("plugin-manifest.toml"),
        r#"
name = "echo"
version = "0.1.0"
plugin_type = "sync"

[entry_point]
command = "python3"
args = ["main.py"]
"#,
    )
    .unwrap();
    std::fs::write(
        plugin_dir.join("main.py"),
        r#"import json, sys

line = sys.stdin.readline()
req = json.loads(line)
args = req.get("params", {}).get("arguments", {})
name = args.get("name", "")
resp = {
    "jsonrpc": "2.0",
    "id": req.get("id", 1),
    "result": {"greeting": "hello " + name},
}
sys.stdout.write(json.dumps(resp, separators=(",", ":")))
sys.stdout.write("\n")
sys.stdout.flush()
"#,
    )
    .unwrap();
}

fn tool_call(call_id: &str, plugin: &str, tool: &str, args_json: &str) -> ServerFrame {
    ServerFrame {
        kind: Some(server_frame::Kind::ToolCall(PbToolCall {
            call_id: call_id.into(),
            plugin: plugin.into(),
            tool: tool.into(),
            args_json: args_json.as_bytes().to_vec(),
            seq: 0,
        })),
    }
}

fn done(reason: &str) -> ServerFrame {
    ServerFrame {
        kind: Some(server_frame::Kind::Done(Done {
            finish_reason: reason.into(),
            usage: None,
            total_tokens_seen: 0,
            wall_time_ms: 0,
        })),
    }
}

// ---------------------------------------------------------------------------
// Harness: builds a chat app + admin app sharing the same gate + config.
// ---------------------------------------------------------------------------

struct Harness {
    chat_app: Router,
    admin_app: Router,
    gate: Arc<ApprovalGate>,
    backend: Arc<ScriptedBackend>,
    /// Kept alive so the scratch plugin / DB files aren't deleted mid-test.
    _plugin_tmp: tempfile::TempDir,
    _db_tmp: tempfile::TempDir,
}

const ADMIN_USER: &str = "admin";
const ADMIN_PASS: &str = "secret";

fn admin_basic_header() -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{ADMIN_USER}:{ADMIN_PASS}"))
    )
}

fn hash_password(password: &str) -> String {
    use argon2::password_hash::{PasswordHasher, SaltString};
    let salt = SaltString::encode_b64(b"corlinman_test_salt_bytes_16").unwrap();
    argon2::Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

async fn harness(
    rules: Vec<ApprovalRule>,
    timeout: Duration,
    backend_frames: Vec<ServerFrame>,
) -> Harness {
    let plugin_tmp = tempfile::tempdir().unwrap();
    scratch_echo_plugin(plugin_tmp.path());
    let roots = vec![SearchRoot::new(plugin_tmp.path(), Origin::Config)];
    let registry = Arc::new(PluginRegistry::from_roots(roots));

    let db_tmp = tempfile::tempdir().unwrap();
    let store = SqliteStore::open(&db_tmp.path().join("kb.sqlite"))
        .await
        .unwrap();
    corlinman_vector::migration::ensure_schema(&store)
        .await
        .unwrap();
    let gate = Arc::new(ApprovalGate::new(rules, Arc::new(store), timeout));

    let backend = Arc::new(ScriptedBackend::new(backend_frames));
    let chat_state =
        ChatState::with_registry(backend.clone() as Arc<dyn ChatBackend>, registry.clone())
            .with_approval_gate(gate.clone());
    let chat_app = router_with_chat_state(chat_state);

    let mut cfg = Config::default();
    cfg.admin.username = Some(ADMIN_USER.into());
    cfg.admin.password_hash = Some(hash_password(ADMIN_PASS));
    let admin_state = AdminState::new(registry, Arc::new(ArcSwap::from_pointee(cfg)))
        .with_approval_gate(gate.clone());
    let admin_app = admin_router(admin_state);

    Harness {
        chat_app,
        admin_app,
        gate,
        backend,
        _plugin_tmp: plugin_tmp,
        _db_tmp: db_tmp,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn prompt_rule_blocks_until_admin_approves_then_plugin_runs() {
    let rules = vec![ApprovalRule {
        plugin: "echo".into(),
        tool: None,
        mode: ApprovalMode::Prompt,
        allow_session_keys: Vec::new(),
    }];
    let h = harness(
        rules,
        Duration::from_secs(10),
        vec![
            tool_call("call_abc", "echo", "greet", r#"{"name":"Ada"}"#),
            done("tool_calls"),
        ],
    )
    .await;

    // Fire the chat request in the background. It will block inside the
    // approval gate waiting for an operator; only a /decide call unblocks it.
    let chat_app = h.chat_app.clone();
    let chat_handle = tokio::spawn(async move {
        let req = Request::builder()
            .method("POST")
            .uri("/v1/chat/completions")
            .header("content-type", "application/json")
            .header("x-session-key", "session-42")
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "model": "test-model",
                    "messages": [{"role": "user", "content": "greet me"}],
                    "stream": false
                }))
                .unwrap(),
            ))
            .unwrap();
        chat_app.oneshot(req).await.unwrap()
    });

    // Poll the admin endpoint until the pending row lands.
    let id = loop {
        let req = Request::builder()
            .method("GET")
            .uri("/admin/approvals")
            .header(header::AUTHORIZATION, admin_basic_header())
            .body(Body::empty())
            .unwrap();
        let resp = h.admin_app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let arr: Value = serde_json::from_slice(&body).unwrap();
        if let Some(row) = arr.as_array().unwrap().first() {
            assert_eq!(row["plugin"], "echo");
            assert_eq!(row["session_key"], "session-42");
            break row["id"].as_str().unwrap().to_string();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };

    // Approve.
    let req = Request::builder()
        .method("POST")
        .uri(format!("/admin/approvals/{id}/decide"))
        .header("content-type", "application/json")
        .header(header::AUTHORIZATION, admin_basic_header())
        .body(Body::from(r#"{"approve":true}"#))
        .unwrap();
    let decide_resp = h.admin_app.clone().oneshot(req).await.unwrap();
    assert_eq!(decide_resp.status(), StatusCode::OK);

    // Chat request now completes — the plugin actually ran.
    let chat_resp = chat_handle.await.unwrap();
    assert_eq!(chat_resp.status(), StatusCode::OK);
    let body = to_bytes(chat_resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["choices"][0]["finish_reason"], "tool_calls");
    // The scripted backend captured exactly one ToolResult, non-error, with
    // the real echo plugin's JSON payload (proves the approval gate let the
    // call through instead of short-circuiting).
    let captured = h.backend.drain_inbound().await;
    let result = captured
        .iter()
        .find_map(|f| match &f.kind {
            Some(client_frame::Kind::ToolResult(r)) => Some(r.clone()),
            _ => None,
        })
        .expect("gateway must have sent a ToolResult after approval");
    assert!(!result.is_error, "approved path must not be is_error");
    assert_eq!(result.result_json, b"{\"greeting\":\"hello Ada\"}".to_vec(),);

    // History view shows the row with decision=approved.
    let req = Request::builder()
        .method("GET")
        .uri("/admin/approvals?include_decided=true")
        .header(header::AUTHORIZATION, admin_basic_header())
        .body(Body::empty())
        .unwrap();
    let resp = h.admin_app.clone().oneshot(req).await.unwrap();
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let arr: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(arr.as_array().unwrap()[0]["decision"], "approved");

    drop(h.gate);
}

#[tokio::test]
async fn prompt_rule_times_out_and_tool_result_carries_structured_error() {
    let rules = vec![ApprovalRule {
        plugin: "echo".into(),
        tool: None,
        mode: ApprovalMode::Prompt,
        allow_session_keys: Vec::new(),
    }];
    // Short TTL so the test doesn't wait minutes.
    let h = harness(
        rules,
        Duration::from_millis(150),
        vec![
            tool_call("call_abc", "echo", "greet", r#"{"name":"Bob"}"#),
            done("tool_calls"),
        ],
    )
    .await;

    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .header("x-session-key", "session-bob")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "model": "test-model",
                "messages": [{"role": "user", "content": "greet me"}],
                "stream": false
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = h.chat_app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // The gateway must have sent a ToolResult with the timeout code.
    let captured = h.backend.drain_inbound().await;
    let result = captured
        .iter()
        .find_map(|f| match &f.kind {
            Some(client_frame::Kind::ToolResult(r)) => Some(r.clone()),
            _ => None,
        })
        .expect("gateway must have sent a ToolResult on timeout");
    assert!(
        result.is_error,
        "timeout path must mark the ToolResult as is_error"
    );
    let payload: Value = serde_json::from_slice(&result.result_json).unwrap();
    assert_eq!(payload["code"], "approval_timeout");
    assert_eq!(payload["plugin"], "echo");
    assert_eq!(payload["tool"], "greet");

    // The row persisted with decision=timeout.
    let rows = h
        .gate
        .store_arc_public()
        .list_pending_approvals(true)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].decision.as_deref(), Some("timeout"));
}
