//! Priority tiers + cooperative cancel.
//!
//! The bus fans out one tier at a time, oldest-priority first, so a
//! Critical hook always observes an event before a Normal or Low one
//! does. `CancelToken` is a cheap `AtomicBool` wrapped in `Arc`: emitters
//! check it before publishing each tier, and external code flips it to
//! signal "stop emitting new events".

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Subscribers pick a tier when they subscribe; `emit` publishes in the
/// order `Critical → Normal → Low`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookPriority {
    Critical,
    Normal,
    Low,
}

impl HookPriority {
    /// Iteration order used by `HookBus::emit`. Critical first, Low last.
    pub(crate) fn ordered() -> [HookPriority; 3] {
        [
            HookPriority::Critical,
            HookPriority::Normal,
            HookPriority::Low,
        ]
    }
}

/// Cooperative cancellation flag shared between emitter and subscribers.
///
/// Cheap to clone (it's an `Arc<AtomicBool>`). Emitters check `is_cancelled`
/// before publishing; callers flip it via `cancel()` to drain the bus.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}
