//! Cancellation + reconnect + loopback-equivalence tests.
//!
//! `cancel_in_flight_propagates_to_handler` uses a oneshot to prove the
//! handler actually observed the cancel — not just that the caller side
//! timed out. That's the whole point of wiring Cancel frames into
//! per-request cancel tokens.

mod common;

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::{oneshot, Mutex};
use tokio_util::sync::CancellationToken;

use common::{simple_advert, spawn_runner, EchoHandler, Harness};
use corlinman_plugins::runtime::{PluginInput, PluginOutput, PluginRuntime};
use corlinman_wstool::{ProgressSink, ToolError, ToolHandler};

struct SlowHandler {
    cancelled_tx: Mutex<Option<oneshot::Sender<()>>>,
}

#[async_trait]
impl ToolHandler for SlowHandler {
    async fn invoke(
        &self,
        _tool: &str,
        _args: serde_json::Value,
        _progress: ProgressSink,
        cancel: CancellationToken,
    ) -> Result<serde_json::Value, ToolError> {
        // Race the cancel token against a never-resolving future. If we
        // observe cancel, notify the test and return ToolError::cancelled.
        tokio::select! {
            _ = cancel.cancelled() => {
                if let Some(tx) = self.cancelled_tx.lock().await.take() {
                    let _ = tx.send(());
                }
                Err(ToolError::cancelled())
            }
            _ = std::future::pending::<()>() => unreachable!(),
        }
    }
}

fn make_input(tool: &str, args: serde_json::Value, deadline_ms: u64) -> PluginInput {
    PluginInput {
        plugin: "test-plugin".into(),
        tool: tool.into(),
        args_json: Bytes::from(serde_json::to_vec(&args).unwrap()),
        call_id: "c".into(),
        session_key: "s".into(),
        trace_id: "t".into(),
        cwd: std::env::temp_dir(),
        env: vec![],
        deadline_ms: Some(deadline_ms),
    }
}

#[tokio::test]
async fn cancel_in_flight_propagates_to_handler() {
    let h = Harness::new().await;
    let (tx, rx) = oneshot::channel();
    let handler = SlowHandler {
        cancelled_tx: Mutex::new(Some(tx)),
    };
    let _serve = spawn_runner(&h, "slow-runner", vec![simple_advert("slow")], handler).await;

    let runtime = h.server.runtime();
    let cancel = CancellationToken::new();
    let input = make_input("slow", serde_json::json!({}), 30_000);

    let cancel_c = cancel.clone();
    let caller = tokio::spawn(async move { runtime.execute(input, None, cancel_c).await });

    // Yield a few times so the invoke frame reaches the handler.
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    cancel.cancel();

    // Handler must observe cancel within a bounded window. We use the
    // oneshot, not a sleep — there's no polling involved.
    tokio::time::timeout(std::time::Duration::from_secs(2), rx)
        .await
        .expect("handler never saw cancel")
        .expect("oneshot closed");

    // Caller side also sees a Cancelled-style error.
    let res = tokio::time::timeout(std::time::Duration::from_secs(2), caller)
        .await
        .expect("caller stuck")
        .expect("join");
    assert!(res.is_err(), "execute should return Err on cancel");
    // Hold Arc for drop order determinism.
    let _ = Arc::clone(&h.server);
}

#[tokio::test]
async fn reconnect_fails_inflight_requests_with_disconnected() {
    let h = Harness::new().await;

    // Runner that never replies — so the invoke is guaranteed in-flight
    // when we tear the socket down.
    struct Wedged;
    #[async_trait]
    impl ToolHandler for Wedged {
        async fn invoke(
            &self,
            _tool: &str,
            _args: serde_json::Value,
            _progress: ProgressSink,
            _cancel: CancellationToken,
        ) -> Result<serde_json::Value, ToolError> {
            std::future::pending().await
        }
    }

    let serve = spawn_runner(&h, "wedged", vec![simple_advert("wedged")], Wedged).await;

    let runtime = h.server.runtime();
    let input = make_input("wedged", serde_json::json!({}), 30_000);
    let caller =
        tokio::spawn(async move { runtime.execute(input, None, CancellationToken::new()).await });

    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    // Kill the runner socket.
    serve.abort();
    let _ = serve.await;

    // Caller should now see a PluginRuntime error citing disconnect.
    let res = tokio::time::timeout(std::time::Duration::from_secs(2), caller)
        .await
        .expect("caller stuck")
        .expect("join");
    let err = res.expect_err("should fail after disconnect");
    let msg = err.to_string();
    assert!(
        msg.contains("disconnect") || msg.contains("runner disconnected"),
        "expected disconnect error, got: {msg}"
    );
}

#[tokio::test]
async fn loopback_equivalence_vs_direct_call() {
    let h = Harness::new().await;
    let handler = EchoHandler;
    let _serve = spawn_runner(&h, "loop", vec![simple_advert("echo")], handler.clone()).await;

    // Direct call to the same handler.
    let direct = handler
        .invoke(
            "echo",
            serde_json::json!({"k": "v"}),
            ProgressSink::discarding(),
            CancellationToken::new(),
        )
        .await
        .expect("direct call");

    // Call through the runtime.
    let runtime = h.server.runtime();
    let input = make_input("echo", serde_json::json!({"k": "v"}), 5_000);
    let via_runtime = match runtime
        .execute(input, None, CancellationToken::new())
        .await
        .expect("runtime call")
    {
        PluginOutput::Success { content, .. } => {
            serde_json::from_slice::<serde_json::Value>(&content).unwrap()
        }
        other => panic!("unexpected: {other:?}"),
    };

    assert_eq!(direct, via_runtime);
}
