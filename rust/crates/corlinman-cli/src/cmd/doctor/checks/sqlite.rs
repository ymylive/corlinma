//! SQLite knowledge-base check.
//!
//! Looks for `data_dir/vector/chunks.sqlite` (the path `corlinman-vector`
//! uses) and verifies:
//!   * the file opens cleanly via [`corlinman_vector::SqliteStore`]
//!   * FTS5 is available in the linked SQLite build (an `fts5` MATCH must
//!     parse, even against an empty virtual table)
//!
//! Missing file → `Warn`, open/parse failure → `Fail`.

use std::path::PathBuf;

use async_trait::async_trait;
use corlinman_vector::SqliteStore;

use super::{DoctorCheck, DoctorContext, DoctorResult};

pub struct SqliteCheck;

impl SqliteCheck {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SqliteCheck {
    fn default() -> Self {
        Self::new()
    }
}

fn sqlite_path(ctx: &DoctorContext) -> PathBuf {
    // Prefer explicit config.server.data_dir; fall back to ctx.data_dir.
    let base = ctx
        .config
        .as_ref()
        .map(|c| c.server.data_dir.clone())
        .unwrap_or_else(|| ctx.data_dir.clone());
    base.join("vector").join("chunks.sqlite")
}

#[async_trait]
impl DoctorCheck for SqliteCheck {
    fn name(&self) -> &str {
        "sqlite"
    }

    async fn run(&self, ctx: &DoctorContext) -> DoctorResult {
        let path = sqlite_path(ctx);
        if !path.exists() {
            return DoctorResult::Warn {
                message: format!("no sqlite db at {}", path.display()),
                hint: Some(
                    "run `corlinman vector rebuild` or let the indexer create it on first use"
                        .into(),
                ),
            };
        }

        let store = match SqliteStore::open(&path).await {
            Ok(s) => s,
            Err(e) => {
                return DoctorResult::Fail {
                    message: format!("cannot open {}: {e}", path.display()),
                    hint: Some("ensure the file isn't locked or corrupt".into()),
                }
            }
        };

        // FTS5 smoke test: a MATCH against the `chunks_fts` virtual table
        // fails fast if SQLite was built without FTS5. Empty result is fine.
        let fts_ok = sqlx::query("SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH 'x' LIMIT 1")
            .fetch_optional(store.pool())
            .await;
        match fts_ok {
            Ok(_) => DoctorResult::Ok {
                message: format!("{} opens; FTS5 available", path.display()),
            },
            Err(e) => DoctorResult::Fail {
                message: format!("FTS5 unavailable: {e}"),
                hint: Some("rebuild corlinman against a SQLite with FTS5 enabled".into()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn ctx_for(data_dir: PathBuf) -> DoctorContext {
        DoctorContext {
            config_path: data_dir.join("config.toml"),
            data_dir,
            config: None,
        }
    }

    #[tokio::test]
    async fn missing_sqlite_is_warn() {
        let dir = tempdir().unwrap();
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = SqliteCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "warn", "got: {:?}", res);
    }

    #[tokio::test]
    async fn fresh_sqlite_is_ok() {
        let dir = tempdir().unwrap();
        let vector_dir = dir.path().join("vector");
        std::fs::create_dir_all(&vector_dir).unwrap();
        // Use SqliteStore to create the file with the correct schema.
        let path = vector_dir.join("chunks.sqlite");
        let _ = SqliteStore::open(&path).await.expect("create sqlite");
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = SqliteCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok", "got: {:?}", res);
    }
}
