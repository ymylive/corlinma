//! Wire-level message frames for the NodeBridge v1 protocol.
//!
//! Transport: JSON text frames over WebSocket. Keeping JSON (instead of
//! protobuf) was deliberate — the target clients are iOS/Android/macOS
//! apps written in Swift/Kotlin by third parties. Every mainstream mobile
//! stack ships a first-class JSON codec; protobuf would force a `.proto`
//! generation step on every downstream integrator for no payoff at the
//! volumes we expect (device-scale telemetry + sparse job dispatch, not
//! high-rate RPC).
//!
//! All frames share one tagged enum. `serde(tag = "kind", rename_all =
//! "snake_case")` keeps `kind` as an explicit wire field — a pcap reader
//! can sort frames without knowing Rust field ordering, same contract
//! decision as [`corlinman_hooks::HookEvent`] and
//! [`corlinman_wstool::message::WsToolMessage`].
//!
//! The [`Capability`] struct is the one place a future signed
//! attestation will live: `NodeBridgeMessage::Register::signature` is
//! currently `Option<String>` and populated only when the client opts in.
//! When `accept_unsigned = false`, a `Register` without a signature is
//! rejected by the server pre-state-change.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Every protocol frame. Direction (client → server vs. server → client)
/// is encoded in commentary; the server rejects mis-directed frames at
/// the dispatcher in [`crate::server`] rather than at the type layer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeBridgeMessage {
    // ---------- client → server ----------
    /// Handshake + advertisement. First frame a client sends. The server
    /// replies with [`NodeBridgeMessage::Registered`] or
    /// [`NodeBridgeMessage::RegisterRejected`] before any other traffic
    /// is accepted. `signature` is reserved for client attestation
    /// (future-work); when the server's `accept_unsigned = false` and
    /// this is `None`, registration is rejected.
    Register {
        node_id: String,
        /// Free-form client classification: `"ios"`, `"android"`,
        /// `"macos"`, `"linux"`, `"other"`. Not enforced server-side.
        node_type: String,
        capabilities: Vec<Capability>,
        auth_token: String,
        version: String,
        /// Future: signed client attestation.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    /// Client → server liveness ping. Expected cadence is
    /// `heartbeat_secs` returned in the prior `Registered` frame. After
    /// three consecutive missed heartbeats the server drops the
    /// connection.
    Heartbeat { node_id: String, at_ms: u64 },
    /// Terminal result for a previously-dispatched job. The server
    /// correlates by `job_id`; unknown job ids are logged and dropped.
    JobResult {
        job_id: String,
        ok: bool,
        payload: serde_json::Value,
    },
    /// Arbitrary metric emission. Forwarded as `HookEvent::Telemetry` on
    /// the gateway's hook bus. `tags` is `BTreeMap` so serialized key
    /// order is stable across emits.
    Telemetry {
        node_id: String,
        metric: String,
        value: f64,
        tags: BTreeMap<String, String>,
    },

    // ---------- server → client ----------
    /// Registration accepted. `heartbeat_secs` tells the client how
    /// often to emit `Heartbeat`.
    Registered {
        node_id: String,
        server_version: String,
        heartbeat_secs: u32,
    },
    /// Registration refused. Followed by connection close.
    RegisterRejected { code: String, message: String },
    /// Execute `kind` on the client. The client must eventually respond
    /// with `JobResult { job_id }`; otherwise the server synthesises a
    /// local `JobResult { ok: false, payload: { "error": "timeout" } }`
    /// once `timeout_ms` elapses.
    ///
    /// The job-kind field is named `job_kind` on the wire to avoid a
    /// collision with the enum discriminant tag (`kind`); the Rust field
    /// keeps the shorter name since there's no ambiguity inside Rust.
    DispatchJob {
        job_id: String,
        #[serde(rename = "job_kind")]
        kind: String,
        params: serde_json::Value,
        timeout_ms: u64,
    },
    /// Liveness probe in either direction.
    Ping,
    /// Liveness reply in either direction.
    Pong,
    /// Server-initiated connection close with a human-readable reason.
    Shutdown { reason: String },
}

/// A single capability a node advertises at registration time.
///
/// `params_schema` is an opaque JSON-Schema-shaped object. Validation is
/// deferred to dispatchers; the server treats it as a black box so
/// clients can extend their schemas without a server upgrade.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Capability {
    pub name: String,
    pub version: String,
    pub params_schema: serde_json::Value,
}

impl Capability {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        params_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            params_schema,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_round_trips_without_signature() {
        let original = NodeBridgeMessage::Register {
            node_id: "ios-dev-1".into(),
            node_type: "ios".into(),
            capabilities: vec![Capability::new(
                "system.notify",
                "1.0",
                serde_json::json!({"type":"object"}),
            )],
            auth_token: "tok".into(),
            version: "0.1.0".into(),
            signature: None,
        };
        let text = serde_json::to_string(&original).unwrap();
        assert!(text.contains("\"kind\":\"register\""));
        // Omitted signature must not appear as `null` on the wire.
        assert!(
            !text.contains("\"signature\""),
            "signature=None must be skipped in serialized form, got {text}"
        );
        let back: NodeBridgeMessage = serde_json::from_str(&text).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn register_round_trips_with_signature() {
        let original = NodeBridgeMessage::Register {
            node_id: "ios-dev-1".into(),
            node_type: "ios".into(),
            capabilities: vec![],
            auth_token: "tok".into(),
            version: "0.1.0".into(),
            signature: Some("base64sig".into()),
        };
        let text = serde_json::to_string(&original).unwrap();
        assert!(text.contains("\"signature\":\"base64sig\""));
        let back: NodeBridgeMessage = serde_json::from_str(&text).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn registered_and_rejected_are_distinct_kinds() {
        let ok = NodeBridgeMessage::Registered {
            node_id: "n".into(),
            server_version: "1.0.0-alpha".into(),
            heartbeat_secs: 15,
        };
        let nope = NodeBridgeMessage::RegisterRejected {
            code: "unsigned_registration".into(),
            message: "signature required".into(),
        };
        let ok_s = serde_json::to_string(&ok).unwrap();
        let nope_s = serde_json::to_string(&nope).unwrap();
        assert!(ok_s.contains("\"kind\":\"registered\""));
        assert!(nope_s.contains("\"kind\":\"register_rejected\""));
    }

    #[test]
    fn dispatch_and_job_result_are_symmetric() {
        let d = NodeBridgeMessage::DispatchJob {
            job_id: "j1".into(),
            kind: "system.notify".into(),
            params: serde_json::json!({"title": "hi"}),
            timeout_ms: 5_000,
        };
        let r = NodeBridgeMessage::JobResult {
            job_id: "j1".into(),
            ok: true,
            payload: serde_json::json!({"delivered": true}),
        };
        for m in [d, r] {
            let s = serde_json::to_string(&m).unwrap();
            let back: NodeBridgeMessage = serde_json::from_str(&s).unwrap();
            assert_eq!(back, m);
        }
    }

    #[test]
    fn telemetry_tags_serialize_in_key_order() {
        let mut tags = BTreeMap::new();
        tags.insert("region".into(), "cn".into());
        tags.insert("build".into(), "dev".into());
        let m = NodeBridgeMessage::Telemetry {
            node_id: "n".into(),
            metric: "battery.level".into(),
            value: 0.9,
            tags,
        };
        let s = serde_json::to_string(&m).unwrap();
        let build_at = s.find("build").expect("build tag");
        let region_at = s.find("region").expect("region tag");
        assert!(build_at < region_at, "tags must be sorted: {s}");
    }

    #[test]
    fn ping_pong_shutdown_round_trip() {
        for m in [
            NodeBridgeMessage::Ping,
            NodeBridgeMessage::Pong,
            NodeBridgeMessage::Shutdown {
                reason: "server_stopping".into(),
            },
        ] {
            let s = serde_json::to_string(&m).unwrap();
            let back: NodeBridgeMessage = serde_json::from_str(&s).unwrap();
            assert_eq!(back, m);
        }
    }
}
