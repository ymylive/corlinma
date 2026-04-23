//! `WsToolRuntime` — [`PluginRuntime`] adapter over the WS tool bus.
//!
//! When the plugin registry calls `execute`, we look up which connected
//! runner advertises the tool, send an `Invoke` frame, and await the
//! terminal `Result` / `Error` frame. Outcomes map onto the existing
//! [`PluginOutput`] variants so callers can't tell us apart from
//! [`corlinman_plugins::runtime::jsonrpc_stdio::JsonRpcStdioRuntime`].

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use bytes::Bytes;
use tokio_util::sync::CancellationToken;

use corlinman_core::CorlinmanError;
use corlinman_plugins::runtime::{PluginInput, PluginOutput, PluginRuntime, ProgressSink};

use crate::error::WsToolError;
use crate::server::{invoke_once, ServerState};

/// Default deadline when the caller's `PluginInput.deadline_ms` is
/// `None`. Matches the stdio runtime's default to keep behaviour uniform.
pub const DEFAULT_DEADLINE_MS: u64 = 30_000;

/// Clone-cheap handle returned by [`crate::WsToolServer::runtime`].
#[derive(Clone)]
pub struct WsToolRuntime {
    state: Arc<ServerState>,
}

impl WsToolRuntime {
    pub(crate) fn new(state: Arc<ServerState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl PluginRuntime for WsToolRuntime {
    #[tracing::instrument(
        name = "wstool_invoke",
        skip(self, input, _progress, cancel),
        fields(
            tool = %input.tool,
            runner_id = tracing::field::Empty,
            duration_ms = tracing::field::Empty,
            ok = tracing::field::Empty,
        )
    )]
    async fn execute(
        &self,
        input: PluginInput,
        _progress: Option<Arc<dyn ProgressSink>>,
        cancel: CancellationToken,
    ) -> Result<PluginOutput, CorlinmanError> {
        let args: serde_json::Value = if input.args_json.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&input.args_json).map_err(|e| CorlinmanError::Parse {
                what: "wstool.args_json",
                message: e.to_string(),
            })?
        };

        // Surface the concrete runner that owns this tool. `runner_for_tool`
        // is cheap (single DashMap lookup); `None` just means the routing
        // layer will map to `Unsupported` below.
        let runner_id = self
            .state
            .tool_index
            .get(&input.tool)
            .map(|r| r.value().clone())
            .unwrap_or_default();
        tracing::Span::current().record("runner_id", runner_id.as_str());

        let timeout_ms = input.deadline_ms.unwrap_or(DEFAULT_DEADLINE_MS);
        let started = Instant::now();

        let outcome = invoke_once(
            self.state.clone(),
            input.tool.clone(),
            args,
            timeout_ms,
            cancel,
        )
        .await;

        let duration_ms = started.elapsed().as_millis() as u64;
        tracing::Span::current().record("duration_ms", duration_ms);

        let ok = outcome.is_ok();
        tracing::Span::current().record("ok", ok);
        corlinman_core::metrics::WSTOOL_INVOKES_TOTAL
            .with_label_values(&[input.tool.as_str(), if ok { "true" } else { "false" }])
            .inc();
        corlinman_core::metrics::WSTOOL_INVOKE_DURATION
            .with_label_values(&[input.tool.as_str()])
            .observe(duration_ms as f64 / 1000.0);

        match outcome {
            Ok(payload) => {
                let bytes = Bytes::from(serde_json::to_vec(&payload).map_err(|e| {
                    CorlinmanError::PluginRuntime {
                        plugin: input.plugin.clone(),
                        message: format!("wstool serialize result: {e}"),
                    }
                })?);
                Ok(PluginOutput::success(bytes, duration_ms))
            }
            Err(WsToolError::Unsupported(tool)) => Err(CorlinmanError::NotFound {
                kind: "wstool.tool",
                id: tool,
            }),
            Err(WsToolError::Timeout { millis }) => Err(CorlinmanError::Timeout {
                what: "wstool.invoke",
                millis,
            }),
            Err(WsToolError::Disconnected) => Err(CorlinmanError::PluginRuntime {
                plugin: input.plugin.clone(),
                message: "runner disconnected before reply".into(),
            }),
            Err(WsToolError::ToolFailed { code, message }) => {
                // `cancelled` is protocol-level, not a controlled failure
                // in the JSON-RPC sense — surface it as Cancelled so
                // callers can distinguish "caller aborted" from "tool
                // ran and reported an error".
                if code == "cancelled" {
                    return Err(CorlinmanError::Cancelled("wstool.cancel"));
                }
                Ok(PluginOutput::error(
                    tool_failed_code_to_jsonrpc(&code),
                    format!("{code}: {message}"),
                    duration_ms,
                ))
            }
            Err(other) => Err(CorlinmanError::PluginRuntime {
                plugin: input.plugin.clone(),
                message: other.to_string(),
            }),
        }
    }

    fn kind(&self) -> &'static str {
        "wstool"
    }
}

/// Map our short `code` strings onto the JSON-RPC error codes the stdio
/// runtime uses, so callers can compare across runtimes.
fn tool_failed_code_to_jsonrpc(code: &str) -> i64 {
    match code {
        "cancelled" => -32001,
        "unsupported" => -32601,
        "timeout" => -32002,
        "disconnected" => -32003,
        _ => -32000,
    }
}
