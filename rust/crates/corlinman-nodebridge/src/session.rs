//! Per-connection state held by the server once a client finishes
//! registration. `NodeSession` is lightweight: the long-lived state
//! lives behind `Arc` in [`crate::server::ServerState`], and a session
//! mostly exists so tests and diagnostics can ask "which nodes are
//! connected, advertising what, and when did we last hear from them?".

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::message::{Capability, NodeBridgeMessage};

/// A single connected client. Cheap to clone (`Arc` around the mpsc
/// sender and an atomic clock).
#[derive(Debug, Clone)]
pub struct NodeSession {
    pub id: String,
    pub node_type: String,
    pub capabilities: Vec<Capability>,
    pub version: String,
    /// Wall-clock millis since the Unix epoch at last heartbeat/frame.
    pub last_heartbeat: Arc<AtomicU64>,
    /// Write half. `None` in test fixtures that want to build a session
    /// without a real socket (see [`NodeSession::for_tests`]).
    pub(crate) outbox: Option<mpsc::Sender<NodeBridgeMessage>>,
}

impl NodeSession {
    pub(crate) fn new(
        id: String,
        node_type: String,
        capabilities: Vec<Capability>,
        version: String,
        outbox: mpsc::Sender<NodeBridgeMessage>,
        at_ms: u64,
    ) -> Self {
        Self {
            id,
            node_type,
            capabilities,
            version,
            last_heartbeat: Arc::new(AtomicU64::new(at_ms)),
            outbox: Some(outbox),
        }
    }

    /// Update `last_heartbeat` to `at_ms`. Called from the reader loop
    /// on every inbound frame, not just `Heartbeat` — any client
    /// liveness (even a `JobResult`) proves the socket is alive.
    pub fn touch(&self, at_ms: u64) {
        self.last_heartbeat.store(at_ms, Ordering::SeqCst);
    }

    pub fn last_heartbeat_ms(&self) -> u64 {
        self.last_heartbeat.load(Ordering::SeqCst)
    }

    /// Returns `true` when this session advertises `kind`. Used by
    /// [`crate::server::ServerState::find_capable_node`].
    pub fn advertises(&self, kind: &str) -> bool {
        self.capabilities.iter().any(|c| c.name == kind)
    }

    /// Test-only builder: skip the socket plumbing and still exercise
    /// capability lookup.
    #[cfg(test)]
    pub fn for_tests(id: &str, caps: &[&str]) -> Self {
        Self {
            id: id.into(),
            node_type: "other".into(),
            capabilities: caps
                .iter()
                .map(|n| Capability::new(*n, "1.0", serde_json::json!({"type": "object"})))
                .collect(),
            version: "test".into(),
            last_heartbeat: Arc::new(AtomicU64::new(0)),
            outbox: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertises_returns_true_only_for_known_capability() {
        let s = NodeSession::for_tests("n1", &["system.notify", "camera"]);
        assert!(s.advertises("system.notify"));
        assert!(s.advertises("camera"));
        assert!(!s.advertises("missing"));
    }

    #[test]
    fn touch_updates_last_heartbeat() {
        let s = NodeSession::for_tests("n1", &[]);
        assert_eq!(s.last_heartbeat_ms(), 0);
        s.touch(1_700_000_000_000);
        assert_eq!(s.last_heartbeat_ms(), 1_700_000_000_000);
    }
}
