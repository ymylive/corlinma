//! `HookBus` + `HookSubscription`.
//!
//! Internally, the bus holds one `tokio::sync::broadcast::Sender` per
//! priority tier. `emit` publishes in strict tier order (Critical →
//! Normal → Low) so Critical subscribers always observe an event before
//! lower-priority ones. Each tier is awaited via `yield_now` between
//! sends to give subscribers a scheduling opportunity.

use tokio::sync::broadcast;
use tracing::Instrument;

use crate::error::{HookError, RecvError};
use crate::event::HookEvent;
use crate::priority::{CancelToken, HookPriority};

/// Cross-cutting event bus. Clone-cheap (all state is behind `Arc`s via
/// the broadcast senders, which are themselves cloneable).
#[derive(Debug, Clone)]
pub struct HookBus {
    critical: broadcast::Sender<HookEvent>,
    normal: broadcast::Sender<HookEvent>,
    low: broadcast::Sender<HookEvent>,
    cancel: CancelToken,
}

impl HookBus {
    /// Build a new bus with `capacity` slots per priority tier.
    pub fn new(capacity: usize) -> Self {
        let (critical, _) = broadcast::channel(capacity);
        let (normal, _) = broadcast::channel(capacity);
        let (low, _) = broadcast::channel(capacity);
        Self {
            critical,
            normal,
            low,
            cancel: CancelToken::new(),
        }
    }

    /// Clone of the bus-wide cancel token. Flipping it stops future
    /// `emit` calls from publishing.
    pub fn cancel_token(&self) -> CancelToken {
        self.cancel.clone()
    }

    fn sender(&self, priority: HookPriority) -> &broadcast::Sender<HookEvent> {
        match priority {
            HookPriority::Critical => &self.critical,
            HookPriority::Normal => &self.normal,
            HookPriority::Low => &self.low,
        }
    }

    /// Subscribe to a priority tier. The subscription only sees events
    /// published to its tier, but tiers are fed in strict order by
    /// `emit`, so a Critical subscriber is guaranteed to observe the
    /// event before any Normal/Low subscriber.
    pub fn subscribe(&self, priority: HookPriority) -> HookSubscription {
        HookSubscription {
            priority,
            rx: self.sender(priority).subscribe(),
        }
    }

    /// Emit in strict priority order. Returns `HookError::Cancelled` if
    /// the cancel token has been flipped by the time we start. Having no
    /// subscribers on a tier is not an error; the send result is
    /// ignored intentionally (broadcast returns `Err` only when there
    /// are zero receivers, which is fine).
    pub async fn emit(&self, event: HookEvent) -> Result<(), HookError> {
        if self.cancel.is_cancelled() {
            return Err(HookError::Cancelled);
        }

        let critical_rx = self.critical.receiver_count();
        let normal_rx = self.normal.receiver_count();
        let low_rx = self.low.receiver_count();
        let priority_tier_count =
            (critical_rx > 0) as u8 + (normal_rx > 0) as u8 + (low_rx > 0) as u8;

        // Refresh the gauge every emit so the per-priority subscriber count
        // tracks churn without needing a separate subscribe/unsubscribe hook.
        corlinman_core::metrics::HOOK_SUBSCRIBERS_CURRENT
            .with_label_values(&["critical"])
            .set(critical_rx as i64);
        corlinman_core::metrics::HOOK_SUBSCRIBERS_CURRENT
            .with_label_values(&["normal"])
            .set(normal_rx as i64);
        corlinman_core::metrics::HOOK_SUBSCRIBERS_CURRENT
            .with_label_values(&["low"])
            .set(low_rx as i64);

        let span = tracing::info_span!(
            "hook_emit",
            event_kind = event.kind(),
            session_key = event.session_key().unwrap_or(""),
            priority_tier_count = priority_tier_count as u64,
        );

        let critical = self.critical.clone();
        let normal = self.normal.clone();
        let low = self.low.clone();
        let cancel = self.cancel.clone();
        let kind = event.kind();

        async move {
            for tier in HookPriority::ordered() {
                if cancel.is_cancelled() {
                    return Err(HookError::Cancelled);
                }
                let (sender, label) = match tier {
                    HookPriority::Critical => (&critical, "critical"),
                    HookPriority::Normal => (&normal, "normal"),
                    HookPriority::Low => (&low, "low"),
                };
                // Broadcast's `send` returns `Err` only when the channel
                // has no subscribers — that's a no-op from our POV. We
                // still count the fan-out attempt per tier so operators
                // can see per-priority hook volume.
                let _ = sender.send(event.clone());
                corlinman_core::metrics::HOOK_EMITS_TOTAL
                    .with_label_values(&[kind, label])
                    .inc();
                // Yield so subscribers on this tier can drain before we
                // publish to the next tier. This is what enforces the
                // ordering guarantee in the "subscriber observes event"
                // sense on a multi-threaded runtime.
                tokio::task::yield_now().await;
            }
            Ok(())
        }
        .instrument(span)
        .await
    }

    /// Fire-and-forget variant. Skips the span overhead and never
    /// awaits. Useful from sync contexts (e.g. Drop, config-reload
    /// callbacks) where blocking on scheduler yields isn't possible.
    pub fn emit_nonblocking(&self, event: HookEvent) {
        if self.cancel.is_cancelled() {
            return;
        }
        let kind = event.kind();
        for tier in HookPriority::ordered() {
            let sender = self.sender(tier);
            let label = match tier {
                HookPriority::Critical => "critical",
                HookPriority::Normal => "normal",
                HookPriority::Low => "low",
            };
            let _ = sender.send(event.clone());
            corlinman_core::metrics::HOOK_EMITS_TOTAL
                .with_label_values(&[kind, label])
                .inc();
        }
    }
}

/// A handle to one priority tier of the bus. Dropping it removes the
/// slot from the underlying broadcast channel; other subscribers and
/// the emitter are unaffected.
#[derive(Debug)]
pub struct HookSubscription {
    priority: HookPriority,
    rx: broadcast::Receiver<HookEvent>,
}

impl HookSubscription {
    pub fn priority(&self) -> HookPriority {
        self.priority
    }

    /// Await the next event on this tier. Translates `broadcast`'s
    /// `RecvError` into our local type so callers don't import `tokio`.
    pub async fn recv(&mut self) -> Result<HookEvent, RecvError> {
        self.rx.recv().await.map_err(Into::into)
    }
}
