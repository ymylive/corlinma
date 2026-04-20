//! Report resident-set size of the current doctor process. This is a sanity
//! check: if simply running `corlinman doctor` exceeds a sane budget, the
//! full gateway will too. Thresholds: >1 GiB RSS → `Warn`.

use async_trait::async_trait;

use super::{DoctorCheck, DoctorContext, DoctorResult};

pub struct MemoryUsageCheck;

impl MemoryUsageCheck {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MemoryUsageCheck {
    fn default() -> Self {
        Self::new()
    }
}

/// RSS in bytes for the current process. `None` on unsupported platforms.
#[cfg(target_os = "linux")]
fn rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn rss_bytes() -> Option<u64> {
    // `ps -o rss= -p $$` returns KB. Shell out because the direct Mach API
    // lives in a C header that rust-libc exposes only partially.
    use std::process::Command;
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let kb: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(kb * 1024)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn rss_bytes() -> Option<u64> {
    None
}

#[async_trait]
impl DoctorCheck for MemoryUsageCheck {
    fn name(&self) -> &str {
        "memory_usage"
    }

    async fn run(&self, _ctx: &DoctorContext) -> DoctorResult {
        let Some(bytes) = rss_bytes() else {
            return DoctorResult::Ok {
                message: "rss probe unsupported on this platform".into(),
            };
        };
        let mib = bytes / (1024 * 1024);
        if bytes >= 1024 * 1024 * 1024 {
            DoctorResult::Warn {
                message: format!("doctor RSS {mib} MiB (>1 GiB is unexpected)"),
                hint: Some("file an issue if this persists".into()),
            }
        } else {
            DoctorResult::Ok {
                message: format!("doctor RSS {mib} MiB"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_a_result() {
        let ctx = DoctorContext {
            data_dir: std::path::PathBuf::from("/tmp"),
            config_path: std::path::PathBuf::from("/tmp/config.toml"),
            config: None,
        };
        let res = MemoryUsageCheck::new().run(&ctx).await;
        assert!(matches!(res.status_str(), "ok" | "warn"));
    }
}
