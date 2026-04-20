//! Data-dir layout + permissions check.
//!
//! Verifies the canonical directory structure exists and that the current
//! process can read + write each subdirectory:
//!
//! ```text
//!   $data_dir/
//!   ├── plugins/
//!   ├── agents/
//!   ├── knowledge/
//!   ├── vector/
//!   └── logs/
//! ```
//!
//! Missing subdirs degrade to `Warn` (hint: `corlinman onboard` creates
//! them); an rw probe failure is a `Fail` — the runtime cannot function
//! without write access to these dirs.

use std::path::{Path, PathBuf};

use async_trait::async_trait;

use super::{DoctorCheck, DoctorContext, DoctorResult};

pub struct PermissionsCheck;

impl PermissionsCheck {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PermissionsCheck {
    fn default() -> Self {
        Self::new()
    }
}

const SUBDIRS: &[&str] = &["plugins", "agents", "knowledge", "vector", "logs"];

fn data_dir(ctx: &DoctorContext) -> PathBuf {
    ctx.config
        .as_ref()
        .map(|c| c.server.data_dir.clone())
        .unwrap_or_else(|| ctx.data_dir.clone())
}

/// Touch a throwaway file to confirm read+write access.
///
/// Uses a unique name so a concurrent `doctor` run doesn't race with itself.
fn probe_rw(dir: &Path) -> Result<(), String> {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let probe = dir.join(format!(".corlinman-doctor-{pid}-{nanos}.tmp"));
    std::fs::write(&probe, b"ok").map_err(|e| format!("write {}: {e}", probe.display()))?;
    std::fs::read(&probe).map_err(|e| format!("read {}: {e}", probe.display()))?;
    std::fs::remove_file(&probe).map_err(|e| format!("rm {}: {e}", probe.display()))?;
    Ok(())
}

#[async_trait]
impl DoctorCheck for PermissionsCheck {
    fn name(&self) -> &str {
        "permissions"
    }

    async fn run(&self, ctx: &DoctorContext) -> DoctorResult {
        let root = data_dir(ctx);
        if !root.exists() {
            return DoctorResult::Warn {
                message: format!("data_dir missing: {}", root.display()),
                hint: Some("run `corlinman onboard` to create the layout".into()),
            };
        }

        let mut missing: Vec<&'static str> = Vec::new();
        let mut failures: Vec<String> = Vec::new();

        // Probe the root first — if rw on root fails, all subdirs will too.
        if let Err(e) = probe_rw(&root) {
            return DoctorResult::Fail {
                message: format!("data_dir rw check failed: {e}"),
                hint: Some("fix filesystem permissions or use a different data_dir".into()),
            };
        }

        for name in SUBDIRS {
            let sub = root.join(name);
            if !sub.exists() {
                missing.push(*name);
                continue;
            }
            if !sub.is_dir() {
                failures.push(format!("{} is not a directory", sub.display()));
                continue;
            }
            if let Err(e) = probe_rw(&sub) {
                failures.push(e);
            }
        }

        if !failures.is_empty() {
            let first = &failures[0];
            let more = if failures.len() > 1 {
                format!(" (+{} more)", failures.len() - 1)
            } else {
                String::new()
            };
            return DoctorResult::Fail {
                message: format!("{first}{more}"),
                hint: Some("ensure the user running corlinman owns the data_dir".into()),
            };
        }

        if !missing.is_empty() {
            return DoctorResult::Warn {
                message: format!(
                    "missing subdir(s): {} under {}",
                    missing.join(", "),
                    root.display()
                ),
                hint: Some("run `corlinman onboard` to create them".into()),
            };
        }

        DoctorResult::Ok {
            message: format!("{} and {} subdir(s) rw ok", root.display(), SUBDIRS.len()),
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
    async fn missing_subdirs_is_warn() {
        let dir = tempdir().unwrap();
        // Root exists (tempdir created it) but no subdirs.
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = PermissionsCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "warn", "got: {:?}", res);
    }

    #[tokio::test]
    async fn full_layout_is_ok() {
        let dir = tempdir().unwrap();
        for name in SUBDIRS {
            std::fs::create_dir_all(dir.path().join(name)).unwrap();
        }
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = PermissionsCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok", "got: {:?}", res);
    }

    #[tokio::test]
    async fn missing_data_dir_is_warn() {
        let dir = tempdir().unwrap();
        // Point at a child that doesn't exist.
        let ctx = ctx_for(dir.path().join("not-there"));
        let res = PermissionsCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "warn", "got: {:?}", res);
    }
}
