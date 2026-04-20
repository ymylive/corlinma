//! Warn when the data_dir filesystem has less than a comfortable amount of
//! free space.
//!
//! We use libc's `statvfs` on unix and skip the check on other platforms.
//! Thresholds: <200 MB → `Fail`; <2 GB → `Warn`; otherwise `Ok`.

use async_trait::async_trait;

use super::{DoctorCheck, DoctorContext, DoctorResult};

pub struct DiskSpaceCheck;

impl DiskSpaceCheck {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DiskSpaceCheck {
    fn default() -> Self {
        Self::new()
    }
}

/// Approx free bytes on the filesystem containing `path`. Returns `None` on
/// platforms or paths where we can't inspect.
#[cfg(unix)]
fn free_bytes(path: &std::path::Path) -> Option<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `statvfs` is safe to call on a nul-terminated path; we
    // initialise the output struct to zeros.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: c-string lifetime outlives the call.
    let rc = unsafe { libc::statvfs(c.as_ptr(), &mut stat) };
    if rc != 0 {
        return None;
    }
    let bsize = stat.f_frsize as u64;
    let bavail = stat.f_bavail as u64;
    Some(bsize.saturating_mul(bavail))
}

#[cfg(not(unix))]
fn free_bytes(_path: &std::path::Path) -> Option<u64> {
    None
}

#[async_trait]
impl DoctorCheck for DiskSpaceCheck {
    fn name(&self) -> &str {
        "disk_space"
    }

    async fn run(&self, ctx: &DoctorContext) -> DoctorResult {
        let path = if ctx.data_dir.exists() {
            ctx.data_dir.clone()
        } else {
            std::env::temp_dir()
        };
        let Some(bytes) = free_bytes(&path) else {
            return DoctorResult::Ok {
                message: "free-space check unsupported on this platform".into(),
            };
        };
        let mb = bytes / (1024 * 1024);
        if bytes < 200 * 1024 * 1024 {
            DoctorResult::Fail {
                message: format!("only {mb} MiB free under {}", path.display()),
                hint: Some("free disk space before running corlinman".into()),
            }
        } else if bytes < 2 * 1024 * 1024 * 1024 {
            DoctorResult::Warn {
                message: format!("{mb} MiB free under {}", path.display()),
                hint: Some("consider freeing disk space; logs/kb may grow".into()),
            }
        } else {
            DoctorResult::Ok {
                message: format!("{mb} MiB free under {}", path.display()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reports_a_result_on_tempdir() {
        let ctx = DoctorContext {
            data_dir: std::env::temp_dir(),
            config_path: std::env::temp_dir().join("config.toml"),
            config: None,
        };
        let res = DiskSpaceCheck::new().run(&ctx).await;
        // Tempdir on CI always has gigabytes; we just assert it didn't panic
        // and produced a concrete status string.
        assert!(matches!(res.status_str(), "ok" | "warn" | "fail"));
    }

    #[cfg(unix)]
    #[test]
    fn free_bytes_positive_on_tempdir() {
        let bytes = free_bytes(&std::env::temp_dir()).expect("statvfs ok");
        assert!(bytes > 0);
    }
}
