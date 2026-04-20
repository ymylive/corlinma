//! Confirm that a usable python interpreter is reachable on `$PATH` (or
//! `$PYTHON`). Spawns `python3 --version` with a 2 s timeout. Missing
//! interpreter is a `Warn` — the gateway can talk to a pre-running server
//! via TCP without spawning python locally.

use std::time::Duration;

use async_trait::async_trait;
use tokio::process::Command;
use tokio::time::timeout;

use super::{DoctorCheck, DoctorContext, DoctorResult};

pub struct PythonSubprocessHealthCheck;

impl PythonSubprocessHealthCheck {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PythonSubprocessHealthCheck {
    fn default() -> Self {
        Self::new()
    }
}

fn python_cmd() -> String {
    std::env::var("PYTHON")
        .ok()
        .unwrap_or_else(|| "python3".to_string())
}

#[async_trait]
impl DoctorCheck for PythonSubprocessHealthCheck {
    fn name(&self) -> &str {
        "python_subprocess_health"
    }

    async fn run(&self, _ctx: &DoctorContext) -> DoctorResult {
        let cmd = python_cmd();
        let fut = Command::new(&cmd).arg("--version").output();
        match timeout(Duration::from_secs(2), fut).await {
            Ok(Ok(out)) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                let version = if stdout.is_empty() { stderr } else { stdout };
                DoctorResult::Ok {
                    message: format!("{cmd}: {version}"),
                }
            }
            Ok(Ok(out)) => DoctorResult::Warn {
                message: format!("{cmd} exited {}", out.status),
                hint: Some("check that python is installed and working".into()),
            },
            Ok(Err(e)) => DoctorResult::Warn {
                message: format!("{cmd}: {e}"),
                hint: Some("install python 3.12+ or set $PYTHON".into()),
            },
            Err(_) => DoctorResult::Warn {
                message: format!("`{cmd} --version` timed out (2s)"),
                hint: Some("python startup unusually slow; check the installation".into()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> DoctorContext {
        DoctorContext {
            data_dir: std::path::PathBuf::from("/tmp"),
            config_path: std::path::PathBuf::from("/tmp/config.toml"),
            config: None,
        }
    }

    #[tokio::test]
    async fn returns_a_result() {
        let res = PythonSubprocessHealthCheck::new().run(&ctx()).await;
        // CI has python3; locally maybe not — both are acceptable.
        assert!(matches!(res.status_str(), "ok" | "warn"));
    }
}
