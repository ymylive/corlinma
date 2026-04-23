//! Contract tests for the NodeBridge v1 stub server.
//!
//! The stub ships without a native client. These tests exercise the
//! wire contract directly with `tokio-tungstenite`, so an iOS/Android
//! engineer reading them can copy the exact JSON shapes they need.
//!
//! All tests bind to `127.0.0.1:0` so parallel runs don't compete for
//! the same port. Heartbeat timing uses `tokio::time::pause()` +
//! `advance()` for deterministic misses.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Message as TungMessage;

use corlinman_hooks::{HookBus, HookEvent, HookPriority};
use corlinman_nodebridge::{
    Capability, NodeBridgeMessage, NodeBridgeServer, NodeBridgeServerConfig, SPEC_VERSION,
};

/// Shared fixture: bind a server with the supplied flags and return the
/// handle plus a `ws://…/nodebridge/connect` URL ready to dial.
struct Harness {
    server: Arc<NodeBridgeServer>,
    hook_bus: Arc<HookBus>,
    ws_url: String,
}

impl Harness {
    async fn with(accept_unsigned: bool, heartbeat_secs: u32) -> Self {
        let hook_bus = Arc::new(HookBus::new(64));
        let mut cfg = NodeBridgeServerConfig::loopback(accept_unsigned);
        cfg.heartbeat_secs = heartbeat_secs;
        let server = Arc::new(NodeBridgeServer::new(cfg, hook_bus.clone()));
        let addr = server.bind().await.expect("bind");
        Self {
            server,
            hook_bus,
            ws_url: format!("ws://{addr}/nodebridge/connect"),
        }
    }
}

fn sample_capability(name: &str) -> Capability {
    Capability::new(name, "1.0", serde_json::json!({"type": "object"}))
}

/// Dial + register with the supplied policy knobs. Returns the WS after
/// the `Registered` / `RegisterRejected` handshake has been drained.
async fn register_node(
    ws_url: &str,
    node_id: &str,
    caps: Vec<Capability>,
    signature: Option<String>,
) -> (
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    NodeBridgeMessage,
) {
    let (mut ws, _) = tokio_tungstenite::connect_async(ws_url)
        .await
        .expect("ws connect");
    let reg = NodeBridgeMessage::Register {
        node_id: node_id.into(),
        node_type: "ios".into(),
        capabilities: caps,
        auth_token: "tok".into(),
        version: "0.1.0".into(),
        signature,
    };
    ws.send(TungMessage::Text(serde_json::to_string(&reg).unwrap()))
        .await
        .unwrap();
    let reply = ws.next().await.expect("reply").expect("ws frame");
    let text = match reply {
        TungMessage::Text(t) => t,
        other => panic!("expected text frame, got {other:?}"),
    };
    let decoded: NodeBridgeMessage = serde_json::from_str(&text).unwrap();
    (ws, decoded)
}

#[tokio::test]
async fn register_accepted_produces_registered_frame() {
    let h = Harness::with(true, 15).await;
    let (_ws, ack) =
        register_node(&h.ws_url, "ios-1", vec![sample_capability("camera")], None).await;

    match ack {
        NodeBridgeMessage::Registered {
            node_id,
            server_version,
            heartbeat_secs,
        } => {
            assert_eq!(node_id, "ios-1");
            assert_eq!(server_version, SPEC_VERSION);
            assert_eq!(heartbeat_secs, 15);
        }
        other => panic!("expected Registered, got {other:?}"),
    }

    // Wait for the server to finalise registration before checking count.
    let deadline = Instant::now() + Duration::from_secs(2);
    while h.server.connected_count() == 0 {
        if Instant::now() > deadline {
            panic!("server never registered node");
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(h.server.connected_count(), 1);
}

#[tokio::test]
async fn register_without_signature_rejected_when_unsigned_disabled() {
    let h = Harness::with(false, 15).await;
    let (_ws, ack) = register_node(&h.ws_url, "ios-2", vec![], None).await;
    match ack {
        NodeBridgeMessage::RegisterRejected { code, .. } => {
            assert_eq!(code, "unsigned_registration");
        }
        other => panic!("expected RegisterRejected, got {other:?}"),
    }
    // Give the server a tick to finish cleanup.
    tokio::task::yield_now().await;
    assert_eq!(h.server.connected_count(), 0);
}

#[tokio::test]
async fn register_without_signature_accepted_when_unsigned_enabled() {
    let h = Harness::with(true, 15).await;
    let (_ws, ack) = register_node(&h.ws_url, "ios-3", vec![], None).await;
    assert!(
        matches!(ack, NodeBridgeMessage::Registered { .. }),
        "expected Registered when accept_unsigned=true, got {ack:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn heartbeat_missed_3_times_disconnects() {
    // heartbeat_secs = 1 → three missed pings at ~3s.
    let h = Harness::with(true, 1).await;
    let (mut ws, ack) =
        register_node(&h.ws_url, "ghost", vec![sample_capability("camera")], None).await;
    assert!(matches!(ack, NodeBridgeMessage::Registered { .. }));

    // Confirm the session landed before we start starving the server.
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.server.connected_count() == 0 {
        if Instant::now() > deadline {
            panic!("registration never settled");
        }
        tokio::task::yield_now().await;
    }

    // Advance past 3 × heartbeat cycles without sending Heartbeat / Pong.
    // Each `advance` + yield cycle lets the server's interval fire and
    // the miss counter increment.
    for _ in 0..5 {
        tokio::time::advance(Duration::from_millis(1_100)).await;
        for _ in 0..20 {
            tokio::task::yield_now().await;
            // Drain any Ping the server sent us without replying.
            if let Ok(Some(_frame)) =
                tokio::time::timeout(Duration::from_millis(1), ws.next()).await
            {
                // intentionally swallow
            }
        }
    }

    assert_eq!(
        h.server.connected_count(),
        0,
        "expected disconnection after 3 missed heartbeats"
    );
}

#[tokio::test]
async fn dispatch_job_routes_to_capable_node_and_returns_result() {
    let h = Harness::with(true, 15).await;
    let (mut ws, ack) = register_node(
        &h.ws_url,
        "ios-dispatch",
        vec![sample_capability("system.notify")],
        None,
    )
    .await;
    assert!(matches!(ack, NodeBridgeMessage::Registered { .. }));

    // Spawn a client-side responder: read frames, and when a DispatchJob
    // arrives echo a successful JobResult back.
    let responder = tokio::spawn(async move {
        while let Some(Ok(frame)) = ws.next().await {
            let TungMessage::Text(text) = frame else {
                continue;
            };
            let parsed: NodeBridgeMessage = match serde_json::from_str(&text) {
                Ok(p) => p,
                Err(_) => continue,
            };
            if let NodeBridgeMessage::DispatchJob { job_id, .. } = parsed {
                let result = NodeBridgeMessage::JobResult {
                    job_id,
                    ok: true,
                    payload: serde_json::json!({"delivered": true}),
                };
                ws.send(TungMessage::Text(serde_json::to_string(&result).unwrap()))
                    .await
                    .unwrap();
                return;
            }
        }
    });

    // Wait for the capability to be indexed. `dispatch_job` returns
    // NoCapableNode if we race.
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if h.server.connected_count() > 0 {
            break;
        }
        if Instant::now() > deadline {
            panic!("capability never indexed");
        }
        tokio::task::yield_now().await;
    }

    let result = h
        .server
        .dispatch_job("system.notify", serde_json::json!({"title": "hi"}), 2_000)
        .await
        .expect("dispatch ok");
    match result {
        NodeBridgeMessage::JobResult { ok, payload, .. } => {
            assert!(ok);
            assert_eq!(payload, serde_json::json!({"delivered": true}));
        }
        other => panic!("expected JobResult, got {other:?}"),
    }

    let _ = responder.await;
}

#[tokio::test]
async fn dispatch_job_unknown_capability_returns_not_found_error() {
    let h = Harness::with(true, 15).await;
    // Register one node with capability "camera"; then ask for "missing".
    let (_ws, ack) = register_node(
        &h.ws_url,
        "ios-cam",
        vec![sample_capability("camera")],
        None,
    )
    .await;
    assert!(matches!(ack, NodeBridgeMessage::Registered { .. }));

    let err = h
        .server
        .dispatch_job("missing.kind", serde_json::json!({}), 500)
        .await
        .expect_err("expected NoCapableNode");
    let msg = err.to_string();
    assert!(
        msg.contains("no capable node"),
        "expected NoCapableNode error, got: {msg}"
    );
}

#[tokio::test]
async fn telemetry_forwarded_to_hook_bus() {
    let h = Harness::with(true, 15).await;
    let mut sub = h.hook_bus.subscribe(HookPriority::Normal);

    let (mut ws, ack) = register_node(&h.ws_url, "ios-tele", vec![], None).await;
    assert!(matches!(ack, NodeBridgeMessage::Registered { .. }));

    let mut tags = std::collections::BTreeMap::new();
    tags.insert("build".into(), "dev".into());
    let tele = NodeBridgeMessage::Telemetry {
        node_id: "ios-tele".into(),
        metric: "battery.level".into(),
        value: 0.73,
        tags,
    };
    ws.send(TungMessage::Text(serde_json::to_string(&tele).unwrap()))
        .await
        .unwrap();

    // Recv with a generous timeout — the hook bus emit is async.
    let got = tokio::time::timeout(Duration::from_secs(2), sub.recv())
        .await
        .expect("hook event within 2s")
        .expect("hook recv ok");
    match got {
        HookEvent::Telemetry {
            node_id,
            metric,
            value,
            tags,
        } => {
            assert_eq!(node_id, "ios-tele");
            assert_eq!(metric, "battery.level");
            assert!((value - 0.73).abs() < 1e-9);
            assert_eq!(tags.get("build").map(String::as_str), Some("dev"));
        }
        other => panic!("expected HookEvent::Telemetry, got {other:?}"),
    }
}
