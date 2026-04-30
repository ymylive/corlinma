//! Errors emitted by the identity store, resolver, and verification
//! protocol. Variants are the canonical surface admin routes and the
//! gateway middleware translate into HTTP envelopes.

use std::path::PathBuf;

/// Identity-layer error. Variants split into three groups:
///
/// 1. **Storage** — schema bootstrap or SQL execution failed.
/// 2. **Resolver** — input was structurally invalid (empty channel
///    name, etc) or referenced a missing entity.
/// 3. **Verification** — phrase exchange protocol violations
///    (expired / consumed / unknown).
#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    /// Schema bootstrap or SQL execution failed.
    #[error("identity store {op}: {source}")]
    Storage {
        op: &'static str,
        #[source]
        source: sqlx::Error,
    },

    /// Path-level open failure. Distinct from `Storage` because the
    /// remediation differs (filesystem permissions vs DB corruption).
    #[error("identity store open failed at {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: sqlx::Error,
    },

    /// Caller passed an empty `channel`, `channel_user_id`, or other
    /// structurally invalid input.
    #[error("invalid input: {0}")]
    InvalidInput(&'static str),

    /// Lookup target doesn't exist (admin merge, alias_for, etc.).
    #[error("user_id not found: {0:?}")]
    UserNotFound(String),

    /// Verification phrase doesn't match any active row.
    #[error("verification phrase unknown")]
    PhraseUnknown,

    /// Phrase exists but is past its `expires_at`.
    #[error("verification phrase expired")]
    PhraseExpired,

    /// Phrase was already redeemed.
    #[error("verification phrase already consumed")]
    PhraseAlreadyConsumed,
}
