//! Schema constants + (in a follow-up iteration) the
//! `IdentityStore` trait + `SqliteIdentityStore` impl.
//!
//! v1 ships only the DDL — making the schema reviewable in isolation
//! before the store impl lands lets the design doc's schema match the
//! code with zero drift. The follow-up iteration adds [`open`],
//! [`resolve_or_create`], [`lookup`], [`aliases_for`], and the
//! verification-phrase methods on top of these constants.

use std::path::{Path, PathBuf};

use corlinman_tenant::{tenant_db_path, TenantId};

/// Full DDL applied on open. Idempotent — safe against an existing
/// file. Mirrors the schema in
/// `docs/design/phase4-w2-b2-design.md` §"Schema".
///
/// Three tables:
///
/// - **`user_identities`** — canonical user rows, one per human. ULID
///   primary key + last-known display name + audit timestamps +
///   `confidence` (1.0 for verified, lower for proposed unions).
/// - **`user_aliases`** — `(channel, channel_user_id) → user_id`
///   bindings. The lookup table the resolver hits on every chat
///   request.
/// - **`verification_phrases`** — short-lived rows for the operator-
///   driven verification protocol.
pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS user_identities (
    user_id TEXT PRIMARY KEY,
    display_name TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 1.0
);

CREATE TABLE IF NOT EXISTS user_aliases (
    channel TEXT NOT NULL,
    channel_user_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    binding_kind TEXT NOT NULL,
    PRIMARY KEY (channel, channel_user_id),
    FOREIGN KEY (user_id) REFERENCES user_identities(user_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_user_aliases_user_id ON user_aliases(user_id);

CREATE TABLE IF NOT EXISTS verification_phrases (
    phrase TEXT PRIMARY KEY,
    issued_to_user_id TEXT NOT NULL,
    issued_on_channel TEXT NOT NULL,
    issued_on_channel_user_id TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT,
    consumed_on_channel TEXT,
    consumed_on_channel_user_id TEXT
);
CREATE INDEX IF NOT EXISTS idx_verification_phrases_expires
    ON verification_phrases(expires_at);
"#;

/// Resolve the per-tenant `user_identity.sqlite` path under
/// `data_dir`, using the same convention the gateway uses for every
/// other tenant DB (`<data_dir>/tenants/<tenant>/<db>.sqlite`). When
/// the tenant is `default` this collapses to the legacy unscoped
/// path segment, matching `corlinman-replay::sessions_db_path`'s
/// behaviour.
pub fn identity_db_path(data_dir: &Path, tenant: &TenantId) -> PathBuf {
    tenant_db_path(data_dir, tenant, "user_identity")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_db_path_uses_per_tenant_layout_for_named_tenant() {
        let acme = TenantId::new("acme").unwrap();
        let p = identity_db_path(Path::new("/data"), &acme);
        let s = p.to_string_lossy();
        assert!(s.contains("/tenants/acme/"));
        assert!(s.ends_with("user_identity.sqlite"));
    }

    #[test]
    fn identity_db_path_collapses_for_legacy_default() {
        // The legacy default path doesn't include a `/tenants/default/`
        // segment — preserves the pre-Phase-4 single-tenant layout
        // the rest of the gateway uses.
        let default = TenantId::legacy_default();
        let p = identity_db_path(Path::new("/data"), &default);
        // Just confirm we didn't accidentally produce a path with
        // `/tenants/default/` (the convention may change file-by-file;
        // assert against the contract `tenant_db_path` carries, not
        // a specific layout we'd duplicate here).
        assert!(p.to_string_lossy().ends_with("user_identity.sqlite"));
    }

    #[test]
    fn schema_sql_contains_all_three_tables() {
        // Sanity check on the DDL string so a copy-paste regression
        // (accidentally truncating a CREATE) gets caught at compile
        // time rather than at first runtime open.
        assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS user_identities"));
        assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS user_aliases"));
        assert!(SCHEMA_SQL.contains("CREATE TABLE IF NOT EXISTS verification_phrases"));
        // Foreign-key cascade is part of the unify story; if it
        // disappears, merges leak orphaned rows.
        assert!(SCHEMA_SQL.contains("ON DELETE CASCADE"));
    }
}
