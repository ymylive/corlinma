//! Handshake tests. These use raw `tokio-tungstenite` to exercise the
//! auth layer directly — no runner client library involvement, so an
//! auth bug can't be masked by the client happening to also reject.

mod common;

use futures_util::SinkExt;
use tokio_tungstenite::tungstenite::handshake::client::Response;

use common::{simple_advert, Harness};
use corlinman_wstool::message::WsToolMessage;

#[tokio::test]
async fn handshake_accepts_valid_token() {
    let h = Harness::new().await;
    let url = format!(
        "{}/wstool/connect?auth_token={}&runner_id=rx-1&version=0.1.0",
        h.ws_url, h.token
    );
    let (mut ws, resp): (_, Response) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("ws connect");
    assert_eq!(resp.status().as_u16(), 101, "websocket upgrade expected");

    // Send our Accept advertisement — mirrors what WsToolRunner does.
    let msg = WsToolMessage::Accept {
        server_version: "0.1.0".into(),
        heartbeat_secs: 15,
        supported_tools: vec![simple_advert("handshake.echo")],
    };
    let text = serde_json::to_string(&msg).unwrap();
    ws.send(tokio_tungstenite::tungstenite::Message::Text(text))
        .await
        .unwrap();

    // Wait until the server registers the tool → proof of acceptance.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if h.server.advertised_tools().contains_key("handshake.echo") {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("tool never registered");
        }
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn handshake_rejects_invalid_token() {
    let h = Harness::new().await;
    let url = format!(
        "{}/wstool/connect?auth_token=WRONG&runner_id=rx-2&version=0.1.0",
        h.ws_url
    );
    let err = tokio_tungstenite::connect_async(&url)
        .await
        .expect_err("should reject");
    // The exact error type varies across tungstenite versions, but the
    // message always carries the HTTP 401 status we returned.
    let msg = err.to_string();
    assert!(
        msg.contains("401") || msg.to_lowercase().contains("unauthorized"),
        "expected 401/Unauthorized error, got: {msg}"
    );

    // And the server's runner count is still zero.
    // Small pause to let the server finalise its reject path.
    tokio::task::yield_now().await;
    assert_eq!(h.server.runner_count(), 0);
}
