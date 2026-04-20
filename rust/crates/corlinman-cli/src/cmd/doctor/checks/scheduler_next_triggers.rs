//! Verify every configured scheduler job can compute a *next trigger*
//! relative to now. A parseable-but-never-fires cron expression (possible
//! on invalid DOW/month combos the basic parser accepts) shows up here.
//!
//! This complements [`super::scheduler::SchedulerCheck`] which only
//! validates the cron grammar.

use std::str::FromStr;

use async_trait::async_trait;
use cron::Schedule;

use super::{DoctorCheck, DoctorContext, DoctorResult};

pub struct SchedulerNextTriggersCheck;

impl SchedulerNextTriggersCheck {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SchedulerNextTriggersCheck {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DoctorCheck for SchedulerNextTriggersCheck {
    fn name(&self) -> &str {
        "scheduler_next_triggers"
    }

    async fn run(&self, ctx: &DoctorContext) -> DoctorResult {
        let Some(cfg) = ctx.config.as_ref() else {
            return DoctorResult::Warn {
                message: "skipped: config not loaded".into(),
                hint: Some("fix the config check first".into()),
            };
        };
        if cfg.scheduler.jobs.is_empty() {
            return DoctorResult::Ok {
                message: "no jobs to trigger".into(),
            };
        }
        let now = chrono::Utc::now();
        let mut failures: Vec<String> = Vec::new();
        for (idx, job) in cfg.scheduler.jobs.iter().enumerate() {
            let sched = match Schedule::from_str(&job.cron) {
                Ok(s) => s,
                Err(e) => {
                    failures.push(format!("jobs[{idx}] ({}) unparseable: {e}", job.name));
                    continue;
                }
            };
            if sched.after(&now).next().is_none() {
                failures.push(format!(
                    "jobs[{idx}] ({}) has no upcoming trigger after now",
                    job.name
                ));
            }
        }
        if failures.is_empty() {
            DoctorResult::Ok {
                message: format!("{} job(s) have upcoming triggers", cfg.scheduler.jobs.len()),
            }
        } else {
            DoctorResult::Warn {
                message: failures.join("; "),
                hint: Some("check each job's cron expression against cronexp.com".into()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use corlinman_core::config::{Config, JobAction, SchedulerJob};

    fn ctx_with(config: Option<Config>) -> DoctorContext {
        DoctorContext {
            data_dir: std::path::PathBuf::from("/tmp"),
            config_path: std::path::PathBuf::from("/tmp/config.toml"),
            config,
        }
    }

    #[tokio::test]
    async fn no_jobs_is_ok() {
        let ctx = ctx_with(Some(Config::default()));
        let res = SchedulerNextTriggersCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok");
    }

    #[tokio::test]
    async fn every_minute_has_next_trigger() {
        let mut cfg = Config::default();
        cfg.scheduler.jobs.push(SchedulerJob {
            name: "every_minute".into(),
            cron: "0 * * * * * *".into(),
            timezone: None,
            action: JobAction::RunAgent { prompt: "x".into() },
        });
        let ctx = ctx_with(Some(cfg));
        let res = SchedulerNextTriggersCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok", "got: {res:?}");
    }
}
