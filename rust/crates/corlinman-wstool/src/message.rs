//! Wire-level message frames for the distributed tool-bus protocol.
//!
//! All frames travel as JSON over WebSocket *text* frames. The `kind`
//! discriminant is an explicit tag so a human reading a pcap can pick out
//! message types without knowing Rust field order; this mirrors the
//! stability decisions made for [`corlinman_hooks::HookEvent`].
//!
//! The protocol is framed but **not** request/reply ordered on the wire —
//! concurrent `Invoke` requests share a single socket and are correlated
//! by `request_id`. The server side maintains the waiter map; the runner
//! side maintains the cancellation map.

use serde::{Deserialize, Serialize};

/// Every protocol frame.
///
/// Direction is encoded in the variant names' commentary rather than in
/// the type — the server and runner each implement their own
/// dispatch-by-kind match and reject frames that should only travel the
/// other way. This keeps the enum a single serde tagged union and avoids
/// a parallel pair of enums that would need to stay in lockstep.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WsToolMessage {
    // ---------- gateway → runner ----------
    /// Execute `tool` with `args`. The runner must eventually reply with
    /// exactly one of `Result` or `Error` bearing the same `request_id`,
    /// or the gateway will time out on its own deadline.
    Invoke {
        request_id: String,
        tool: String,
        args: serde_json::Value,
        timeout_ms: u64,
    },
    /// Cancel an in-flight invocation. Best-effort — the runner decides
    /// whether to abort the underlying handler, but must free its
    /// per-request state either way.
    Cancel { request_id: String },
    /// Liveness probe. Runner must answer with `Pong` or be disconnected
    /// after the configured number of misses.
    Ping,

    // ---------- runner → gateway ----------
    /// Handshake response — the runner accepted the auth token and
    /// declares its advertised tools. Sent exactly once per connection.
    Accept {
        server_version: String,
        heartbeat_secs: u32,
        supported_tools: Vec<ToolAdvert>,
    },
    /// Handshake response — auth/version mismatch or policy reject.
    /// Followed by connection close.
    Reject { code: String, message: String },
    /// Mid-flight progress update for an in-flight `Invoke`. Opaque JSON
    /// so handlers can stream whatever shape makes sense for them.
    Progress {
        request_id: String,
        data: serde_json::Value,
    },
    /// Terminal success/controlled-failure frame for a given invoke.
    /// `ok == false` carries a structured error payload in `payload`.
    Result {
        request_id: String,
        ok: bool,
        payload: serde_json::Value,
    },
    /// Terminal protocol-level error for a given invoke. Distinct from
    /// `Result{ok:false}` so callers can tell "tool ran and returned an
    /// error" from "tool never ran".
    Error {
        request_id: String,
        code: String,
        message: String,
    },
    /// Heartbeat reply.
    Pong,
}

/// Per-tool advertisement emitted by the runner inside `Accept`.
///
/// `parameters` is a JSON-Schema-shaped object suitable for
/// OpenAI-function-call style advertisement. We don't validate its shape
/// here — the registry layer does that when the runner is registered.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolAdvert {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoke_round_trips() {
        let original = WsToolMessage::Invoke {
            request_id: "req-1".into(),
            tool: "echo".into(),
            args: serde_json::json!({"msg": "hi"}),
            timeout_ms: 5_000,
        };
        let text = serde_json::to_string(&original).unwrap();
        assert!(text.contains("\"kind\":\"invoke\""));
        let back: WsToolMessage = serde_json::from_str(&text).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn accept_round_trips_with_tools() {
        let original = WsToolMessage::Accept {
            server_version: "0.1.0".into(),
            heartbeat_secs: 15,
            supported_tools: vec![ToolAdvert {
                name: "echo".into(),
                description: "returns args".into(),
                parameters: serde_json::json!({"type":"object"}),
            }],
        };
        let text = serde_json::to_string(&original).unwrap();
        let back: WsToolMessage = serde_json::from_str(&text).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn ping_and_pong_are_symmetric() {
        for m in [WsToolMessage::Ping, WsToolMessage::Pong] {
            let s = serde_json::to_string(&m).unwrap();
            let back: WsToolMessage = serde_json::from_str(&s).unwrap();
            assert_eq!(back, m);
        }
    }

    #[test]
    fn result_and_error_are_distinct_kinds() {
        let r = WsToolMessage::Result {
            request_id: "r".into(),
            ok: true,
            payload: serde_json::json!(1),
        };
        let e = WsToolMessage::Error {
            request_id: "r".into(),
            code: "boom".into(),
            message: "nope".into(),
        };
        let rs = serde_json::to_string(&r).unwrap();
        let es = serde_json::to_string(&e).unwrap();
        assert!(rs.contains("\"kind\":\"result\""));
        assert!(es.contains("\"kind\":\"error\""));
    }
}
