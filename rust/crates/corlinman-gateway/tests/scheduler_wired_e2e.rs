//! Phase 3.1 smoke test: prove the gateway crate can pull in
//! `corlinman_scheduler` and that a `*/1 * * * * * *` cron actually
//! triggers a subprocess within a few seconds.
//!
//! The unit suite inside `corlinman-scheduler` covers the dispatch
//! branches in isolation; this test specifically guards against the
//! failure mode that prompted the fix — a scheduler crate that compiles
//! but is never wired into the gateway. Living in
//! `corlinman-gateway/tests/` means a missing `corlinman-scheduler`
//! dependency on the gateway would fail this build.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use corlinman_core::config::{JobAction, SchedulerConfig, SchedulerJob};
use corlinman_hooks::HookBus;
use tokio_util::sync::CancellationToken;

/// Ensure tmpfile collisions across CI runs don't false-flag the assert
/// — every test gets a unique sentinel path. We can't use the spec's
/// `$$` shell expansion form because `Command::spawn` doesn't run the
/// argument through a shell; instead we hand the path explicitly so
/// the smoke is self-contained.
fn unique_sentinel() -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("corlinman-scheduler-smoke-{pid}-{nanos}"))
}

#[tokio::test]
async fn one_second_cron_actually_fires_subprocess() {
    let sentinel = unique_sentinel();
    // Belt-and-braces — clear any earlier file from this test path.
    let _ = std::fs::remove_file(&sentinel);

    let cfg = SchedulerConfig {
        jobs: vec![SchedulerJob {
            name: "smoke".into(),
            cron: "*/1 * * * * * *".into(),
            timezone: None,
            action: JobAction::Subprocess {
                command: "/bin/sh".into(),
                args: vec!["-c".into(), format!("echo HELLO > {}", sentinel.display())],
                timeout_secs: 5,
                working_dir: Some(PathBuf::from("/tmp")),
                env: Default::default(),
            },
        }],
    };

    let bus = Arc::new(HookBus::new(16));
    let cancel = CancellationToken::new();
    let handle = corlinman_scheduler::spawn(&cfg, bus, cancel.clone());
    assert_eq!(
        handle.handles.len(),
        1,
        "exactly one job should have been spawned",
    );

    // Up to 3 seconds of patience — the cron evaluates to "every second"
    // so any single-second window must catch a fire.
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        if sentinel.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    cancel.cancel();
    // Cancellation should drain handles within a couple of seconds even
    // if a subprocess was mid-flight (kill_on_drop kills it).
    tokio::time::timeout(Duration::from_secs(3), handle.join_all())
        .await
        .expect("scheduler drained after cancel");

    assert!(
        sentinel.exists(),
        "sentinel file {} was never created — the cron job didn't fire",
        sentinel.display(),
    );

    // Cleanup; ignore errors if some other test already collected it.
    let _ = std::fs::remove_file(&sentinel);
}
