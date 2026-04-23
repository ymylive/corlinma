//! Crate-local errors for the NodeBridge stub server.

use thiserror::Error;

/// Errors surfaced at the crate boundary. Internal paths that talk to
/// the reader/writer halves of a socket tend to collapse into `Io` or
/// `Protocol`; dispatch failures into `NoCapableNode` / `Timeout`.
#[derive(Debug, Error)]
pub enum NodeBridgeError {
    /// The supplied `config.listen` failed `SocketAddr::parse`.
    #[error("invalid listen address: {0}")]
    InvalidListenAddr(String),

    /// Binding the TCP listener failed.
    #[error("bind: {0}")]
    Bind(#[from] std::io::Error),

    /// A frame arrived in a position it isn't allowed to (e.g. something
    /// other than `Register` as the first frame).
    #[error("protocol: {0}")]
    Protocol(String),

    /// `DispatchJob` asked for a `kind` no registered node advertises.
    #[error("no capable node for kind: {0}")]
    NoCapableNode(String),

    /// A dispatched job didn't receive a `JobResult` in time.
    #[error("dispatch timed out after {millis}ms")]
    Timeout { millis: u64 },

    /// Registration was refused with a coded reason.
    #[error("register rejected [{code}]: {message}")]
    RegisterRejected { code: String, message: String },
}
