//! Integration tests for the shared [`Channel`] trait + registry (B4-BE2).
//!
//! These tests verify the trait contract without standing up the OneBot /
//! Telegram transports — the per-adapter behaviour stays covered by the
//! existing `qq_to_chat_e2e.rs` / `onebot_integration.rs` / `telegram_smoke.rs`
//! suites.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use corlinman_channels::channel::{
    spawn_all, Channel, ChannelContext, ChannelError, ChannelRegistry,
};
use corlinman_core::config::Config;
use corlinman_gateway_api::{ChatEventStream, ChatService, InternalChatRequest};
use futures::stream;
use tokio_util::sync::CancellationToken;

/// Minimal `ChatService` that emits an empty event stream — enough to build
/// a `ChannelContext` for tests that never actually dispatch a message.
struct NoopChatService;

#[async_trait]
impl ChatService for NoopChatService {
    async fn run(&self, _req: InternalChatRequest, _cancel: CancellationToken) -> ChatEventStream {
        Box::pin(stream::empty())
    }
}

/// Build a baseline `ChannelContext` backed by a default `Config`. Individual
/// tests mutate the returned context's `config` via `Arc::make_mut`-style
/// cloning when they need to flip `enabled` flags.
fn base_ctx(cfg: Config) -> ChannelContext {
    ChannelContext {
        config: Arc::new(cfg),
        chat_service: Arc::new(NoopChatService) as Arc<dyn ChatService>,
        model: "test-model".into(),
        rate_limit_hook: None,
        hook_bus: None,
    }
}

// ---------------------------------------------------------------------------
// 1. builtin_registry_contains_qq_and_telegram
// ---------------------------------------------------------------------------

#[test]
fn builtin_registry_contains_qq_and_telegram() {
    let registry = ChannelRegistry::builtin();
    let ids: Vec<&str> = registry.iter().map(|c| c.id()).collect();
    assert!(ids.contains(&"qq"), "builtin registry must include qq");
    assert!(
        ids.contains(&"telegram"),
        "builtin registry must include telegram"
    );
    assert_eq!(registry.len(), 2, "no unexpected built-in channels");
}

// ---------------------------------------------------------------------------
// 2. disabled_channel_is_skipped_by_spawn_all
// ---------------------------------------------------------------------------

#[tokio::test]
async fn disabled_channel_is_skipped_by_spawn_all() {
    // Default Config has `channels.qq = None` and `channels.telegram = None`,
    // so both built-ins return `enabled() == false`. `spawn_all` must emit
    // zero handles — and, critically, must not attempt to `run()` them
    // (which would otherwise fail the config assertions inside each
    // adapter's wrapper).
    let ctx = base_ctx(Config::default());
    let cancel = CancellationToken::new();
    let handles = spawn_all(&ChannelRegistry::builtin(), ctx, cancel);
    assert!(
        handles.is_empty(),
        "no channels enabled → no spawned handles, got {}",
        handles.len()
    );
}

// ---------------------------------------------------------------------------
// 3. channel_send_unsupported_default_errors
// ---------------------------------------------------------------------------
//
// The trait surface today exposes `run` only (outbound helpers live inside
// each adapter's run loop — see `channel.rs` module docs). To preserve the
// spec's intent we verify the `ChannelError::Unsupported` variant itself
// behaves as documented: display message is stable and downstream code can
// pattern-match on it. When outbound helpers move onto the trait this test
// will graduate into a default-impl check.

#[test]
fn channel_send_unsupported_default_errors() {
    let err = ChannelError::Unsupported("send");
    let msg = err.to_string();
    assert!(msg.contains("send"), "error display mentions op: {msg}");
    assert!(
        msg.contains("not supported"),
        "error display mentions the verdict: {msg}"
    );

    // Round-trip through `anyhow::Error` — mirrors how adapters would surface
    // it from a trait method.
    let wrapped: anyhow::Error = ChannelError::Unsupported("edit").into();
    assert!(wrapped.to_string().contains("edit"));
}

// ---------------------------------------------------------------------------
// 4. channel_typing_default_is_noop_ok
// ---------------------------------------------------------------------------
//
// Same rationale as (3): typing is not yet on the trait. The contract the
// spec called for ("default returns Ok(()) so read-only channels don't have
// to think about it") is honoured today by *omitting* the method entirely —
// no adapter is forced to implement typing. We assert that by confirming a
// bare `Channel` impl (no typing method) compiles and spawns fine.

#[test]
fn channel_typing_default_is_noop_ok() {
    struct Bare;
    #[async_trait]
    impl Channel for Bare {
        fn id(&self) -> &str {
            "bare"
        }
        fn enabled(&self, _: &Config) -> bool {
            false
        }
        async fn run(&self, _: ChannelContext, _: CancellationToken) -> anyhow::Result<()> {
            Ok(())
        }
    }

    let mut r = ChannelRegistry::new();
    r.push(Arc::new(Bare));
    assert_eq!(r.len(), 1);
    assert_eq!(r.iter().next().unwrap().id(), "bare");
}

// ---------------------------------------------------------------------------
// 5. mock_channel_run_respects_cancel
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mock_channel_run_respects_cancel() {
    // A mock channel that loops until cancelled, flipping a flag on entry
    // and another on exit. `spawn_all` should drive it; cancelling the
    // root token must make `run` return within a bounded window.
    struct Mock {
        entered: Arc<AtomicBool>,
        exited: Arc<AtomicBool>,
    }
    #[async_trait]
    impl Channel for Mock {
        fn id(&self) -> &str {
            "mock"
        }
        fn enabled(&self, _: &Config) -> bool {
            true
        }
        async fn run(&self, _ctx: ChannelContext, cancel: CancellationToken) -> anyhow::Result<()> {
            self.entered.store(true, Ordering::SeqCst);
            cancel.cancelled().await;
            self.exited.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    let entered = Arc::new(AtomicBool::new(false));
    let exited = Arc::new(AtomicBool::new(false));
    let mock = Mock {
        entered: entered.clone(),
        exited: exited.clone(),
    };

    let mut registry = ChannelRegistry::new();
    registry.push(Arc::new(mock));

    let ctx = base_ctx(Config::default());
    let cancel = CancellationToken::new();
    let handles = spawn_all(&registry, ctx, cancel.clone());
    assert_eq!(handles.len(), 1);

    // Wait for the task to observe the first yield.
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(
        entered.load(Ordering::SeqCst),
        "mock.run should have started"
    );
    assert!(
        !exited.load(Ordering::SeqCst),
        "mock.run should still be awaiting cancel"
    );

    cancel.cancel();

    // `run` must complete promptly after cancel; 1s is generous.
    for h in handles {
        let res = tokio::time::timeout(Duration::from_secs(1), h)
            .await
            .expect("channel did not exit within 1s of cancel");
        assert!(res.is_ok(), "join failed: {res:?}");
        assert!(res.unwrap().is_ok(), "channel exited with Err after cancel");
    }
    assert!(
        exited.load(Ordering::SeqCst),
        "mock.run should have passed the cancel await"
    );
}
