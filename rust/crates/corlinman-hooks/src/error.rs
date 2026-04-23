//! Error types for the hook bus.

use thiserror::Error;

/// Failure modes when publishing an event.
#[derive(Debug, Error)]
pub enum HookError {
    /// The bus was cancelled before the event could be published.
    #[error("hook bus cancelled")]
    Cancelled,
}

/// Failure modes when a subscriber pulls from the bus. Mirrors
/// `tokio::sync::broadcast::error::RecvError` but owned by this crate so
/// consumers don't need a direct `tokio` dep.
#[derive(Debug, Error)]
pub enum RecvError {
    /// The sender side of this priority tier has been dropped (the bus
    /// itself is gone).
    #[error("hook bus closed")]
    Closed,
    /// The subscriber fell behind and `n` events were dropped.
    #[error("hook subscriber lagged by {0} events")]
    Lagged(u64),
}

impl From<tokio::sync::broadcast::error::RecvError> for RecvError {
    fn from(err: tokio::sync::broadcast::error::RecvError) -> Self {
        match err {
            tokio::sync::broadcast::error::RecvError::Closed => RecvError::Closed,
            tokio::sync::broadcast::error::RecvError::Lagged(n) => RecvError::Lagged(n),
        }
    }
}
