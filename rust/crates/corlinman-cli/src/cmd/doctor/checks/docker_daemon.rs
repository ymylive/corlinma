//! Lightweight Docker-daemon reachability check — just the socket presence
//! on unix (`/var/run/docker.sock`) or the `DOCKER_HOST` env var. The
//! heavier bollard `ping()` smoke test is handled by the sandbox runner at
//! plugin spawn time; this check is fast + offline-safe.
//!
//! Semantics:
//!   - No sandboxed plugin configured → `Ok`, skipped.
//!   - `DOCKER_HOST` / socket present → `Ok`.
//!   - Otherwise → `Warn` (sandboxed plugins will fail at spawn time).

use std::path::Path;

use async_trait::async_trait;
use corlinman_plugins::{discover, Origin, SearchRoot};

use super::{DoctorCheck, DoctorContext, DoctorResult};

pub struct DockerDaemonCheck;

impl DockerDaemonCheck {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DockerDaemonCheck {
    fn default() -> Self {
        Self::new()
    }
}

fn needs_docker(ctx: &DoctorContext) -> bool {
    let plugins_dir = ctx.data_dir.join("plugins");
    if !plugins_dir.exists() {
        return false;
    }
    let roots = vec![SearchRoot::new(&plugins_dir, Origin::Global)];
    let (entries, _) = discover(&roots);
    entries
        .iter()
        .any(|e| corlinman_plugins::sandbox::is_enabled(&e.manifest.sandbox))
}

#[async_trait]
impl DoctorCheck for DockerDaemonCheck {
    fn name(&self) -> &str {
        "docker_daemon"
    }

    async fn run(&self, ctx: &DoctorContext) -> DoctorResult {
        if !needs_docker(ctx) {
            return DoctorResult::Ok {
                message: "no sandboxed plugins; docker not required".into(),
            };
        }
        if std::env::var_os("DOCKER_HOST").is_some() {
            return DoctorResult::Ok {
                message: "DOCKER_HOST is set".into(),
            };
        }
        let sock = Path::new("/var/run/docker.sock");
        if sock.exists() {
            DoctorResult::Ok {
                message: format!("{} present", sock.display()),
            }
        } else {
            DoctorResult::Warn {
                message: format!(
                    "no docker socket at {} and DOCKER_HOST unset",
                    sock.display()
                ),
                hint: Some(
                    "start Docker Desktop / dockerd before running sandboxed plugins".into(),
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn ctx_for(data_dir: std::path::PathBuf) -> DoctorContext {
        DoctorContext {
            config_path: data_dir.join("config.toml"),
            data_dir,
            config: None,
        }
    }

    #[tokio::test]
    async fn no_plugins_is_ok() {
        let dir = tempdir().unwrap();
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = DockerDaemonCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok", "got: {res:?}");
    }
}
