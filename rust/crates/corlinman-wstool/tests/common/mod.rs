#![allow(dead_code)]

//! Shared test helpers.
//!
//! Every test gets a fresh gateway (bound to `127.0.0.1:0`) so parallel
//! test execution doesn't compete for a port. Helpers here spin the
//! server up, dial a runner, and wait for tool registration to settle
//! before returning — that last step removes the biggest source of
//! flake (tests running before the runner's `Accept` frame had landed).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use corlinman_hooks::HookBus;
use corlinman_wstool::{
    ProgressSink, ToolAdvert, ToolError, ToolHandler, WsToolConfig, WsToolRunner, WsToolServer,
};

pub struct Harness {
    pub server: Arc<WsToolServer>,
    pub hook_bus: Arc<HookBus>,
    pub token: String,
    pub ws_url: String,
}

impl Harness {
    pub async fn new() -> Self {
        Self::with_heartbeat(15).await
    }

    pub async fn with_heartbeat(heartbeat_secs: u32) -> Self {
        let hook_bus = Arc::new(HookBus::new(64));
        let token = "test-token".to_string();
        let mut cfg = WsToolConfig::loopback(&token);
        cfg.heartbeat_secs = heartbeat_secs;
        let server = Arc::new(WsToolServer::new(cfg, hook_bus.clone()));
        let addr = server.bind().await.expect("bind");
        Self {
            server,
            hook_bus,
            token,
            ws_url: format!("ws://{addr}"),
        }
    }
}

#[derive(Clone, Default)]
pub struct EchoHandler;

#[async_trait]
impl ToolHandler for EchoHandler {
    async fn invoke(
        &self,
        tool: &str,
        args: serde_json::Value,
        _progress: ProgressSink,
        _cancel: CancellationToken,
    ) -> Result<serde_json::Value, ToolError> {
        Ok(serde_json::json!({ "tool": tool, "echo": args }))
    }
}

/// Dial and register a runner with the supplied handler. Returns the
/// serve task handle and a shutdown cancel for the runner.
///
/// Also waits until `tool_name` shows up in the server's advertised-tools
/// set (bounded, no sleeps) so tests can invoke immediately after this.
pub async fn spawn_runner<H: ToolHandler>(
    h: &Harness,
    runner_id: &str,
    tools: Vec<ToolAdvert>,
    handler: H,
) -> tokio::task::JoinHandle<()> {
    let urls: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
    let runner = WsToolRunner::connect(&h.ws_url, &h.token, runner_id, tools)
        .await
        .expect("runner connect");
    let serve = tokio::spawn(async move {
        let _ = runner.serve_with(handler).await;
    });
    // Spin on tool_index until the tools show up (or 2s wall time).
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let adv = h.server.advertised_tools();
        if urls.iter().all(|name| adv.contains_key(name)) {
            break;
        }
        if Instant::now() > deadline {
            panic!("runner tools never registered: {urls:?}");
        }
        tokio::task::yield_now().await;
    }
    serve
}

pub fn simple_advert(name: &str) -> ToolAdvert {
    ToolAdvert {
        name: name.to_string(),
        description: format!("{name} tool"),
        parameters: serde_json::json!({"type": "object"}),
    }
}
