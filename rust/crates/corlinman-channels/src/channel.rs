//! Shared [`Channel`] trait + [`ChannelRegistry`] (B4-BE2).
//!
//! The two existing adapters ([`crate::service::run_qq_channel`] and
//! [`crate::telegram::run_telegram_channel`]) were wired ad-hoc from
//! `corlinman-gateway::main::maybe_spawn_*_channel`. This module extracts a
//! uniform contract so:
//!
//! 1. New inbound transports follow a single trait.
//! 2. The gateway spawns every enabled channel via one iteration
//!    ([`spawn_all`]) instead of bespoke helpers.
//! 3. Per-channel behaviour is unchanged — [`QqChannel`] / [`TelegramChannel`]
//!    are thin wrappers that forward [`Channel::run`] to the existing
//!    `run_*_channel` function bodies. No regression in the hot path.
//!
//! ## Why the trait surface is minimal
//!
//! The spec sketched `send` / `edit` / `typing` / `send_media` methods on the
//! trait, but today the reply path lives *inside* each adapter (it owns the
//! WS action channel / the Telegram reply mpsc). Exposing those as trait
//! methods now would require tearing out both adapters' internals — a change
//! the parent task explicitly forbids ("DO NOT touch qq/onebot.rs or
//! qq/service.rs internals — wrap them"). Instead the trait exposes only the
//! stable surface the gateway actually consumes (`id`, `enabled`, `run`);
//! outbound helpers can be added later with default `Unsupported` impls once
//! the adapters are refactored to thread sends through a shared channel.
//!
//! ## `ChannelError::Unsupported`
//!
//! Reserved for future outbound helpers on read-only channels. Kept here so
//! downstream crates can rely on the error variant name without churn when
//! those helpers land.

use std::sync::Arc;

use corlinman_core::config::Config;
use corlinman_gateway_api::ChatService;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::router::RateLimitHook;

/// Per-channel runtime handle shared by the gateway at spawn time.
///
/// `Clone` so `spawn_all` can hand each enabled adapter its own copy without
/// forcing callers to re-build it. All fields are `Arc`-wrapped or cheap to
/// clone.
#[derive(Clone)]
pub struct ChannelContext {
    /// Full config snapshot; each adapter pulls its own `[channels.*]`
    /// sub-section inside [`Channel::enabled`] + [`Channel::run`].
    pub config: Arc<Config>,
    /// Shared chat pipeline the gateway built on top of its `ChatBackend`.
    pub chat_service: Arc<dyn ChatService>,
    /// Default model id (`cfg.models.default`) for channels whose inbound
    /// events carry no model hint.
    pub model: String,
    /// Optional observation hook fired by the router each time a message is
    /// dropped by a rate-limit check. `None` in tests; `Some(..)` in prod
    /// where the gateway wires it to a Prometheus CounterVec.
    pub rate_limit_hook: Option<RateLimitHook>,
    /// Optional shared hook bus (B4-BE6). Threaded through to the router
    /// so rate-limit rejections surface on
    /// [`corlinman_hooks::HookEvent::RateLimitTriggered`] in addition to
    /// the legacy `rate_limit_hook` callback.
    pub hook_bus: Option<Arc<corlinman_hooks::HookBus>>,
}

/// Inbound channel adapter contract.
///
/// Implementations are constructed once at gateway boot and stored as
/// `Arc<dyn Channel>` in a [`ChannelRegistry`]. For each enabled channel the
/// gateway calls [`Channel::run`] on a dedicated task; the returned future
/// must honour the [`CancellationToken`] so shutdown drains in bounded time.
#[async_trait::async_trait]
pub trait Channel: Send + Sync + 'static {
    /// Short stable id (`"qq"`, `"telegram"`). Used for logging, metrics
    /// labels, and registry lookup.
    fn id(&self) -> &str;

    /// Human-readable name for admin UI / logs. Defaults to [`Self::id`].
    fn display_name(&self) -> &str {
        self.id()
    }

    /// Whether this channel is enabled for the given config snapshot.
    /// Called once per boot by [`spawn_all`].
    fn enabled(&self, cfg: &Config) -> bool;

    /// Run the adapter to completion or cancellation. `Ok(())` = graceful
    /// exit; `Err` = fatal configuration / transport error surfaced to the
    /// caller.
    async fn run(&self, ctx: ChannelContext, cancel: CancellationToken) -> anyhow::Result<()>;
}

/// Error surface reserved for future outbound helpers. Today only
/// [`ChannelError::Unsupported`] is used by mock impls in tests; the
/// adapters' internal outbound paths still live inside their `run_*_channel`
/// bodies (see module docs for rationale).
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    /// This channel does not implement the requested operation.
    #[error("operation {0} not supported by this channel")]
    Unsupported(&'static str),
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Ordered set of [`Channel`] impls the gateway will try to spawn at boot.
///
/// Order is insertion order; [`ChannelRegistry::builtin`] preserves
/// `qq` → `telegram` (matches the pre-refactor `main.rs` call order so log
/// output stays identical).
#[derive(Default, Clone)]
pub struct ChannelRegistry {
    channels: Vec<Arc<dyn Channel>>,
}

impl ChannelRegistry {
    /// Empty registry. Mostly useful for tests.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registry pre-populated with the built-in adapters: `qq`, `telegram`.
    pub fn builtin() -> Self {
        let mut r = Self::new();
        r.push(Arc::new(QqChannel));
        r.push(Arc::new(TelegramChannel));
        r
    }

    /// Append an adapter. External crates (future: Discord, Slack) can push
    /// their own impls into a registry before [`spawn_all`].
    pub fn push(&mut self, ch: Arc<dyn Channel>) {
        self.channels.push(ch);
    }

    /// Iterate registered adapters in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn Channel>> {
        self.channels.iter()
    }

    /// Count of registered adapters (regardless of enabled state).
    pub fn len(&self) -> usize {
        self.channels.len()
    }

    /// True when no adapters are registered.
    pub fn is_empty(&self) -> bool {
        self.channels.is_empty()
    }
}

/// Spawn one task per enabled channel and return the join handles.
///
/// Disabled channels ([`Channel::enabled`] returns `false`) are skipped
/// without spawning; the returned `Vec` length matches the enabled count.
/// Each task's `JoinHandle` yields the channel's `run` result so the caller
/// can log per-channel failures on shutdown.
pub fn spawn_all(
    registry: &ChannelRegistry,
    ctx: ChannelContext,
    cancel: CancellationToken,
) -> Vec<JoinHandle<anyhow::Result<()>>> {
    registry
        .iter()
        .filter(|ch| ch.enabled(&ctx.config))
        .map(|ch| {
            let ch = Arc::clone(ch);
            let ctx = ctx.clone();
            let cancel = cancel.child_token();
            tokio::spawn(async move { ch.run(ctx, cancel).await })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Built-in adapters — thin wrappers around existing `run_*_channel` bodies.
// ---------------------------------------------------------------------------

/// QQ / OneBot v11 adapter. Forwards [`Channel::run`] to
/// [`crate::service::run_qq_channel`] so the runtime behaviour is bit-for-bit
/// identical to the pre-refactor `maybe_spawn_qq_channel` path.
pub struct QqChannel;

#[async_trait::async_trait]
impl Channel for QqChannel {
    fn id(&self) -> &str {
        "qq"
    }

    fn display_name(&self) -> &str {
        "QQ (OneBot v11)"
    }

    fn enabled(&self, cfg: &Config) -> bool {
        cfg.channels.qq.as_ref().map(|q| q.enabled).unwrap_or(false)
    }

    async fn run(&self, ctx: ChannelContext, cancel: CancellationToken) -> anyhow::Result<()> {
        // `enabled` is true so this `as_ref` always produces Some — but defend
        // against misuse by erroring instead of panicking.
        let qq_cfg = ctx
            .config
            .channels
            .qq
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("qq channel run() called but channels.qq is None"))?
            .clone();
        let params = crate::service::QqChannelParams {
            config: qq_cfg,
            model: ctx.model.clone(),
            chat_service: ctx.chat_service.clone(),
            rate_limit_hook: ctx.rate_limit_hook.clone(),
            hook_bus: ctx.hook_bus.clone(),
        };
        crate::service::run_qq_channel(params, cancel).await
    }
}

/// Telegram adapter. Forwards to [`crate::telegram::run_telegram_channel`].
///
/// B4-BE1 is adding webhook handling inside the `telegram/` module in
/// parallel; once their public API stabilises this wrapper can switch
/// between webhook / long-poll based on `cfg.channels.telegram.mode`
/// without touching the trait surface.
pub struct TelegramChannel;

#[async_trait::async_trait]
impl Channel for TelegramChannel {
    fn id(&self) -> &str {
        "telegram"
    }

    fn display_name(&self) -> &str {
        "Telegram"
    }

    fn enabled(&self, cfg: &Config) -> bool {
        cfg.channels
            .telegram
            .as_ref()
            .map(|t| t.enabled)
            .unwrap_or(false)
    }

    async fn run(&self, ctx: ChannelContext, cancel: CancellationToken) -> anyhow::Result<()> {
        let tg_cfg = ctx
            .config
            .channels
            .telegram
            .as_ref()
            .ok_or_else(|| {
                anyhow::anyhow!("telegram channel run() called but channels.telegram is None")
            })?
            .clone();
        let params = crate::telegram::TelegramParams {
            config: tg_cfg,
            chat_service: ctx.chat_service.clone(),
            model: ctx.model.clone(),
        };
        crate::telegram::run_telegram_channel(params, cancel).await
    }
}

// ---------------------------------------------------------------------------
// Unit tests — integration-style tests that exercise enable/skip logic live
// in `tests/trait_impl.rs`.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_push_and_iter_preserves_order() {
        // Two no-op channels with distinct ids; iter should yield them in
        // insertion order so `ChannelRegistry::builtin` can rely on it.
        struct StubA;
        struct StubB;
        #[async_trait::async_trait]
        impl Channel for StubA {
            fn id(&self) -> &str {
                "a"
            }
            fn enabled(&self, _: &Config) -> bool {
                false
            }
            async fn run(&self, _: ChannelContext, _: CancellationToken) -> anyhow::Result<()> {
                Ok(())
            }
        }
        #[async_trait::async_trait]
        impl Channel for StubB {
            fn id(&self) -> &str {
                "b"
            }
            fn enabled(&self, _: &Config) -> bool {
                false
            }
            async fn run(&self, _: ChannelContext, _: CancellationToken) -> anyhow::Result<()> {
                Ok(())
            }
        }

        let mut r = ChannelRegistry::new();
        r.push(Arc::new(StubA));
        r.push(Arc::new(StubB));
        let ids: Vec<&str> = r.iter().map(|c| c.id()).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[test]
    fn builtin_ordering_matches_pre_refactor_call_order() {
        let r = ChannelRegistry::builtin();
        let ids: Vec<&str> = r.iter().map(|c| c.id()).collect();
        assert_eq!(ids, vec!["qq", "telegram"]);
    }

    #[test]
    fn display_name_defaults_to_id() {
        struct S;
        #[async_trait::async_trait]
        impl Channel for S {
            fn id(&self) -> &str {
                "x"
            }
            fn enabled(&self, _: &Config) -> bool {
                false
            }
            async fn run(&self, _: ChannelContext, _: CancellationToken) -> anyhow::Result<()> {
                Ok(())
            }
        }
        assert_eq!(S.display_name(), "x");
    }
}
