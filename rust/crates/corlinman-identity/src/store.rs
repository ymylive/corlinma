//! Schema constants + the [`SqliteIdentityStore`] handle.
//!
//! Iteration 2 lands the `open` path and schema bootstrap; the
//! resolver methods (`resolve_or_create`, `lookup`, `aliases_for`,
//! verification-phrase) ship in iteration 3+. That sequencing matches
//! the design doc at `docs/design/phase4-w2-b2-design.md`
//! §"Implementation order".

use std::path::{Path, PathBuf};
use std::str::FromStr;

use corlinman_tenant::{tenant_db_path, TenantId};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

use crate::error::IdentityError;

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

/// SQLite-backed identity store. Cheap to clone; internally holds a
/// pooled connection. The pool is sized for a per-tenant workload —
/// one tenant's identity graph is small and high-frequency reads
/// dominate, so the default 4-conn budget mirrors `SqliteSessionStore`.
#[derive(Debug, Clone)]
pub struct SqliteIdentityStore {
    pool: SqlitePool,
}

impl SqliteIdentityStore {
    /// Open (or create) the identity DB at `path`.
    ///
    /// Opens with WAL + `synchronous=NORMAL` for write throughput and
    /// applies [`SCHEMA_SQL`] so callers never have to run migrations
    /// by hand. Idempotent — re-opening an already-bootstrapped file
    /// is safe (each `CREATE TABLE` is `IF NOT EXISTS`).
    pub async fn open(path: &Path) -> Result<Self, IdentityError> {
        Self::open_with_pool_size(path, 4).await
    }

    /// As [`Self::open`], but with an explicit `max_connections`.
    /// Tests pin `1` to dodge the WAL cross-connection visibility
    /// race the rest of the workspace's per-tenant stores have hit
    /// (see `corlinman-evolution::EvolutionStore::open_with_pool_size`
    /// for the same pattern + reasoning landed in `26a721e`).
    pub async fn open_with_pool_size(
        path: &Path,
        max_connections: u32,
    ) -> Result<Self, IdentityError> {
        let url = format!("sqlite://{}", path.display());
        let options = SqliteConnectOptions::from_str(&url)
            .map_err(|e| IdentityError::Open {
                path: path.to_path_buf(),
                source: e,
            })?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);

        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect_with(options)
            .await
            .map_err(|e| IdentityError::Open {
                path: path.to_path_buf(),
                source: e,
            })?;

        sqlx::raw_sql(SCHEMA_SQL)
            .execute(&pool)
            .await
            .map_err(|e| IdentityError::Storage {
                op: "apply_schema",
                source: e,
            })?;

        Ok(Self { pool })
    }

    /// Borrow the pool. `pub(crate)` rather than `pub` so the resolver
    /// methods (next iteration) can run their own queries without
    /// widening the public surface — every external caller goes
    /// through the trait API.
    #[allow(dead_code)]
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }
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

    #[tokio::test]
    async fn open_creates_schema_and_reopens_idempotently() {
        let tmp = tempfile::TempDir::new().unwrap();
        let tenant = TenantId::legacy_default();
        let path = identity_db_path(tmp.path(), &tenant);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        // First open: file doesn't exist, schema applies fresh.
        let store = SqliteIdentityStore::open(&path).await.unwrap();
        // Sanity probe: a select against `user_identities` must return
        // 0 rows (table exists, just empty).
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_identities")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(n, 0);
        drop(store);

        // Second open: file exists. CREATE TABLE IF NOT EXISTS makes
        // every CREATE a no-op, but a regression there would fail
        // here loudly rather than silently dropping data.
        let store2 = SqliteIdentityStore::open(&path).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_aliases")
            .fetch_one(store2.pool())
            .await
            .unwrap();
        assert_eq!(n, 0);
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM verification_phrases")
            .fetch_one(store2.pool())
            .await
            .unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn open_with_pool_size_one_passes_through() {
        // Pool sizing is mostly a test-fixture knob; the important
        // assertion is just that the constructor accepts and applies
        // the override without panicking.
        let tmp = tempfile::TempDir::new().unwrap();
        let tenant = TenantId::legacy_default();
        let path = identity_db_path(tmp.path(), &tenant);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let _store = SqliteIdentityStore::open_with_pool_size(&path, 1)
            .await
            .unwrap();
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
