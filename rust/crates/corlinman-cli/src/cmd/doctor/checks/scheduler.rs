//! Scheduler cron-expression check.
//!
//! Every `[[scheduler.jobs]].cron` expression is passed through the `cron`
//! crate's parser. Any parse failure is a `Fail`; zero jobs is an `Ok`
//! (scheduler simply has nothing to run).

use std::str::FromStr;

use async_trait::async_trait;
use cron::Schedule;

use super::{DoctorCheck, DoctorContext, DoctorResult};

pub struct SchedulerCheck;

impl SchedulerCheck {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SchedulerCheck {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DoctorCheck for SchedulerCheck {
    fn name(&self) -> &str {
        "scheduler"
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
                message: "no scheduler jobs declared".into(),
            };
        }

        let mut errors: Vec<String> = Vec::new();
        for (idx, job) in cfg.scheduler.jobs.iter().enumerate() {
            if let Err(e) = Schedule::from_str(&job.cron) {
                errors.push(format!(
                    "scheduler.jobs[{idx}] ({}): invalid cron `{}`: {e}",
                    job.name, job.cron
                ));
            }
        }

        if errors.is_empty() {
            DoctorResult::Ok {
                message: format!("{} job(s) parse cleanly", cfg.scheduler.jobs.len()),
            }
        } else {
            let first = &errors[0];
            let more = if errors.len() > 1 {
                format!(" (+{} more)", errors.len() - 1)
            } else {
                String::new()
            };
            DoctorResult::Fail {
                message: format!("{first}{more}"),
                hint: Some(
                    "corlinman uses the 7-field cron format (sec min hour day month weekday year)"
                        .into(),
                ),
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
        let res = SchedulerCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok", "got: {:?}", res);
    }

    #[tokio::test]
    async fn valid_cron_is_ok() {
        let mut cfg = Config::default();
        cfg.scheduler.jobs.push(SchedulerJob {
            name: "every minute".into(),
            cron: "0 * * * * * *".into(),
            timezone: None,
            action: JobAction::RunAgent {
                prompt: "tick".into(),
            },
        });
        let ctx = ctx_with(Some(cfg));
        let res = SchedulerCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok", "got: {:?}", res);
    }

    #[tokio::test]
    async fn invalid_cron_is_fail() {
        let mut cfg = Config::default();
        cfg.scheduler.jobs.push(SchedulerJob {
            name: "broken".into(),
            cron: "not a cron".into(),
            timezone: None,
            action: JobAction::RunAgent { prompt: "x".into() },
        });
        let ctx = ctx_with(Some(cfg));
        let res = SchedulerCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "fail", "got: {:?}", res);
    }
}
