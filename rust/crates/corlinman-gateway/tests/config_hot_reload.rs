//! B5-BE3 integration tests for `ConfigWatcher`.
//!
//! Covers the pieces of the hot-reload pipeline that don't belong in the
//! `config_watcher` module's own `#[cfg(test)] mod tests` — end-to-end wiring
//! (SIGHUP path, notify-driven fs event path, admin endpoint) plus the
//! subtle "what happens if parsing fails?" / "what happens on a restart-
//! required section?" contracts.
//!
//! Flake prevention for fs events
//! ------------------------------
//! `notify`'s macOS FSEvents backend can take 50-200ms to register new
//! watches on CI runners, and even longer to emit the first event after a
//! write. Every fs-triggered test therefore:
//!   * waits for the watcher to install (sleep 200ms after `ConfigWatcher::
//!     run` is spawned),
//!   * rewrites the file *atomically* (write temp + rename) so notify
//!     always observes a single `Create/Modify` pair,
//!   * polls the `HookSubscription` with a generous 5-second timeout
//!     rather than relying on fixed sleeps — if the reload happens, the
//!     test passes quickly; if it doesn't, the test fails with a clear
//!     timeout message instead of a flaky race.
//!
//! The admin endpoint test + the `trigger_reload`-direct tests bypass
//! `notify` entirely, so they're the authoritative coverage for the diff
//! and emit logic. The fs path is asserted once to prove wiring works.

use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use base64::Engine;
use corlinman_core::config::{Config, ProviderEntry, SecretRef};
use corlinman_gateway::config_watcher::ConfigWatcher;
use corlinman_gateway::routes::admin::{router_with_state, AdminState};
use corlinman_hooks::{HookBus, HookEvent, HookPriority};
use corlinman_plugins::registry::PluginRegistry;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ADMIN_USER: &str = "admin";
const ADMIN_PASS: &str = "secret";

fn hash_password(password: &str) -> String {
    use argon2::password_hash::{PasswordHasher, SaltString};
    let salt = SaltString::encode_b64(b"corlinman_test_salt_bytes_16").unwrap();
    argon2::Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn admin_basic_header() -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{ADMIN_USER}:{ADMIN_PASS}"))
    )
}

fn base_config() -> Config {
    let mut cfg = Config::default();
    cfg.admin.username = Some(ADMIN_USER.into());
    cfg.admin.password_hash = Some(hash_password(ADMIN_PASS));
    cfg.providers.insert(
        "anthropic",
        ProviderEntry {
            api_key: Some(SecretRef::EnvVar {
                env: "ANTHROPIC_API_KEY".into(),
            }),
            enabled: true,
            ..Default::default()
        },
    );
    cfg
}

fn write_config(path: &std::path::Path, cfg: &Config) {
    // Atomic rewrite: write to sidecar, rename. Matches the admin POST handler
    // so tests don't see a half-written file mid-inotify event.
    let text = toml::to_string_pretty(cfg).unwrap();
    let mut tmp = path.to_path_buf();
    tmp.as_mut_os_string().push(".tmp");
    std::fs::write(&tmp, text).unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

/// Wait up to `timeout` for the next `ConfigChanged` event with `section ==
/// wanted`. Returns the matched event or None on timeout.
async fn await_change(
    sub: &mut corlinman_hooks::HookSubscription,
    wanted: &str,
    timeout: Duration,
) -> Option<HookEvent> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = match deadline.checked_duration_since(tokio::time::Instant::now()) {
            Some(r) if !r.is_zero() => r,
            _ => return None,
        };
        match tokio::time::timeout(remaining, sub.recv()).await {
            Err(_) => return None,
            Ok(Err(_)) => continue,
            Ok(Ok(ev)) => {
                if let HookEvent::ConfigChanged { section, .. } = &ev {
                    if section == wanted {
                        return Some(ev);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Test 1 — fs modify triggers reload and emits ConfigChanged hook
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fs_modify_triggers_reload_and_emits_changed_hook() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    let initial = base_config();
    write_config(&path, &initial);

    let bus = Arc::new(HookBus::new(64));
    let watcher = Arc::new(ConfigWatcher::new(path.clone(), initial, bus.clone()));
    let cancel = CancellationToken::new();
    let task = {
        let w = watcher.clone();
        let c = cancel.clone();
        tokio::spawn(async move { w.run(c).await })
    };

    // Give notify time to install (FSEvents is notoriously slow to arm).
    tokio::time::sleep(Duration::from_millis(250)).await;

    let mut sub = bus.subscribe(HookPriority::Normal);

    // Mutate a hot-reloadable section (models.default) and rewrite.
    let mut next = base_config();
    next.models.default = "claude-opus-4-7".into();
    write_config(&path, &next);

    // 5s is generous: typical turnaround on a dev mac is <500ms, CI ~1.5s.
    let event = await_change(&mut sub, "models", Duration::from_secs(5))
        .await
        .expect("expected ConfigChanged{section=models}");
    if let HookEvent::ConfigChanged { new, .. } = event {
        assert_eq!(
            new.get("default").and_then(Value::as_str),
            Some("claude-opus-4-7")
        );
    } else {
        panic!("wrong event variant");
    }
    assert_eq!(watcher.current().models.default, "claude-opus-4-7");

    cancel.cancel();
    let _ = task.await;
}

// ---------------------------------------------------------------------------
// Test 2 — malformed TOML does not swap and reports error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn malformed_toml_does_not_swap_and_reports_error() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    let initial = base_config();
    write_config(&path, &initial);

    let bus = Arc::new(HookBus::new(16));
    let watcher = Arc::new(ConfigWatcher::new(path.clone(), initial, bus.clone()));

    // Stomp the file with garbage.
    std::fs::write(&path, "::garbage:: not = toml = here").unwrap();

    let report = watcher.trigger_reload().await.unwrap();
    assert!(!report.errors.is_empty(), "expected parse error");
    assert!(report.changed_sections.is_empty());
    // Snapshot must be unchanged.
    assert_eq!(
        watcher.current().models.default,
        Config::default().models.default
    );
}

// ---------------------------------------------------------------------------
// Test 3 — validation failure does not swap
// ---------------------------------------------------------------------------

#[tokio::test]
async fn validation_failure_does_not_swap() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    let initial = base_config();
    write_config(&path, &initial);

    let bus = Arc::new(HookBus::new(16));
    let watcher = Arc::new(ConfigWatcher::new(path.clone(), initial, bus.clone()));

    // server.port = 0 is outside validator-derive's `range(min=1)` → error.
    let bad = r#"
[server]
port = 0
bind = "0.0.0.0"
data_dir = "/tmp/corlinman-test"

[models]
default = "claude-sonnet-4-5"
"#;
    std::fs::write(&path, bad).unwrap();

    let report = watcher.trigger_reload().await.unwrap();
    assert!(
        !report.errors.is_empty(),
        "expected a validation error, got {report:?}",
    );
    assert!(report.changed_sections.is_empty());
    // Snapshot preserved.
    assert_eq!(watcher.current().server.port, Config::default().server.port);
}

// ---------------------------------------------------------------------------
// Test 4 — restart-required section change flags warning
// ---------------------------------------------------------------------------

#[tokio::test]
async fn restart_required_section_change_flags_warning() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    let initial = base_config();
    write_config(&path, &initial);

    let bus = Arc::new(HookBus::new(64));
    let watcher = Arc::new(ConfigWatcher::new(path.clone(), initial, bus.clone()));

    let mut sub = bus.subscribe(HookPriority::Normal);

    // Bump server.port — that's in RESTART_REQUIRED_SECTIONS.
    let mut next = base_config();
    next.server.port = 7777;
    write_config(&path, &next);

    let report = watcher.trigger_reload().await.unwrap();
    assert!(report.errors.is_empty(), "unexpected errors: {report:?}");
    assert!(
        report.changed_sections.iter().any(|s| s == "server"),
        "expected `server` in changed_sections, got {:?}",
        report.changed_sections,
    );

    // Drain expected events: first `server`, then the `server.restart_required`
    // marker. Both must surface within a generous timeout; `BTreeSet` ordering
    // in `diff_sections` makes `server` come first.
    let _ = await_change(&mut sub, "server", Duration::from_secs(2))
        .await
        .expect("missed `server` ConfigChanged");
    let _ = await_change(&mut sub, "server.restart_required", Duration::from_secs(2))
        .await
        .expect("missed `server.restart_required` warning");

    // Snapshot is still swapped — restart_required is additive.
    assert_eq!(watcher.current().server.port, 7777);
}

// ---------------------------------------------------------------------------
// Test 5 — unchanged reload is a no-op
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unchanged_reload_is_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    let cfg = base_config();
    write_config(&path, &cfg);

    let bus = Arc::new(HookBus::new(16));
    let watcher = Arc::new(ConfigWatcher::new(path.clone(), cfg, bus.clone()));

    let mut sub = bus.subscribe(HookPriority::Normal);
    let report = watcher.trigger_reload().await.unwrap();
    assert!(report.is_noop(), "expected noop, got {report:?}");
    // No hook events.
    assert!(tokio::time::timeout(Duration::from_millis(100), sub.recv())
        .await
        .is_err());
}

// ---------------------------------------------------------------------------
// Test 6 — manual trigger via admin endpoint
// ---------------------------------------------------------------------------

async fn body_json(resp: axum::response::Response) -> Value {
    let b = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&b).unwrap()
}

#[tokio::test]
async fn manual_trigger_via_admin_endpoint_works() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    let initial = base_config();
    write_config(&path, &initial);

    let bus = Arc::new(HookBus::new(64));
    let watcher = Arc::new(ConfigWatcher::new(
        path.clone(),
        initial.clone(),
        bus.clone(),
    ));

    // Build the admin router against the same ArcSwap the watcher owns so
    // `/admin/config/reload`-initiated swaps propagate to `/admin/config`
    // readers on the same state.
    let config_handle: Arc<ArcSwap<Config>> = watcher.arc_swap();
    let state = AdminState::new(Arc::new(PluginRegistry::default()), config_handle.clone())
        .with_config_path(path.clone())
        .with_config_watcher(watcher.clone());
    let app = router_with_state(state);

    // Mutate on disk, then hit /admin/config/reload.
    let mut next = base_config();
    next.models.default = "claude-opus-4-7".into();
    next.models
        .aliases
        .insert("smart".into(), "claude-opus-4-7".into());
    write_config(&path, &next);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/config/reload")
                .header(header::AUTHORIZATION, admin_basic_header())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    let changed: Vec<String> = v["changed_sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect();
    assert!(changed.contains(&"models".to_string()), "got {changed:?}");
    assert!(v["errors"].as_array().unwrap().is_empty());

    // Admin GET sees the swap (same ArcSwap is shared).
    assert_eq!(config_handle.load().models.default, "claude-opus-4-7");

    // Unauth request rejected — /admin/config/reload sits behind the guard.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/config/reload")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// Test 7 — SIGHUP triggers reload (Unix-only)
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[tokio::test]
async fn sighup_triggers_reload() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    let initial = base_config();
    write_config(&path, &initial);

    let bus = Arc::new(HookBus::new(64));
    let watcher = Arc::new(ConfigWatcher::new(path.clone(), initial, bus.clone()));
    let cancel = CancellationToken::new();
    let task = {
        let w = watcher.clone();
        let c = cancel.clone();
        tokio::spawn(async move { w.run(c).await })
    };

    // Let the SIGHUP handler register. Without this sleep, the raise happens
    // before `signal(SignalKind::hangup())` arms and tokio silently drops it.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut sub = bus.subscribe(HookPriority::Normal);

    // Rewrite the file with a hot-reloadable change, then raise SIGHUP.
    let mut next = base_config();
    next.models.default = "claude-opus-4-7".into();
    write_config(&path, &next);

    // Raise SIGHUP on the current process. Avoid adding `libc` / `nix` to
    // dev-deps by shelling out to `kill -HUP <our-pid>` — POSIX `kill(1)` is
    // always present on any machine the gateway runs on.
    let pid = std::process::id();
    let status = std::process::Command::new("kill")
        .arg("-HUP")
        .arg(pid.to_string())
        .status()
        .expect("failed to run kill -HUP");
    assert!(status.success(), "kill -HUP exited non-zero: {status}");

    let _ev = await_change(&mut sub, "models", Duration::from_secs(5))
        .await
        .expect("expected models ConfigChanged after SIGHUP");
    assert_eq!(watcher.current().models.default, "claude-opus-4-7");

    cancel.cancel();
    let _ = task.await;
}
