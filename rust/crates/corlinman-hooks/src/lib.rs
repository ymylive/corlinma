//! `corlinman-hooks` — cross-cutting event bus for the corlinman platform.
//!
//! Design:
//!   - Three priority tiers (`Critical` < `Normal` < `Low`). `emit` fans out
//!     in that order and awaits each tier before moving on so Critical
//!     subscribers always observe an event before Normal/Low do.
//!   - Each tier is a `tokio::sync::broadcast` channel. Dropped subscribers
//!     are transparent; slow subscribers see `Lagged` and skip forward.
//!   - Every `emit` call opens a `tracing` span tagged with the event kind
//!     and (if present) session key, making cross-component traces easy to
//!     correlate.
//!   - `CancelToken` is a cooperative flag: emitters check it and bail
//!     without publishing, so downstream listeners stop seeing new events
//!     once a shutdown/abort is signalled upstream.

mod bus;
mod error;
mod event;
mod priority;

pub use bus::{HookBus, HookSubscription};
pub use error::{HookError, RecvError};
pub use event::HookEvent;
pub use priority::{CancelToken, HookPriority};
