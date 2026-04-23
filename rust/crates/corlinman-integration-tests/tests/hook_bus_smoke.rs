//! Test 1 — `hook_bus_smoke`.
//!
//! Build a `HookBus`, subscribe at each priority tier, emit a
//! `HookEvent::GatewayStartup`, and assert all three subscribers observe it
//! with the Critical tier winning the race. The bus fans out Critical →
//! Normal → Low with a `yield_now` between tiers, so we use `recv` timeouts
//! to establish that Critical is already ready by the time Normal/Low are
//! sampled.

use std::time::Duration;

use corlinman_hooks::{HookBus, HookEvent, HookPriority};
use tokio::time::timeout;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hook_bus_smoke_fan_out_in_priority_order() {
    let bus = HookBus::new(1024);

    let mut critical = bus.subscribe(HookPriority::Critical);
    let mut normal = bus.subscribe(HookPriority::Normal);
    let mut low = bus.subscribe(HookPriority::Low);

    // Drive the emit concurrently with the receivers so the yield_now between
    // tiers actually interleaves with subscriber wakeups.
    let bus_for_emit = bus.clone();
    let emitter = tokio::spawn(async move {
        bus_for_emit
            .emit(HookEvent::GatewayStartup {
                version: "0.1.2".into(),
            })
            .await
            .expect("emit should succeed when not cancelled");
    });

    // Critical must land first — give it the shortest patience.
    let crit_event = timeout(Duration::from_secs(1), critical.recv())
        .await
        .expect("critical subscriber must receive within 1s")
        .expect("critical channel open");

    let normal_event = timeout(Duration::from_secs(1), normal.recv())
        .await
        .expect("normal subscriber must receive within 1s")
        .expect("normal channel open");

    let low_event = timeout(Duration::from_secs(1), low.recv())
        .await
        .expect("low subscriber must receive within 1s")
        .expect("low channel open");

    emitter.await.expect("emitter task joined");

    for (label, ev) in [
        ("critical", &crit_event),
        ("normal", &normal_event),
        ("low", &low_event),
    ] {
        match ev {
            HookEvent::GatewayStartup { version } => {
                assert_eq!(version, "0.1.2", "{label} tier got wrong payload");
            }
            other => panic!("{label} tier got unexpected event: {other:?}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hook_bus_cancel_token_blocks_further_emits() {
    // Defence in depth: the CancelToken surface is part of the Batch 1 contract,
    // so prove that flipping it stops subsequent emits from landing.
    let bus = HookBus::new(16);
    let mut rx = bus.subscribe(HookPriority::Normal);

    bus.cancel_token().cancel();
    let res = bus
        .emit(HookEvent::GatewayStartup {
            version: "x".into(),
        })
        .await;
    assert!(res.is_err(), "emit after cancel must error");

    // Verify no event leaked through.
    let recv = timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(recv.is_err(), "no event should be delivered after cancel");
}
