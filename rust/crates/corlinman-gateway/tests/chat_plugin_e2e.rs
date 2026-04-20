//! End-to-end test for M3 plugin wiring.
//!
//! Drives `/v1/chat/completions` with a scripted mock `ChatBackend` that emits
//! a `ServerFrame::ToolCall` for a real, on-disk `plugin-manifest.toml`. The
//! gateway must:
//!
//!   1. spawn the plugin (a tiny `python3` JSON-RPC stdio script materialised
//!      in a `tempfile::tempdir()`),
//!   2. forward the result back as a `ClientFrame::ToolResult` (byte-identical
//!      to the JSON the script emitted),
//!   3. surface the original `tool_calls` array in the HTTP response body.
//!
//! This closes the M1/M2 placeholder path: the gateway no longer returns
//! `awaiting_plugin_runtime`; it invokes the real plugin.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use corlinman_core::CorlinmanError;
use corlinman_gateway::routes::chat::{BackendRx, ChatBackend, ChatState};
use corlinman_gateway::routes::router_with_chat_state;
use corlinman_plugins::{Origin, PluginRegistry, SearchRoot};
use corlinman_proto::v1::{
    client_frame, server_frame, ChatStart, ClientFrame, Done, ServerFrame, ToolCall as PbToolCall,
};
use futures::{stream, Stream};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tower::ServiceExt;

/// Scripted backend that emits a fixed list of `ServerFrame`s and captures
/// every `ClientFrame` the gateway pushes back.
///
/// Unlike the in-crate unit-test `MockBackend` (which drops its receiver),
/// this one *stores* the receiver so the test can drain it after the
/// handler completes. That lets us assert the real `ToolResult` that the
/// gateway shipped back to Python, byte-identical with the plugin's stdout.
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

    /// Drain every `ClientFrame` the gateway has sent back so far. Safe to
    /// call once the handler has completed (the handler drops its `tx`, so
    /// `recv()` yields `None` after every message is drained).
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

/// Build a temp plugin dir containing `echo/plugin-manifest.toml` +
/// `echo/main.py`. The plugin reads one JSON-RPC line from stdin and emits a
/// deterministic result derived from `params.arguments.name`.
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

    // The script writes a canonical, whitespace-free JSON response so the
    // bytes we ship back to the Python side are byte-identical with the
    // expectation encoded in the assertion below.
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

#[tokio::test]
async fn chat_tool_call_runs_real_plugin_and_returns_tool_result() {
    // ---- scratch plugin ------------------------------------------------
    let tmp = tempfile::tempdir().unwrap();
    scratch_echo_plugin(tmp.path());
    let roots = vec![SearchRoot::new(tmp.path(), Origin::Config)];
    let registry = Arc::new(PluginRegistry::from_roots(roots));
    assert!(
        registry.get("echo").is_some(),
        "registry should contain the scratch plugin",
    );

    // ---- scripted backend -> ToolCall -> Done --------------------------
    let backend = Arc::new(ScriptedBackend::new(vec![
        tool_call("call_abc", "echo", "greet", r#"{"name":"Ada"}"#),
        done("tool_calls"),
    ]));

    // ---- router --------------------------------------------------------
    let state = ChatState::with_registry(backend.clone(), registry);
    let app = router_with_chat_state(state);

    // ---- POST /v1/chat/completions (non-streaming) ---------------------
    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "model": "test-model",
                "messages": [{"role": "user", "content": "greet me"}],
                "stream": false
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();

    // OpenAI tool_calls surfaced with the model's original args.
    let first = &v["choices"][0]["message"]["tool_calls"][0];
    assert_eq!(first["id"], "call_abc");
    assert_eq!(first["function"]["name"], "greet");
    let args: Value =
        serde_json::from_str(first["function"]["arguments"].as_str().unwrap()).unwrap();
    assert_eq!(args, json!({"name": "Ada"}));
    assert_eq!(v["choices"][0]["finish_reason"], "tool_calls");

    // The gateway must have shipped a populated ToolResult back to Python —
    // byte-identical to what the plugin printed on stdout.
    let captured = backend.drain_inbound().await;
    let result = captured
        .iter()
        .find_map(|f| match &f.kind {
            Some(client_frame::Kind::ToolResult(r)) => Some(r.clone()),
            _ => None,
        })
        .expect("gateway must have sent a ToolResult");
    assert_eq!(result.call_id, "call_abc");
    assert!(
        !result.is_error,
        "plugin succeeded, result must not be error"
    );
    assert_eq!(
        result.result_json,
        b"{\"greeting\":\"hello Ada\"}".to_vec(),
        "ToolResult bytes must be byte-identical with plugin stdout",
    );
}

#[tokio::test]
async fn chat_tool_call_missing_plugin_returns_structured_error() {
    let empty = Arc::new(PluginRegistry::default());
    let backend = Arc::new(ScriptedBackend::new(vec![
        tool_call("call_xyz", "nonexistent", "whatever", r#"{}"#),
        done("tool_calls"),
    ]));
    let state = ChatState::with_registry(backend.clone(), empty);
    let app = router_with_chat_state(state);

    let req = Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_vec(&json!({
                "model": "test-model",
                "messages": [{"role": "user", "content": "go"}],
                "stream": false
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let captured = backend.drain_inbound().await;
    let result = captured
        .iter()
        .find_map(|f| match &f.kind {
            Some(client_frame::Kind::ToolResult(r)) => Some(r.clone()),
            _ => None,
        })
        .expect("gateway must have sent a ToolResult (structured error)");
    assert!(result.is_error, "missing plugin must produce is_error=true");
    let payload: Value = serde_json::from_slice(&result.result_json).unwrap();
    assert_eq!(payload["code"], -32601);
    let msg = payload["message"].as_str().unwrap();
    assert!(
        msg.contains("nonexistent"),
        "message should name the plugin: {msg}"
    );
}
