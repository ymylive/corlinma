//! Happy-path and negative-path invocation tests. The `unsupported`
//! case proves the tool-index lookup actually enforces advertisement.

mod common;

use std::sync::Arc;

use bytes::Bytes;
use tokio_util::sync::CancellationToken;

use common::{simple_advert, spawn_runner, EchoHandler, Harness};
use corlinman_core::CorlinmanError;
use corlinman_plugins::runtime::{PluginInput, PluginOutput, PluginRuntime};

fn make_input(tool: &str, args: serde_json::Value) -> PluginInput {
    PluginInput {
        plugin: "test-plugin".into(),
        tool: tool.into(),
        args_json: Bytes::from(serde_json::to_vec(&args).unwrap()),
        call_id: "call-1".into(),
        session_key: "session-1".into(),
        trace_id: "trace-1".into(),
        cwd: std::env::temp_dir(),
        env: vec![],
        deadline_ms: Some(5_000),
    }
}

#[tokio::test]
async fn invoke_roundtrip_produces_result() {
    let h = Harness::new().await;
    let _serve = spawn_runner(&h, "runner-A", vec![simple_advert("echo")], EchoHandler).await;

    let runtime = h.server.runtime();
    let input = make_input("echo", serde_json::json!({"hello": "world"}));
    let out = runtime
        .execute(input, None, CancellationToken::new())
        .await
        .expect("runtime execute");
    match out {
        PluginOutput::Success { content, .. } => {
            let v: serde_json::Value = serde_json::from_slice(&content).unwrap();
            assert_eq!(v["tool"], "echo");
            assert_eq!(v["echo"]["hello"], "world");
        }
        other => panic!("unexpected output: {other:?}"),
    }
}

#[tokio::test]
async fn invoke_with_unknown_tool_returns_unsupported() {
    let h = Harness::new().await;
    // No runner connected → every tool is unsupported.
    let runtime = h.server.runtime();
    let input = make_input("missing.tool", serde_json::json!({}));
    let err = runtime
        .execute(input, None, CancellationToken::new())
        .await
        .expect_err("expected NotFound");
    match err {
        CorlinmanError::NotFound { kind, id } => {
            assert_eq!(kind, "wstool.tool");
            assert_eq!(id, "missing.tool");
        }
        other => panic!("unexpected err: {other:?}"),
    }
    // Hold Arc so drop order is predictable.
    let _ = Arc::clone(&h.server);
}
