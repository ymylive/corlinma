//! Warn when any single log file under `<data_dir>/logs/` is dangerously
//! large — a sign that rotation isn't happening (e.g. the tracing-appender
//! rolling daemon wasn't started, or the user disabled it).
//!
//! Thresholds: any file >= 512 MiB → `Warn`. The `Fail` path is intentionally
//! omitted; large logs never break runtime, only waste disk.

use std::path::PathBuf;

use async_trait::async_trait;

use super::{DoctorCheck, DoctorContext, DoctorResult};

const WARN_BYTES: u64 = 512 * 1024 * 1024;

pub struct LogRotationCheck;

impl LogRotationCheck {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LogRotationCheck {
    fn default() -> Self {
        Self::new()
    }
}

fn logs_dir(ctx: &DoctorContext) -> PathBuf {
    ctx.data_dir.join("logs")
}

fn scan(dir: &std::path::Path) -> (u64, Option<(PathBuf, u64)>) {
    let mut total: u64 = 0;
    let mut biggest: Option<(PathBuf, u64)> = None;
    let Ok(read) = std::fs::read_dir(dir) else {
        return (0, None);
    };
    for entry in read.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let size = meta.len();
        total = total.saturating_add(size);
        if biggest.as_ref().map(|(_, s)| size > *s).unwrap_or(true) {
            biggest = Some((entry.path(), size));
        }
    }
    (total, biggest)
}

#[async_trait]
impl DoctorCheck for LogRotationCheck {
    fn name(&self) -> &str {
        "log_rotation"
    }

    async fn run(&self, ctx: &DoctorContext) -> DoctorResult {
        let dir = logs_dir(ctx);
        if !dir.exists() {
            return DoctorResult::Ok {
                message: format!("no logs dir at {}", dir.display()),
            };
        }
        let (total, biggest) = scan(&dir);
        let Some((path, size)) = biggest else {
            return DoctorResult::Ok {
                message: format!("empty logs dir at {}", dir.display()),
            };
        };
        if size >= WARN_BYTES {
            DoctorResult::Warn {
                message: format!(
                    "{} is {} MiB (threshold {} MiB)",
                    path.display(),
                    size / (1024 * 1024),
                    WARN_BYTES / (1024 * 1024),
                ),
                hint: Some("ensure log rotation is configured, or delete the file".into()),
            }
        } else {
            DoctorResult::Ok {
                message: format!(
                    "largest log {} MiB; total {} MiB",
                    size / (1024 * 1024),
                    total / (1024 * 1024),
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn ctx_for(data_dir: PathBuf) -> DoctorContext {
        DoctorContext {
            config_path: data_dir.join("config.toml"),
            data_dir,
            config: None,
        }
    }

    #[tokio::test]
    async fn missing_logs_dir_is_ok() {
        let dir = tempdir().unwrap();
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = LogRotationCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok", "got: {res:?}");
    }

    #[tokio::test]
    async fn small_logs_is_ok() {
        let dir = tempdir().unwrap();
        let logs = dir.path().join("logs");
        fs::create_dir_all(&logs).unwrap();
        fs::write(logs.join("a.log"), b"hello world").unwrap();
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = LogRotationCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok", "got: {res:?}");
    }
}
