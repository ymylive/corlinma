//! Long-lived gRPC service runtime (Sprint 2 T1).
//!
//! Service plugins run continuously: the gateway spawns the plugin once on
//! boot (see [`crate::supervisor::PluginSupervisor`]), hands it a UDS path via
//! the `CORLINMAN_PLUGIN_ADDR` environment variable, and then talks to it
//! over gRPC for every tool call.
//!
//! [`ServiceRuntime`] owns the per-plugin `PluginBridgeClient` cache. The
//! supervisor calls [`ServiceRuntime::register`] once the child has bound
//! its UDS, and [`ServiceRuntime::unregister`] before respawn. The gateway's
//! `RegistryToolExecutor` calls [`ServiceRuntime::execute`] for every tool
//! invocation targeting a `plugin_type = "service"` manifest.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use dashmap::DashMap;
use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;
use tokio_util::sync::CancellationToken;
use tonic::transport::{Channel, Endpoint, Uri};

use corlinman_core::CorlinmanError;
use corlinman_proto::v1::plugin_bridge_client::PluginBridgeClient;
use corlinman_proto::v1::{tool_event::Kind as ToolEventKind, PluginToolCall, ToolEvent};

use crate::runtime::{PluginInput, PluginOutput};

/// Environment variable the gateway exports so the plugin knows where to
/// bind its gRPC server (UDS path on Unix).
pub const PLUGIN_ADDR_ENV: &str = "CORLINMAN_PLUGIN_ADDR";

/// Number of times `register` will retry connecting to the UDS before
/// giving up. With a 100 ms delay this gives the plugin up to 3 s to boot.
pub const REGISTER_RETRY_COUNT: u32 = 30;
/// Delay between register attempts.
pub const REGISTER_RETRY_DELAY: Duration = Duration::from_millis(100);

/// Long-lived gRPC runtime for `plugin_type = "service"` plugins.
///
/// Holds one tonic `PluginBridgeClient` per registered plugin; all clients
/// share a single tokio runtime via the enclosing `tokio::main` executor.
/// Cloning the runtime is cheap — the client map is `Arc<DashMap>`.
#[derive(Debug, Clone, Default)]
pub struct ServiceRuntime {
    clients: Arc<DashMap<String, PluginBridgeClient<Channel>>>,
}

impl ServiceRuntime {
    /// Construct an empty runtime. Supervisors call `register` once per
    /// service plugin after spawning the child process.
    pub fn new() -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
        }
    }

    /// Connect to a plugin's UDS endpoint and cache the client under `name`.
    ///
    /// Because the plugin process may take a moment to bind its socket, we
    /// retry [`REGISTER_RETRY_COUNT`] times at [`REGISTER_RETRY_DELAY`]
    /// intervals. The Uri passed to tonic is a placeholder — the actual
    /// transport is the `UnixStream` returned by the `service_fn` connector.
    pub async fn register(&self, name: &str, socket_path: &Path) -> Result<(), CorlinmanError> {
        let socket: PathBuf = socket_path.to_path_buf();

        let endpoint =
            Endpoint::try_from("http://[::]:50051").map_err(|e| CorlinmanError::PluginRuntime {
                plugin: name.to_string(),
                message: format!("invalid placeholder endpoint: {e}"),
            })?;

        let mut last_err: Option<String> = None;
        for attempt in 0..REGISTER_RETRY_COUNT {
            if !socket.exists() {
                last_err = Some(format!("socket {} not yet bound", socket.display()));
                tokio::time::sleep(REGISTER_RETRY_DELAY).await;
                continue;
            }

            let connect_socket = socket.clone();
            let connector = tower::service_fn(move |_uri: Uri| {
                let path = connect_socket.clone();
                async move {
                    let stream = UnixStream::connect(&path).await?;
                    Ok::<_, std::io::Error>(TokioIo::new(stream))
                }
            });

            match endpoint.clone().connect_with_connector(connector).await {
                Ok(channel) => {
                    let client = PluginBridgeClient::new(channel);
                    self.clients.insert(name.to_string(), client);
                    tracing::info!(
                        plugin = name,
                        socket = %socket.display(),
                        attempt = attempt + 1,
                        "service plugin gRPC client registered",
                    );
                    return Ok(());
                }
                Err(e) => {
                    last_err = Some(e.to_string());
                    tokio::time::sleep(REGISTER_RETRY_DELAY).await;
                }
            }
        }

        Err(CorlinmanError::PluginRuntime {
            plugin: name.to_string(),
            message: format!(
                "gRPC connect to {} failed after {} attempts: {}",
                socket.display(),
                REGISTER_RETRY_COUNT,
                last_err.unwrap_or_else(|| "socket never appeared".into()),
            ),
        })
    }

    /// Drop the cached client for `name`. Called by the supervisor before
    /// respawning a crashed plugin so stale channels don't leak.
    pub async fn unregister(&self, name: &str) {
        if self.clients.remove(name).is_some() {
            tracing::info!(plugin = name, "service plugin gRPC client unregistered");
        }
    }

    /// Issue one `Execute` RPC to the named plugin and wait for the first
    /// terminal frame (`result`, `error`, or `awaiting_approval`).
    ///
    /// `Progress` frames are drained silently; richer progress plumbing will
    /// arrive when the streaming layer is threaded through
    /// `RegistryToolExecutor`. Cancellation aborts the in-flight stream.
    pub async fn execute(
        &self,
        input: PluginInput,
        cancel: CancellationToken,
    ) -> Result<PluginOutput, CorlinmanError> {
        let mut client = match self.clients.get(&input.plugin) {
            Some(c) => c.clone(),
            None => {
                return Err(CorlinmanError::PluginRuntime {
                    plugin: input.plugin.clone(),
                    message: "service plugin not registered (supervisor spawn failed?)".into(),
                });
            }
        };

        let request = PluginToolCall {
            call_id: input.call_id.clone(),
            plugin: input.plugin.clone(),
            tool: input.tool.clone(),
            args_json: input.args_json.to_vec(),
            binding: None,
            session_key: input.session_key.clone(),
            approval_preconsented: false,
            trace: None,
        };

        let started = Instant::now();

        // Apply deadline via tokio::time::timeout if the caller supplied one;
        // otherwise the server-streaming RPC runs indefinitely under tonic's
        // default keepalive.
        let rpc = client.execute(request);
        let response = if let Some(ms) = input.deadline_ms {
            let deadline = Duration::from_millis(ms);
            tokio::select! {
                _ = cancel.cancelled() => {
                    return Err(CorlinmanError::Cancelled("service_grpc"));
                }
                r = tokio::time::timeout(deadline, rpc) => match r {
                    Err(_) => return Err(CorlinmanError::Timeout {
                        what: "service_grpc",
                        millis: ms,
                    }),
                    Ok(Err(status)) => return Err(tonic_status_to_err(&input.plugin, status)),
                    Ok(Ok(resp)) => resp,
                },
            }
        } else {
            tokio::select! {
                _ = cancel.cancelled() => {
                    return Err(CorlinmanError::Cancelled("service_grpc"));
                }
                r = rpc => match r {
                    Err(status) => return Err(tonic_status_to_err(&input.plugin, status)),
                    Ok(resp) => resp,
                },
            }
        };

        let mut stream = response.into_inner();

        loop {
            let next = tokio::select! {
                _ = cancel.cancelled() => {
                    return Err(CorlinmanError::Cancelled("service_grpc"));
                }
                r = stream.message() => r,
            };

            let event: Option<ToolEvent> =
                next.map_err(|s| tonic_status_to_err(&input.plugin, s))?;
            let Some(event) = event else {
                return Err(CorlinmanError::PluginRuntime {
                    plugin: input.plugin.clone(),
                    message: "service plugin closed stream without terminal frame".into(),
                });
            };

            let Some(kind) = event.kind else {
                // Spurious empty frame; keep reading.
                continue;
            };

            match kind {
                ToolEventKind::Progress(_) => {
                    // Progress frames are advisory; drain and wait for terminal.
                    continue;
                }
                ToolEventKind::Result(result) => {
                    let duration_ms = if result.duration_ms > 0 {
                        result.duration_ms
                    } else {
                        started.elapsed().as_millis() as u64
                    };
                    return Ok(PluginOutput::Success {
                        content: Bytes::from(result.result_json),
                        duration_ms,
                    });
                }
                ToolEventKind::Error(err) => {
                    let duration_ms = started.elapsed().as_millis() as u64;
                    return Ok(PluginOutput::Error {
                        code: err.reason as i64,
                        message: err.message,
                        duration_ms,
                    });
                }
                ToolEventKind::AwaitingApproval(pending) => {
                    // Approval gate lands in T3; for now surface as a runtime
                    // error so callers don't silently proceed.
                    return Err(CorlinmanError::PluginRuntime {
                        plugin: input.plugin.clone(),
                        message: format!(
                            "service plugin paused for approval (reason: {}); approval gate not wired yet",
                            pending.reason,
                        ),
                    });
                }
            }
        }
    }
}

fn tonic_status_to_err(plugin: &str, status: tonic::Status) -> CorlinmanError {
    CorlinmanError::PluginRuntime {
        plugin: plugin.to_string(),
        message: format!("gRPC status {:?}: {}", status.code(), status.message()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn execute_on_unregistered_plugin_errors() {
        let rt = ServiceRuntime::new();
        let input = PluginInput {
            plugin: "missing".into(),
            tool: "t".into(),
            args_json: Bytes::new(),
            call_id: "c1".into(),
            session_key: String::new(),
            trace_id: String::new(),
            cwd: std::path::PathBuf::from("/tmp"),
            env: Vec::new(),
            deadline_ms: None,
        };
        let err = rt
            .execute(input, CancellationToken::new())
            .await
            .unwrap_err();
        match err {
            CorlinmanError::PluginRuntime { plugin, message } => {
                assert_eq!(plugin, "missing");
                assert!(message.contains("not registered"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn register_times_out_when_socket_missing() {
        let rt = ServiceRuntime::new();
        let tmp = tempfile::tempdir().unwrap();
        let sock = tmp.path().join("never.sock");
        // Shorten the wait by using a dedicated nonexistent path; retry loop
        // runs 30 × 100 ms = 3 s, acceptable in CI.
        let err = rt.register("ghost", &sock).await.unwrap_err();
        match err {
            CorlinmanError::PluginRuntime { plugin, message } => {
                assert_eq!(plugin, "ghost");
                assert!(message.contains("failed after"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
