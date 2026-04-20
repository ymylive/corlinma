//! Walk the data dir looking for symlinks pointing at non-existent paths.
//! A broken symlink under `plugins/` or `knowledge/` means an uninstalled
//! package left a dangling reference.

use std::path::PathBuf;

use async_trait::async_trait;

use super::{DoctorCheck, DoctorContext, DoctorResult};

pub struct BrokenSymlinksCheck;

impl BrokenSymlinksCheck {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BrokenSymlinksCheck {
    fn default() -> Self {
        Self::new()
    }
}

/// Recursive, symlink-aware scan. Uses `walkdir` which is already a
/// workspace dep. Depth-cap 4 to keep this check cheap.
fn find_broken(root: &std::path::Path) -> Vec<PathBuf> {
    use walkdir::WalkDir;
    let mut broken: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .max_depth(4)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let p = entry.path();
        if let Ok(meta) = std::fs::symlink_metadata(p) {
            if meta.file_type().is_symlink() && std::fs::metadata(p).is_err() {
                broken.push(p.to_path_buf());
            }
        }
    }
    broken
}

#[async_trait]
impl DoctorCheck for BrokenSymlinksCheck {
    fn name(&self) -> &str {
        "broken_symlinks"
    }

    async fn run(&self, ctx: &DoctorContext) -> DoctorResult {
        if !ctx.data_dir.exists() {
            return DoctorResult::Ok {
                message: "data_dir missing; nothing to scan".into(),
            };
        }
        let broken = find_broken(&ctx.data_dir);
        if broken.is_empty() {
            DoctorResult::Ok {
                message: format!("no broken symlinks under {}", ctx.data_dir.display()),
            }
        } else {
            let first = &broken[0];
            let more = if broken.len() > 1 {
                format!(" (+{} more)", broken.len() - 1)
            } else {
                String::new()
            };
            DoctorResult::Warn {
                message: format!("broken symlink: {}{}", first.display(), more),
                hint: Some("remove the dangling link or re-install the target".into()),
            }
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
    async fn no_symlinks_is_ok() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("file.txt"), b"ok").unwrap();
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = BrokenSymlinksCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok", "got: {res:?}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dangling_symlink_is_warn() {
        let dir = tempdir().unwrap();
        let link = dir.path().join("dangling");
        std::os::unix::fs::symlink(dir.path().join("not-there"), &link).unwrap();
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = BrokenSymlinksCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "warn", "got: {res:?}");
    }
}
