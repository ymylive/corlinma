//! Count un-decided rows in `pending_approvals`; warn when the queue is
//! unexpectedly large. A backlog of hundreds usually means an operator
//! lost track of `/admin/approvals` — the backlog itself is not a fault
//! but warrants visibility from `doctor`.

use std::path::PathBuf;

use async_trait::async_trait;
use sqlx::Row;

use super::{DoctorCheck, DoctorContext, DoctorResult};

/// Threshold above which we flip from Ok → Warn. Kept generous; an
/// approval-heavy deployment may legitimately sit at 10s of pending.
const WARN_PENDING: i64 = 100;

pub struct PendingApprovalsOverflowCheck;

impl PendingApprovalsOverflowCheck {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PendingApprovalsOverflowCheck {
    fn default() -> Self {
        Self::new()
    }
}

fn kb_path(ctx: &DoctorContext) -> PathBuf {
    ctx.data_dir.join("kb.sqlite")
}

async fn count_pending(path: &std::path::Path) -> Result<i64, String> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_millis(500))
        .connect(&format!("sqlite://{}", path.display()))
        .await
        .map_err(|e| format!("open: {e}"))?;
    let row = sqlx::query("SELECT count(*) as n FROM pending_approvals WHERE decided_at IS NULL")
        .fetch_one(&pool)
        .await;
    pool.close().await;
    match row {
        Ok(r) => r.try_get::<i64, _>("n").map_err(|e| format!("decode: {e}")),
        // Missing table = schema hasn't migrated yet; treat as zero.
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("no such table") {
                Ok(0)
            } else {
                Err(format!("query: {e}"))
            }
        }
    }
}

#[async_trait]
impl DoctorCheck for PendingApprovalsOverflowCheck {
    fn name(&self) -> &str {
        "pending_approvals_overflow"
    }

    async fn run(&self, ctx: &DoctorContext) -> DoctorResult {
        let path = kb_path(ctx);
        if !path.exists() {
            return DoctorResult::Ok {
                message: format!("no kb.sqlite at {}", path.display()),
            };
        }
        match count_pending(&path).await {
            Ok(n) if n >= WARN_PENDING => DoctorResult::Warn {
                message: format!("{n} pending approvals queued"),
                hint: Some("triage via /admin/approvals".into()),
            },
            Ok(n) => DoctorResult::Ok {
                message: format!("{n} pending approvals"),
            },
            Err(e) => DoctorResult::Warn {
                message: format!("approvals probe failed: {e}"),
                hint: Some("check kb.sqlite schema and permissions".into()),
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
    async fn missing_kb_is_ok() {
        let dir = tempdir().unwrap();
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = PendingApprovalsOverflowCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok", "got: {res:?}");
    }

    #[tokio::test]
    async fn empty_kb_reports_zero() {
        // A blank sqlite file should report zero (missing table branch).
        let dir = tempdir().unwrap();
        let path = dir.path().join("kb.sqlite");
        // Touch an empty file so sqlx opens it.
        std::fs::write(&path, b"").unwrap();
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = PendingApprovalsOverflowCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok", "got: {res:?}");
    }
}
