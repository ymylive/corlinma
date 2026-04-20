//! `DoctorCheck` trait + module registry.
//!
//! Each concrete check lives in its own module and implements [`DoctorCheck`].
//! The runner ([`super::run`]) instantiates all checks via [`all`] and
//! executes them sequentially against a shared [`DoctorContext`].

use std::path::PathBuf;

use async_trait::async_trait;
use serde::Serialize;

pub mod agent_grpc_ping;
pub mod broken_symlinks;
pub mod channels;
pub mod config;
pub mod disk_space;
pub mod docker_daemon;
pub mod log_rotation;
pub mod manifest;
pub mod manifest_duplicates;
pub mod memory_usage;
pub mod pending_approvals_overflow;
pub mod permissions;
pub mod provider_https_smoke;
pub mod python_subprocess_health;
pub mod scheduler;
pub mod scheduler_next_triggers;
pub mod sqlite;
pub mod startup_time;
pub mod upstream;
pub mod usearch;

/// Shared, read-only context handed to every check. Built once by the runner
/// so individual checks don't re-resolve the data dir or re-parse config.
pub struct DoctorContext {
    /// Effective data dir (`CORLINMAN_DATA_DIR` env override, else `~/.corlinman`).
    pub data_dir: PathBuf,
    /// Parsed config if `data_dir/config.toml` exists and decoded cleanly.
    /// `None` when the file is missing or failed to decode — the config check
    /// is the source of truth for *why*; other checks just degrade gracefully.
    pub config: Option<corlinman_core::config::Config>,
    /// Path actually loaded (for display in check messages).
    pub config_path: PathBuf,
}

/// Per-check outcome. `Ok` / `Warn` / `Fail` map directly to the three output
/// glyphs (✓ / ! / ✗) and to the `status` field in `--json` mode.
#[derive(Debug, Clone)]
pub enum DoctorResult {
    Ok {
        message: String,
    },
    Warn {
        message: String,
        hint: Option<String>,
    },
    Fail {
        message: String,
        hint: Option<String>,
    },
}

impl DoctorResult {
    pub fn status_str(&self) -> &'static str {
        match self {
            Self::Ok { .. } => "ok",
            Self::Warn { .. } => "warn",
            Self::Fail { .. } => "fail",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Ok { message } | Self::Warn { message, .. } | Self::Fail { message, .. } => {
                message
            }
        }
    }

    pub fn hint(&self) -> Option<&str> {
        match self {
            Self::Ok { .. } => None,
            Self::Warn { hint, .. } | Self::Fail { hint, .. } => hint.as_deref(),
        }
    }
}

/// JSON-serialisable projection of a single check run.
#[derive(Debug, Clone, Serialize)]
pub struct CheckReport {
    pub name: String,
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl CheckReport {
    pub fn new(name: &str, result: &DoctorResult) -> Self {
        Self {
            name: name.to_string(),
            status: result.status_str().to_string(),
            message: result.message().to_string(),
            hint: result.hint().map(str::to_string),
        }
    }
}

/// A diagnostic module. Kept `async` so future checks (upstream ping, SQLite
/// health) can do real I/O without changing the trait.
#[async_trait]
pub trait DoctorCheck: Send + Sync {
    /// Short machine-readable name, also used for `--module <name>` filtering.
    fn name(&self) -> &str;

    async fn run(&self, ctx: &DoctorContext) -> DoctorResult;
}

/// Canonical list of checks. Order is the display order in human output.
pub fn all() -> Vec<Box<dyn DoctorCheck>> {
    vec![
        Box::new(config::ConfigCheck::new()),
        Box::new(manifest::ManifestCheck::new()),
        Box::new(manifest_duplicates::ManifestDuplicatesCheck::new()),
        Box::new(upstream::UpstreamCheck::new()),
        Box::new(provider_https_smoke::ProviderHttpsSmokeCheck::new()),
        Box::new(sqlite::SqliteCheck::new()),
        Box::new(usearch::UsearchCheck::new()),
        Box::new(channels::ChannelsCheck::new()),
        Box::new(scheduler::SchedulerCheck::new()),
        Box::new(scheduler_next_triggers::SchedulerNextTriggersCheck::new()),
        Box::new(permissions::PermissionsCheck::new()),
        Box::new(agent_grpc_ping::AgentGrpcPingCheck::new()),
        Box::new(disk_space::DiskSpaceCheck::new()),
        Box::new(log_rotation::LogRotationCheck::new()),
        Box::new(docker_daemon::DockerDaemonCheck::new()),
        Box::new(python_subprocess_health::PythonSubprocessHealthCheck::new()),
        Box::new(memory_usage::MemoryUsageCheck::new()),
        Box::new(startup_time::StartupTimeCheck::new()),
        Box::new(pending_approvals_overflow::PendingApprovalsOverflowCheck::new()),
        Box::new(broken_symlinks::BrokenSymlinksCheck::new()),
    ]
}
