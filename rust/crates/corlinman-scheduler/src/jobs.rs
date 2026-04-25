//! Job-side helpers: turning a `corlinman_core::config::SchedulerJob` into
//! a `JobSpec` the runtime tick loop can dispatch on.
//!
//! Kept small on purpose; the heavy lifting (cron tick, subprocess) lives
//! in `runtime.rs` / `subprocess.rs`. Putting the conversion here makes
//! the runtime module easier to read and lets tests build `JobSpec`
//! values without going through the full `Config` path.

use std::collections::BTreeMap;
use std::path::PathBuf;

use corlinman_core::config::{JobAction, SchedulerJob};

/// A scheduler job after the config has been validated. Mirrors
/// `SchedulerJob` but with the cron expression already parsed (the runtime
/// drops jobs whose cron fails to parse with a `tracing::warn!` — they
/// don't propagate as fatal errors so one bad job doesn't take the whole
/// scheduler down).
#[derive(Debug, Clone)]
pub struct JobSpec {
    pub name: String,
    pub cron: super::cron::Schedule,
    pub action: ActionSpec,
}

/// Action variants the runtime knows how to dispatch. Today only
/// `Subprocess` is end-to-end; `RunAgent` / `RunTool` are surfaced so the
/// dispatcher can emit a clear "not implemented" outcome on the hook bus
/// rather than silently dropping them.
#[derive(Debug, Clone)]
pub enum ActionSpec {
    Subprocess {
        command: String,
        args: Vec<String>,
        timeout_secs: u64,
        working_dir: Option<PathBuf>,
        env: BTreeMap<String, String>,
    },
    RunAgent {
        prompt: String,
    },
    RunTool {
        plugin: String,
        tool: String,
        args: serde_json::Value,
    },
}

impl JobSpec {
    /// Try to convert a config-level `SchedulerJob` into a runtime-ready
    /// spec. Returns `None` (with `tracing::warn!`) if the cron expression
    /// fails to parse; the caller should drop the job rather than abort
    /// scheduler startup.
    pub fn from_config(job: &SchedulerJob) -> Option<Self> {
        let schedule = match super::cron::parse(&job.cron) {
            Ok(s) => s,
            Err(err) => {
                tracing::warn!(
                    job = %job.name,
                    cron = %job.cron,
                    error = %err,
                    "scheduler: dropping job with unparseable cron",
                );
                return None;
            }
        };
        let action = match &job.action {
            JobAction::Subprocess {
                command,
                args,
                timeout_secs,
                working_dir,
                env,
            } => ActionSpec::Subprocess {
                command: command.clone(),
                args: args.clone(),
                timeout_secs: *timeout_secs,
                working_dir: working_dir.clone(),
                env: env.clone(),
            },
            JobAction::RunAgent { prompt } => ActionSpec::RunAgent {
                prompt: prompt.clone(),
            },
            JobAction::RunTool { plugin, tool, args } => ActionSpec::RunTool {
                plugin: plugin.clone(),
                tool: tool.clone(),
                args: args.clone(),
            },
        };
        Some(Self {
            name: job.name.clone(),
            cron: schedule,
            action,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(cron: &str, action: JobAction) -> SchedulerJob {
        SchedulerJob {
            name: "t".into(),
            cron: cron.into(),
            timezone: None,
            action,
        }
    }

    #[test]
    fn drops_invalid_cron() {
        let j = cfg(
            "not a cron",
            JobAction::Subprocess {
                command: "true".into(),
                args: vec![],
                timeout_secs: 60,
                working_dir: None,
                env: BTreeMap::new(),
            },
        );
        assert!(JobSpec::from_config(&j).is_none());
    }

    #[test]
    fn maps_subprocess_fields() {
        let mut env = BTreeMap::new();
        env.insert("FOO".into(), "bar".into());
        let j = cfg(
            "0 0 3 * * * *",
            JobAction::Subprocess {
                command: "echo".into(),
                args: vec!["hi".into()],
                timeout_secs: 5,
                working_dir: Some(PathBuf::from("/tmp")),
                env,
            },
        );
        let spec = JobSpec::from_config(&j).expect("valid cron");
        match spec.action {
            ActionSpec::Subprocess {
                command,
                args,
                timeout_secs,
                working_dir,
                env,
            } => {
                assert_eq!(command, "echo");
                assert_eq!(args, vec!["hi"]);
                assert_eq!(timeout_secs, 5);
                assert_eq!(working_dir.as_deref(), Some(std::path::Path::new("/tmp")));
                assert_eq!(env.get("FOO").map(String::as_str), Some("bar"));
            }
            other => panic!("expected Subprocess, got {other:?}"),
        }
    }
}
