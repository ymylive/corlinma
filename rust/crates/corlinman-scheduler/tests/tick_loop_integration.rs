//! Integration test: a per-second cron job that successfully exits 0
//! must emit `HookEvent::EngineRunCompleted` at least 3 times in 5
//! seconds. Validates the end-to-end tick path (cron parse → sleep_until →
//! dispatch → bus emit) without involving the gateway crate.
//!
//! The cron expression `"* * * * * * *"` is the 7-field `cron` crate's
//! "every second" — exact same parser used by the doctor checks, so we
//! aren't introducing a second cron dialect just to test fast-firing.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use corlinman_core::config::{JobAction, SchedulerConfig, SchedulerJob};
use corlinman_hooks::{HookBus, HookEvent, HookPriority, RecvError};
use corlinman_scheduler::spawn;
use tokio_util::sync::CancellationToken;

#[tokio::test(flavor = "multi_thread")]
async fn per_second_cron_fires_multiple_times_in_five_seconds() {
    let bus = Arc::new(HookBus::new(64));
    let mut sub = bus.subscribe(HookPriority::Normal);
    let cancel = CancellationToken::new();

    let cfg = SchedulerConfig {
        jobs: vec![SchedulerJob {
            name: "tick".into(),
            cron: "* * * * * * *".into(), // every second
            timezone: None,
            action: JobAction::Subprocess {
                command: "true".into(),
                args: vec![],
                timeout_secs: 5,
                working_dir: None,
                env: BTreeMap::new(),
            },
        }],
    };

    let handle = spawn(&cfg, bus.clone(), cancel.clone());

    // Drain `EngineRunCompleted` events for 5 seconds. We require at
    // least 3 — a 2-fire allowance for boot scheduling jitter (the loop
    // might first sleep up to ~1s before the very first firing).
    let collected = tokio::time::timeout(Duration::from_secs(6), async {
        let mut count = 0usize;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if tokio::time::Instant::now() >= deadline {
                break count;
            }
            match tokio::time::timeout(Duration::from_millis(500), sub.recv()).await {
                Ok(Ok(HookEvent::EngineRunCompleted { .. })) => count += 1,
                Ok(Ok(_other)) => {}
                Ok(Err(RecvError::Lagged(_))) => {}
                Ok(Err(RecvError::Closed)) => break count,
                Err(_) => {}
            }
        }
    })
    .await
    .expect("loop didn't hang");

    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(3), handle.join_all()).await;

    assert!(
        collected >= 3,
        "expected >= 3 firings in 5s, got {collected}",
    );
}
