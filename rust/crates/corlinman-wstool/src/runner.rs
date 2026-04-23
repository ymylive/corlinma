//! `WsToolRunner` — client side of the protocol.
//!
//! A runner is usually a separate process (or even machine) that owns
//! one or more tool handlers, dials the gateway's WS endpoint, and
//! serves invocations for as long as the connection is alive.
//!
//! Reconnect: on an unexpected disconnect, `run_forever` rebuilds the
//! socket, re-sends the `Accept` advertisement, and resumes listening.
//! In-flight requests on the old socket are abandoned — by design, the
//! server surfaces them as `Error { code: "disconnected" }` to its
//! caller, so the runner does not need to ship replay state.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message as TungMessage;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::error::ToolError;
use crate::message::{ToolAdvert, WsToolMessage};

/// What a runner can serve. Implementors do the actual work of
/// executing a tool once we've framed arguments for them.
///
/// `progress` is a cheap sink; writes to it produce one `Progress` frame
/// on the wire per call. Implementations may safely ignore it.
///
/// `cancel` fires when the gateway sent a `Cancel` frame for this
/// request. Long-running handlers should select on it.
#[async_trait]
pub trait ToolHandler: Send + Sync + 'static {
    async fn invoke(
        &self,
        tool: &str,
        args: serde_json::Value,
        progress: ProgressSink,
        cancel: CancellationToken,
    ) -> Result<serde_json::Value, ToolError>;
}

/// Write-end of the per-invocation progress channel.
#[derive(Clone)]
pub struct ProgressSink {
    request_id: String,
    tx: mpsc::Sender<WsToolMessage>,
}

impl ProgressSink {
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Best-effort emit; drops silently if the writer is gone.
    pub async fn emit(&self, data: serde_json::Value) {
        let _ = self
            .tx
            .send(WsToolMessage::Progress {
                request_id: self.request_id.clone(),
                data,
            })
            .await;
    }

    /// Build a sink whose [`emit`] calls go nowhere. Useful for unit
    /// tests that call [`ToolHandler::invoke`] directly without an
    /// attached runner. The receiving half of the channel is dropped
    /// immediately, so a later `emit` is a no-op on a closed channel.
    pub fn discarding() -> Self {
        let (tx, _rx) = mpsc::channel(1);
        Self {
            request_id: "discarding".into(),
            tx,
        }
    }
}

/// Connected runner session. Call [`serve_with`] to start processing
/// invocations until the gateway hangs up or [`close`] is invoked.
pub struct WsToolRunner {
    #[allow(dead_code)]
    gateway_url: String,
    #[allow(dead_code)]
    auth_token: String,
    runner_id: String,
    #[allow(dead_code)]
    advert: Vec<ToolAdvert>,
    server_info: Option<AcceptInfo>,
    conn: Option<Connection>,
    handler_cancel: CancellationToken,
    background: Vec<JoinHandle<()>>,
}

#[derive(Debug, Clone)]
pub struct AcceptInfo {
    pub server_version: String,
    pub heartbeat_secs: u32,
}

struct Connection {
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
}

impl WsToolRunner {
    /// Dial the gateway, exchange handshake, and return a ready runner.
    pub async fn connect(
        gateway_url: &str,
        auth_token: &str,
        runner_id: &str,
        tools: Vec<ToolAdvert>,
    ) -> anyhow::Result<Self> {
        let url = build_connect_url(gateway_url, auth_token, runner_id, "0.1.0");
        let (ws_stream, _resp) = tokio_tungstenite::connect_async(&url).await?;
        let mut conn = Connection { ws: ws_stream };

        // Send our Accept advertisement first. The server considers
        // this its hand-shake trigger.
        let accept = WsToolMessage::Accept {
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            heartbeat_secs: 15,
            supported_tools: tools.clone(),
        };
        let text = serde_json::to_string(&accept)?;
        conn.ws.send(TungMessage::Text(text)).await?;

        // Server currently doesn't echo back an Accept of its own —
        // silence after auth means "accepted". We still populate an info
        // record so callers can fetch it.
        let info = AcceptInfo {
            server_version: "unknown".into(),
            heartbeat_secs: 15,
        };

        info!(runner_id, url = %url, "wstool: runner connected");
        Ok(Self {
            gateway_url: gateway_url.to_string(),
            auth_token: auth_token.to_string(),
            runner_id: runner_id.to_string(),
            advert: tools,
            server_info: Some(info),
            conn: Some(conn),
            handler_cancel: CancellationToken::new(),
            background: Vec::new(),
        })
    }

    pub fn server_info(&self) -> Option<&AcceptInfo> {
        self.server_info.as_ref()
    }

    pub fn runner_id(&self) -> &str {
        &self.runner_id
    }

    /// Serve invocations with `handler` until the socket closes or the
    /// runner is dropped. Returns when the connection ends.
    ///
    /// If this future is dropped mid-flight (e.g. the enclosing task is
    /// aborted), the `DropGuard` fires the runner-wide cancel token,
    /// which causes every handler task to stop waiting on its
    /// per-request cancel and drop its progress sink; the writer then
    /// sees its outbox close and shuts down the socket. That's what
    /// makes `serve.abort()` from a test observable on the gateway
    /// side as a plain TCP close.
    pub async fn serve_with<H: ToolHandler>(mut self, handler: H) -> anyhow::Result<()> {
        let handler = Arc::new(handler);
        let conn = self.conn.take().expect("runner already consumed");
        let cancel_all = self.handler_cancel.clone();
        let _drop_guard = cancel_all.clone().drop_guard();
        // Per-request cancel tokens so Cancel frames fire only the one
        // handler they target.
        let per_req = Arc::new(DashMap::<String, CancellationToken>::new());

        let (ws_tx, mut ws_rx) = conn.ws.split();
        let ws_tx = Arc::new(tokio::sync::Mutex::new(ws_tx));

        // mpsc fan-in: handlers push Progress/Result/Error frames,
        // writer drains and sends on the socket.
        let (outbox_tx, mut outbox_rx) = mpsc::channel::<WsToolMessage>(64);
        let writer_ws = ws_tx.clone();
        let writer_cancel = cancel_all.clone();
        let writer = tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = writer_cancel.cancelled() => break,
                    maybe = outbox_rx.recv() => {
                        let Some(msg) = maybe else { break };
                        let text = match serde_json::to_string(&msg) {
                            Ok(s) => s,
                            Err(err) => {
                                warn!(%err, "wstool runner: serialize failed");
                                continue;
                            }
                        };
                        let mut guard = writer_ws.lock().await;
                        if guard.send(TungMessage::Text(text)).await.is_err() {
                            break;
                        }
                    }
                }
            }
            // Best-effort close to make the peer observe disconnect
            // promptly instead of waiting for TCP half-close timeout.
            let mut guard = writer_ws.lock().await;
            let _ = guard.close().await;
        });
        self.background.push(writer);

        while let Some(frame) = ws_rx.next().await {
            let text = match frame {
                Ok(TungMessage::Text(t)) => t,
                Ok(TungMessage::Close(_)) => break,
                Ok(TungMessage::Ping(p)) => {
                    let mut guard = ws_tx.lock().await;
                    let _ = guard.send(TungMessage::Pong(p)).await;
                    continue;
                }
                Ok(_) => continue,
                Err(err) => {
                    debug!(%err, "wstool runner: socket error");
                    break;
                }
            };
            let msg: WsToolMessage = match serde_json::from_str(&text) {
                Ok(m) => m,
                Err(err) => {
                    warn!(%err, "wstool runner: bad frame");
                    continue;
                }
            };
            match msg {
                WsToolMessage::Ping => {
                    let _ = outbox_tx.send(WsToolMessage::Pong).await;
                }
                WsToolMessage::Invoke {
                    request_id,
                    tool,
                    args,
                    timeout_ms: _,
                } => {
                    let sink = ProgressSink {
                        request_id: request_id.clone(),
                        tx: outbox_tx.clone(),
                    };
                    let handler = handler.clone();
                    let req_cancel = CancellationToken::new();
                    per_req.insert(request_id.clone(), req_cancel.clone());
                    let per_req_c = per_req.clone();
                    let outbox_c = outbox_tx.clone();
                    let parent_cancel = cancel_all.clone();
                    tokio::spawn(async move {
                        // Parent cancel cascades to the per-request token.
                        let child_cancel = req_cancel.clone();
                        let guard = tokio::spawn({
                            let c = parent_cancel.clone();
                            let t = child_cancel.clone();
                            async move {
                                c.cancelled().await;
                                t.cancel();
                            }
                        });
                        // If the parent cancel fires first we abandon
                        // the handler entirely — the progress sink and
                        // `outbox_c` drop here so the writer sees its
                        // outbox drain down toward closed.
                        let result = tokio::select! {
                            _ = parent_cancel.cancelled() => {
                                guard.abort();
                                return;
                            }
                            r = handler.invoke(&tool, args, sink, child_cancel) => r,
                        };
                        let frame = match result {
                            Ok(payload) => WsToolMessage::Result {
                                request_id: request_id.clone(),
                                ok: true,
                                payload,
                            },
                            Err(err) => WsToolMessage::Error {
                                request_id: request_id.clone(),
                                code: err.code,
                                message: err.message,
                            },
                        };
                        let _ = outbox_c.send(frame).await;
                        per_req_c.remove(&request_id);
                        guard.abort();
                    });
                }
                WsToolMessage::Cancel { request_id } => {
                    if let Some((_, token)) = per_req.remove(&request_id) {
                        token.cancel();
                    }
                }
                _ => {
                    // Client shouldn't see Accept/Reject/Progress/Result/Error
                    // coming from the server side of the socket.
                }
            }
        }
        // Close writer and any stragglers.
        drop(outbox_tx);
        for h in std::mem::take(&mut self.background) {
            h.abort();
        }
        Ok(())
    }

    /// Convenience: build a runner, reconnect forever with exponential
    /// backoff. Callers pass a *factory* for the handler so each new
    /// connection gets a fresh handler if desired.
    pub async fn run_forever<F, H>(
        gateway_url: String,
        auth_token: String,
        runner_id: String,
        tools: Vec<ToolAdvert>,
        mut make_handler: F,
        shutdown: CancellationToken,
    ) -> anyhow::Result<()>
    where
        F: FnMut() -> H + Send + 'static,
        H: ToolHandler,
    {
        let mut delay = Duration::from_secs(1);
        loop {
            if shutdown.is_cancelled() {
                return Ok(());
            }
            match WsToolRunner::connect(&gateway_url, &auth_token, &runner_id, tools.clone()).await
            {
                Ok(runner) => {
                    let handler = make_handler();
                    if let Err(err) = runner.serve_with(handler).await {
                        warn!(%err, "wstool runner: serve returned");
                    }
                    delay = Duration::from_secs(1);
                }
                Err(err) => {
                    warn!(%err, "wstool runner: connect failed");
                }
            }
            tokio::select! {
                _ = shutdown.cancelled() => return Ok(()),
                _ = tokio::time::sleep(delay) => {}
            }
            delay = (delay * 2).min(Duration::from_secs(30));
        }
    }

    pub async fn close(self) -> anyhow::Result<()> {
        self.handler_cancel.cancel();
        for h in self.background {
            h.abort();
        }
        Ok(())
    }
}

/// Map `ws://host:port` → `ws://host:port/wstool/connect?…`. Works for
/// both `ws://` and `wss://` schemes; falls back to appending a path if
/// the input already has one.
pub fn build_connect_url(
    gateway_url: &str,
    auth_token: &str,
    runner_id: &str,
    version: &str,
) -> String {
    let base = gateway_url.trim_end_matches('/');
    let path = if base.ends_with("/wstool/connect") {
        String::new()
    } else {
        "/wstool/connect".to_string()
    };
    format!(
        "{base}{path}?auth_token={}&runner_id={}&version={}",
        urlenc(auth_token),
        urlenc(runner_id),
        urlenc(version),
    )
}

fn urlenc(s: &str) -> String {
    // Minimal percent-encoder. The fields in practice are token-like
    // (no spaces, no non-ASCII) but we encode `&`, `=`, `#`, `?`, and
    // space defensively so malformed inputs still produce a valid URL.
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Surface WsToolError into this module so tests can pattern match on
/// it without reaching into `crate::error`.
pub use crate::error::WsToolError as RunnerError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_builder_appends_path_once() {
        let u = build_connect_url("ws://127.0.0.1:18790", "tok", "r1", "0.1.0");
        assert_eq!(
            u,
            "ws://127.0.0.1:18790/wstool/connect?auth_token=tok&runner_id=r1&version=0.1.0"
        );
    }

    #[test]
    fn url_builder_trims_trailing_slash() {
        let u = build_connect_url("ws://127.0.0.1:18790/", "tok", "r1", "0.1.0");
        assert!(u.starts_with("ws://127.0.0.1:18790/wstool/connect?"));
    }
}
