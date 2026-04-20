//! Report last gateway startup time if the gateway wrote a marker file.
//!
//! Contract: the gateway atomically writes `<data_dir>/logs/.startup_ms`
//! with its boot-to-serving duration in milliseconds. This check reads it
//! and warns when the value crosses a budget (10 s). A missing marker is
//! `Ok` — fresh installs haven't recorded one yet.

use std::path::PathBuf;

use async_trait::async_trait;

use super::{DoctorCheck, DoctorContext, DoctorResult};

pub struct StartupTimeCheck;

impl StartupTimeCheck {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StartupTimeCheck {
    fn default() -> Self {
        Self::new()
    }
}

fn marker_path(ctx: &DoctorContext) -> PathBuf {
    ctx.data_dir.join("logs").join(".startup_ms")
}

#[async_trait]
impl DoctorCheck for StartupTimeCheck {
    fn name(&self) -> &str {
        "startup_time"
    }

    async fn run(&self, ctx: &DoctorContext) -> DoctorResult {
        let path = marker_path(ctx);
        if !path.exists() {
            return DoctorResult::Ok {
                message: format!("no startup marker at {}", path.display()),
            };
        }
        match std::fs::read_to_string(&path) {
            Ok(s) => match s.trim().parse::<u64>() {
                Ok(ms) if ms > 10_000 => DoctorResult::Warn {
                    message: format!("last startup took {ms} ms (>10 s)"),
                    hint: Some("investigate slow plugins / embeddings load".into()),
                },
                Ok(ms) => DoctorResult::Ok {
                    message: format!("last startup {ms} ms"),
                },
                Err(e) => DoctorResult::Warn {
                    message: format!("{} unparseable: {e}", path.display()),
                    hint: Some("delete the file; the gateway rewrites it on next boot".into()),
                },
            },
            Err(e) => DoctorResult::Warn {
                message: format!("read {} failed: {e}", path.display()),
                hint: None,
            },
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
    async fn missing_marker_is_ok() {
        let dir = tempdir().unwrap();
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = StartupTimeCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok", "got: {res:?}");
    }

    #[tokio::test]
    async fn fast_startup_is_ok() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("logs")).unwrap();
        fs::write(dir.path().join("logs/.startup_ms"), "500").unwrap();
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = StartupTimeCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok", "got: {res:?}");
    }

    #[tokio::test]
    async fn slow_startup_is_warn() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("logs")).unwrap();
        fs::write(dir.path().join("logs/.startup_ms"), "45000").unwrap();
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = StartupTimeCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "warn", "got: {res:?}");
    }
}
