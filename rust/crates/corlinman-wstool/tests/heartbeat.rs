//! Heartbeat / disconnect tests. Uses `tokio::time::pause()` +
//! `tokio::time::advance()` to make the ping cadence deterministic
//! regardless of CI load.

mod common;

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::time::Instant;

use common::{simple_advert, Harness};
use corlinman_wstool::message::WsToolMessage;

#[tokio::test(start_paused = true)]
async fn heartbeat_disconnect_after_missed_pings() {
    // heartbeat = 1s, max_missed = 3 → disconnect after ~3s of silence.
    let h = Harness::with_heartbeat(1).await;
    let url = format!(
        "{}/wstool/connect?auth_token={}&runner_id=ghost&version=0.1.0",
        h.ws_url, h.token
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("ws connect");

    // Send a minimal Accept so the server considers us registered.
    let msg = WsToolMessage::Accept {
        server_version: "0.1.0".into(),
        heartbeat_secs: 1,
        supported_tools: vec![simple_advert("hb.echo")],
    };
    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        serde_json::to_string(&msg).unwrap(),
    ))
    .await
    .unwrap();

    // Wait until we're registered — spin, not sleep, so the paused
    // clock stays paused.
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.server.runner_count() == 0 {
        if Instant::now() > deadline {
            panic!("ghost runner never registered");
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(h.server.runner_count(), 1);

    // Drain incoming Ping frames silently — do *not* send Pong. That's
    // what triggers the miss counter.
    //
    // We advance 4 × 1s and yield between ticks so the server's
    // heartbeat interval fires each time and the reader loop processes
    // the empty-response case.
    for _ in 0..5 {
        tokio::time::advance(Duration::from_millis(1_100)).await;
        for _ in 0..10 {
            tokio::task::yield_now().await;
            // Consume any frames the server sent us; we're ignoring
            // them on purpose.
            if let Ok(Some(_frame)) =
                tokio::time::timeout(Duration::from_millis(1), ws.next()).await
            {
                // swallow
            }
        }
    }

    // By now the server should have removed the runner.
    assert_eq!(
        h.server.runner_count(),
        0,
        "expected runner disconnected after missed pings, still have {} connected",
        h.server.runner_count()
    );
}
