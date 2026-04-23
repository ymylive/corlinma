//! `NodeBridgeServer` — the gateway-side stub for the NodeBridge v1
//! protocol.
//!
//! This crate ships **no** native device client. Per the project
//! philosophy, the useful artefact is the *wire contract*: a future
//! Swift/Kotlin/Electron client can read [`crate::message`] and
//! [`docs/protocols/nodebridge.md`], implement the Register/Heartbeat/
//! JobResult/Telemetry side, and talk to this server with no code
//! shared.
//!
//! Connection lifecycle:
//!   1. Client dials `GET /nodebridge/connect`. We upgrade to WS.
//!   2. First frame **must** be [`NodeBridgeMessage::Register`]. Anything
//!      else, or a `Register` with `signature: None` when
//!      `accept_unsigned = false`, produces a `RegisterRejected` frame
//!      followed by a close.
//!   3. Server replies `Registered { server_version: "1.0.0-alpha",
//!      heartbeat_secs }` and stores the session in
//!      [`ServerState::sessions`].
//!   4. Reader loop dispatches inbound frames (`Heartbeat`, `JobResult`,
//!      `Telemetry`, `Pong`). Heartbeat misses are counted; after three
//!      the session is removed and the socket closed.
//!   5. [`NodeBridgeServer::dispatch_job`] fans out to the first
//!      registered session whose capabilities contain `kind`. The
//!      returned future resolves when that session posts a matching
//!      `JobResult` or when `timeout_ms` elapses.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::routing::get;
use axum::Router;
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tracing::{debug, info, warn};

use corlinman_hooks::{HookBus, HookEvent};

use crate::error::NodeBridgeError;
use crate::message::NodeBridgeMessage;
use crate::session::NodeSession;

/// Protocol spec version reported in the `Registered` frame. Bumped on
/// any breaking change to [`NodeBridgeMessage`].
pub const SPEC_VERSION: &str = "1.0.0-alpha";

/// Default heartbeat cadence, in seconds. Matches `WsTool` (the adjacent
/// crate) so a client team supporting both bridges can share timer
/// logic. Three consecutive misses → disconnect.
const DEFAULT_HEARTBEAT_SECS: u32 = 15;
/// Number of consecutive missed heartbeats before the server drops the
/// session. Matches the spec.
const MAX_MISSED_HEARTBEATS: u32 = 3;

/// Runtime configuration for the stub.
///
/// Kept independent of `corlinman_core::config::NodeBridgeConfig` so
/// this crate doesn't depend on gateway-side config plumbing — a caller
/// destructures their config once and passes the relevant fields here.
#[derive(Debug, Clone)]
pub struct NodeBridgeServerConfig {
    pub bind: SocketAddr,
    /// Mirror of `[nodebridge].accept_unsigned`. When `false`, a
    /// `Register` without `signature` is refused.
    pub accept_unsigned: bool,
    pub heartbeat_secs: u32,
}

impl NodeBridgeServerConfig {
    pub fn loopback(accept_unsigned: bool) -> Self {
        Self {
            bind: "127.0.0.1:0".parse().expect("literal socket addr"),
            accept_unsigned,
            heartbeat_secs: DEFAULT_HEARTBEAT_SECS,
        }
    }
}

/// Tracks one in-flight `DispatchJob` waiting on its `JobResult`.
#[derive(Debug)]
struct PendingJob {
    tx: oneshot::Sender<NodeBridgeMessage>,
}

/// Shared server state. Public so adjacent crate modules (future
/// integration layer) can hold an `Arc<ServerState>`; external callers
/// should use [`NodeBridgeServer`].
pub struct ServerState {
    cfg: NodeBridgeServerConfig,
    hook_bus: Arc<HookBus>,
    /// node_id → session. First registration with a given id wins;
    /// re-registration from a live id is refused with `duplicate_node_id`.
    sessions: DashMap<String, Arc<NodeSession>>,
    /// capability name → set of node ids advertising it. Updated on
    /// register/disconnect.
    capability_index: DashMap<String, Vec<String>>,
    /// job_id → pending waiter.
    pending_jobs: DashMap<String, PendingJob>,
    job_seq: AtomicU64,
}

impl ServerState {
    fn next_job_id(&self) -> String {
        let n = self.job_seq.fetch_add(1, Ordering::Relaxed);
        format!("job-{n}")
    }

    /// Find the first session that advertises `kind`. "First" means
    /// "first in the capability_index list", which in practice is
    /// insertion order — mirrors `corlinman-wstool`'s first-wins policy.
    fn find_capable_node(&self, kind: &str) -> Option<Arc<NodeSession>> {
        let ids = self.capability_index.get(kind)?;
        for id in ids.value().iter() {
            if let Some(sess) = self.sessions.get(id) {
                return Some(sess.value().clone());
            }
        }
        None
    }

    /// Insert a session; update capability_index. Returns `Err` if the
    /// node_id is already registered (duplicate connection).
    fn register_session(&self, session: Arc<NodeSession>) -> Result<(), NodeBridgeError> {
        use dashmap::mapref::entry::Entry;
        let id = session.id.clone();
        match self.sessions.entry(id.clone()) {
            Entry::Occupied(_) => Err(NodeBridgeError::RegisterRejected {
                code: "duplicate_node_id".into(),
                message: format!("node_id {id} already connected"),
            }),
            Entry::Vacant(v) => {
                for cap in &session.capabilities {
                    self.capability_index
                        .entry(cap.name.clone())
                        .or_default()
                        .push(id.clone());
                }
                v.insert(session);
                Ok(())
            }
        }
    }

    fn remove_session(&self, node_id: &str) {
        if self.sessions.remove(node_id).is_some() {
            self.capability_index.alter_all(|_k, mut v| {
                v.retain(|n| n != node_id);
                v
            });
            // Prune empty entries to keep diagnostics readable.
            self.capability_index.retain(|_, v| !v.is_empty());
        }
    }
}

/// Public server handle. Drop or call [`shutdown`] to tear down.
pub struct NodeBridgeServer {
    state: Arc<ServerState>,
    bound_addr: Mutex<Option<SocketAddr>>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl NodeBridgeServer {
    pub fn new(cfg: NodeBridgeServerConfig, hook_bus: Arc<HookBus>) -> Self {
        Self {
            state: Arc::new(ServerState {
                cfg,
                hook_bus,
                sessions: DashMap::new(),
                capability_index: DashMap::new(),
                pending_jobs: DashMap::new(),
                job_seq: AtomicU64::new(0),
            }),
            bound_addr: Mutex::new(None),
            join: Mutex::new(None),
        }
    }

    /// Build the axum router. Useful when the gateway wants to mount us
    /// under its own app instead of calling [`bind`].
    pub fn router(&self) -> Router {
        Router::new()
            .route("/nodebridge/connect", get(ws_upgrade_handler))
            .with_state(self.state.clone())
    }

    /// Bind the TCP listener and spawn the serve task. Returns the
    /// resolved local address (useful when `cfg.bind` uses port 0).
    pub async fn bind(&self) -> Result<SocketAddr, NodeBridgeError> {
        let listener = tokio::net::TcpListener::bind(self.state.cfg.bind).await?;
        let local = listener.local_addr()?;
        let app = self.router();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        *self.bound_addr.lock().await = Some(local);
        *self.join.lock().await = Some(handle);
        info!(addr = %local, "nodebridge server bound");
        Ok(local)
    }

    pub async fn local_addr(&self) -> Option<SocketAddr> {
        *self.bound_addr.lock().await
    }

    pub async fn shutdown(&self) {
        if let Some(h) = self.join.lock().await.take() {
            h.abort();
        }
    }

    /// Snapshot of currently-connected node ids mapped to node_type.
    pub fn connected_nodes(&self) -> HashMap<String, String> {
        self.state
            .sessions
            .iter()
            .map(|e| (e.key().clone(), e.value().node_type.clone()))
            .collect()
    }

    pub fn connected_count(&self) -> usize {
        self.state.sessions.len()
    }

    /// Dispatch a job to whichever node first advertised `kind`. Returns
    /// the `JobResult` received from that node, or a synthesised
    /// `JobResult { ok: false, payload: { "error": "timeout" } }` after
    /// `timeout_ms` with [`NodeBridgeError::Timeout`]. If no node
    /// advertises `kind`, returns [`NodeBridgeError::NoCapableNode`]
    /// without touching the wire.
    pub async fn dispatch_job(
        &self,
        kind: &str,
        params: serde_json::Value,
        timeout_ms: u64,
    ) -> Result<NodeBridgeMessage, NodeBridgeError> {
        let session = self
            .state
            .find_capable_node(kind)
            .ok_or_else(|| NodeBridgeError::NoCapableNode(kind.to_string()))?;

        let job_id = self.state.next_job_id();
        let (tx, rx) = oneshot::channel();
        self.state
            .pending_jobs
            .insert(job_id.clone(), PendingJob { tx });

        let msg = NodeBridgeMessage::DispatchJob {
            job_id: job_id.clone(),
            kind: kind.to_string(),
            params,
            timeout_ms,
        };
        let outbox = match &session.outbox {
            Some(tx) => tx.clone(),
            None => {
                self.state.pending_jobs.remove(&job_id);
                return Err(NodeBridgeError::Protocol(
                    "session has no outbox (test fixture?)".into(),
                ));
            }
        };
        if outbox.send(msg).await.is_err() {
            self.state.pending_jobs.remove(&job_id);
            return Err(NodeBridgeError::Protocol("client outbox closed".into()));
        }

        match tokio::time::timeout(Duration::from_millis(timeout_ms), rx).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => {
                // Sender dropped without reply — the session disconnected.
                Err(NodeBridgeError::Protocol(
                    "pending job sender dropped".into(),
                ))
            }
            Err(_) => {
                self.state.pending_jobs.remove(&job_id);
                Err(NodeBridgeError::Timeout { millis: timeout_ms })
            }
        }
    }
}

impl Drop for NodeBridgeServer {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.join.try_lock() {
            if let Some(h) = guard.take() {
                h.abort();
            }
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

async fn ws_upgrade_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ServerState>>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| connection_loop(socket, state))
}

/// Reject helper: send a `RegisterRejected` frame and close.
async fn reject(
    mut ws_tx: futures_util::stream::SplitSink<WebSocket, WsMessage>,
    code: &str,
    message: &str,
) {
    let frame = NodeBridgeMessage::RegisterRejected {
        code: code.into(),
        message: message.into(),
    };
    if let Ok(text) = serde_json::to_string(&frame) {
        let _ = ws_tx.send(WsMessage::Text(text)).await;
    }
    let _ = ws_tx.close().await;
}

async fn connection_loop(socket: WebSocket, state: Arc<ServerState>) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Step 1: first frame must be Register.
    let first_text = match ws_rx.next().await {
        Some(Ok(WsMessage::Text(t))) => t,
        other => {
            warn!(?other, "nodebridge: missing Register frame");
            let _ = ws_tx.close().await;
            return;
        }
    };
    let register = match serde_json::from_str::<NodeBridgeMessage>(&first_text) {
        Ok(NodeBridgeMessage::Register {
            node_id,
            node_type,
            capabilities,
            auth_token,
            version,
            signature,
        }) => (
            node_id,
            node_type,
            capabilities,
            auth_token,
            version,
            signature,
        ),
        Ok(_) => {
            warn!("nodebridge: first frame was not Register");
            reject(ws_tx, "protocol_violation", "first frame must be Register").await;
            return;
        }
        Err(err) => {
            warn!(%err, "nodebridge: first frame parse failed");
            reject(ws_tx, "bad_frame", &err.to_string()).await;
            return;
        }
    };
    let (node_id, node_type, capabilities, _auth_token, version, signature) = register;

    // Step 2: signing policy.
    if signature.is_none() && !state.cfg.accept_unsigned {
        warn!(%node_id, "nodebridge: unsigned registration refused by policy");
        reject(
            ws_tx,
            "unsigned_registration",
            "signature required; accept_unsigned is false",
        )
        .await;
        return;
    }

    // Step 3: build session + outbox.
    let (outbox_tx, mut outbox_rx) = mpsc::channel::<NodeBridgeMessage>(64);
    let session = Arc::new(NodeSession::new(
        node_id.clone(),
        node_type,
        capabilities,
        version,
        outbox_tx.clone(),
        now_ms(),
    ));
    if let Err(err) = state.register_session(session.clone()) {
        warn!(%node_id, %err, "nodebridge: register refused");
        let code = match &err {
            NodeBridgeError::RegisterRejected { code, .. } => code.clone(),
            _ => "register_rejected".to_string(),
        };
        reject(ws_tx, &code, &err.to_string()).await;
        return;
    }

    // Step 4: send Registered ack.
    let ack = NodeBridgeMessage::Registered {
        node_id: node_id.clone(),
        server_version: SPEC_VERSION.into(),
        heartbeat_secs: state.cfg.heartbeat_secs,
    };
    if let Ok(text) = serde_json::to_string(&ack) {
        if ws_tx.send(WsMessage::Text(text)).await.is_err() {
            state.remove_session(&node_id);
            return;
        }
    }
    info!(%node_id, "nodebridge: node registered");

    // Step 5: writer task drains outbox.
    let writer = tokio::spawn(async move {
        while let Some(msg) = outbox_rx.recv().await {
            let text = match serde_json::to_string(&msg) {
                Ok(s) => s,
                Err(err) => {
                    warn!(%err, "nodebridge: serialize failed, dropping frame");
                    continue;
                }
            };
            if ws_tx.send(WsMessage::Text(text)).await.is_err() {
                break;
            }
        }
        let _ = ws_tx.close().await;
    });

    // Step 6: heartbeat monitor. A single ticker at `heartbeat_secs`
    // increments a miss counter; any inbound frame resets it. Three
    // misses in a row drops the session.
    let hb_state = state.clone();
    let hb_node = node_id.clone();
    let hb_secs = state.cfg.heartbeat_secs.max(1) as u64;
    let missed = Arc::new(AtomicU32::new(0));
    let hb_missed = missed.clone();
    let hb_outbox = outbox_tx.clone();
    let heartbeat = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(hb_secs));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        ticker.tick().await; // eat immediate
        loop {
            ticker.tick().await;
            // Opportunistically probe with Ping; absence of reply is
            // counted the same as a missed Heartbeat.
            if hb_outbox.send(NodeBridgeMessage::Ping).await.is_err() {
                break;
            }
            let prior = hb_missed.fetch_add(1, Ordering::SeqCst);
            if prior + 1 >= MAX_MISSED_HEARTBEATS {
                warn!(
                    node_id = %hb_node,
                    misses = prior + 1,
                    "nodebridge: heartbeat miss threshold hit, dropping"
                );
                hb_state.remove_session(&hb_node);
                break;
            }
        }
    });

    // Step 7: reader loop — dispatch inbound frames.
    while let Some(frame) = ws_rx.next().await {
        let text = match frame {
            Ok(WsMessage::Text(t)) => t,
            Ok(WsMessage::Close(_)) => break,
            Ok(WsMessage::Ping(_)) => continue, // axum auto-pongs at WS layer
            Ok(_) => continue,
            Err(err) => {
                debug!(%err, %node_id, "nodebridge: socket read error");
                break;
            }
        };
        let msg: NodeBridgeMessage = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(err) => {
                warn!(%err, %node_id, "nodebridge: bad frame, ignoring");
                continue;
            }
        };
        // Every inbound frame is a liveness signal.
        missed.store(0, Ordering::SeqCst);
        session.touch(now_ms());
        handle_client_frame(&state, &node_id, msg).await;
    }

    // Step 8: cleanup.
    state.remove_session(&node_id);
    heartbeat.abort();
    // Dropping outbox_tx causes writer to exit once its rx drains.
    drop(outbox_tx);
    let _ = writer.await;
    info!(%node_id, "nodebridge: node disconnected");
}

async fn handle_client_frame(state: &Arc<ServerState>, node_id: &str, msg: NodeBridgeMessage) {
    match msg {
        NodeBridgeMessage::Heartbeat { .. } | NodeBridgeMessage::Pong => {
            // Liveness already stamped by the reader loop.
        }
        NodeBridgeMessage::JobResult {
            job_id,
            ok,
            payload,
        } => {
            if let Some((_, pending)) = state.pending_jobs.remove(&job_id) {
                let _ = pending.tx.send(NodeBridgeMessage::JobResult {
                    job_id,
                    ok,
                    payload,
                });
            } else {
                debug!(%node_id, %job_id, "nodebridge: JobResult for unknown job");
            }
        }
        NodeBridgeMessage::Telemetry {
            node_id: tele_node,
            metric,
            value,
            tags,
        } => {
            let event = HookEvent::Telemetry {
                node_id: tele_node,
                metric,
                value,
                tags,
            };
            let _ = state.hook_bus.emit(event).await;
        }
        NodeBridgeMessage::Ping => {
            // Reply with Pong if we can get at the session's outbox.
            if let Some(sess) = state.sessions.get(node_id) {
                if let Some(tx) = &sess.outbox {
                    let _ = tx.send(NodeBridgeMessage::Pong).await;
                }
            }
        }
        // Server-bound-only frames: client violated direction. Ignore.
        NodeBridgeMessage::Register { .. }
        | NodeBridgeMessage::Registered { .. }
        | NodeBridgeMessage::RegisterRejected { .. }
        | NodeBridgeMessage::DispatchJob { .. }
        | NodeBridgeMessage::Shutdown { .. } => {
            debug!(%node_id, kind = ?std::mem::discriminant(&msg), "nodebridge: unexpected direction");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_capable_node_returns_first_match() {
        let bus = Arc::new(HookBus::new(8));
        let state = Arc::new(ServerState {
            cfg: NodeBridgeServerConfig::loopback(true),
            hook_bus: bus,
            sessions: DashMap::new(),
            capability_index: DashMap::new(),
            pending_jobs: DashMap::new(),
            job_seq: AtomicU64::new(0),
        });

        let s1 = Arc::new(NodeSession::for_tests("n1", &["camera"]));
        let s2 = Arc::new(NodeSession::for_tests("n2", &["camera", "system.notify"]));
        state.register_session(s1).unwrap();
        state.register_session(s2).unwrap();

        let got = state.find_capable_node("camera").unwrap();
        assert_eq!(got.id, "n1"); // first inserted wins

        let got = state.find_capable_node("system.notify").unwrap();
        assert_eq!(got.id, "n2");

        assert!(state.find_capable_node("missing").is_none());
    }

    #[test]
    fn remove_session_prunes_capability_index() {
        let bus = Arc::new(HookBus::new(8));
        let state = Arc::new(ServerState {
            cfg: NodeBridgeServerConfig::loopback(true),
            hook_bus: bus,
            sessions: DashMap::new(),
            capability_index: DashMap::new(),
            pending_jobs: DashMap::new(),
            job_seq: AtomicU64::new(0),
        });
        let s1 = Arc::new(NodeSession::for_tests("n1", &["camera"]));
        state.register_session(s1).unwrap();
        assert!(state.find_capable_node("camera").is_some());
        state.remove_session("n1");
        assert!(state.find_capable_node("camera").is_none());
        assert!(state.capability_index.get("camera").is_none());
    }
}
