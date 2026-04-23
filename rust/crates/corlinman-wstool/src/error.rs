//! Crate-local errors.
//!
//! [`WsToolError`] is what falls out of [`crate::server::WsToolServer`] and
//! [`crate::runner::WsToolRunner`] at the edges. Internally, most paths
//! use `anyhow::Error` because the handler boundary surfaces a plain
//! string back to the gateway caller.
//!
//! [`ToolError`] is the lightweight error a [`ToolHandler`] may return;
//! it carries a stable `code` string so callers can branch on it without
//! string-matching messages.

use thiserror::Error;

/// Returned by [`ToolHandler::invoke`](crate::runner::ToolHandler::invoke).
#[derive(Debug, Clone, Error)]
#[error("tool error [{code}]: {message}")]
pub struct ToolError {
    pub code: String,
    pub message: String,
}

impl ToolError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    /// Handlers spawn this when the gateway cancelled their call.
    pub fn cancelled() -> Self {
        Self::new("cancelled", "handler observed cancellation")
    }
}

/// Higher-level errors used at the crate boundary.
#[derive(Debug, Error)]
pub enum WsToolError {
    #[error("auth rejected: {0}")]
    Auth(String),

    #[error("unsupported tool: {0}")]
    Unsupported(String),

    #[error("runner disconnected")]
    Disconnected,

    #[error("timed out after {millis}ms")]
    Timeout { millis: u64 },

    #[error("tool {code}: {message}")]
    ToolFailed { code: String, message: String },

    #[error("protocol: {0}")]
    Protocol(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("internal: {0}")]
    Internal(String),
}
