//! Thin SQLite wrapper. Opens (or creates) the evolution DB and applies
//! `SCHEMA_SQL` idempotently. Phase 2 default path is `/data/evolution.sqlite`
//! — separate from `kb.sqlite` so RAG churn doesn't touch the audit trail.

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};

use crate::schema::SCHEMA_SQL;

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("invalid sqlite url '{0}': {1}")]
    InvalidUrl(String, sqlx::Error),
    #[error("connect '{0}': {1}")]
    Connect(String, sqlx::Error),
    #[error("apply SCHEMA_SQL: {0}")]
    ApplySchema(sqlx::Error),
}

#[derive(Debug, Clone)]
pub struct EvolutionStore {
    pool: SqlitePool,
}

impl EvolutionStore {
    /// Open (or create) the evolution SQLite at `path`. WAL +
    /// `synchronous=NORMAL` + `foreign_keys=ON`. Applies `SCHEMA_SQL`
    /// once — `CREATE … IF NOT EXISTS` makes this safe to repeat.
    pub async fn open(path: &Path) -> Result<Self, OpenError> {
        let url = format!("sqlite://{}", path.display());

        let options = SqliteConnectOptions::from_str(&url)
            .map_err(|e| OpenError::InvalidUrl(url.clone(), e))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await
            .map_err(|e| OpenError::Connect(url, e))?;

        sqlx::raw_sql(SCHEMA_SQL)
            .execute(&pool)
            .await
            .map_err(OpenError::ApplySchema)?;

        Ok(Self { pool })
    }

    /// Underlying pool. Repos take this by reference rather than owning it
    /// so multiple repos share the same connection budget.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn open_creates_tables() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("evolution.sqlite");
        let store = EvolutionStore::open(&path).await.unwrap();

        // Round-trip: count rows from each table — should be 0 but the
        // query must succeed (== schema applied).
        for table in [
            "evolution_signals",
            "evolution_proposals",
            "evolution_history",
        ] {
            let row: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(store.pool())
                .await
                .expect("table exists");
            assert_eq!(row.0, 0, "{table} starts empty");
        }
    }

    #[tokio::test]
    async fn open_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("evolution.sqlite");
        let _first = EvolutionStore::open(&path).await.unwrap();
        // Re-opening must not error (CREATE … IF NOT EXISTS).
        let _second = EvolutionStore::open(&path).await.unwrap();
    }
}
