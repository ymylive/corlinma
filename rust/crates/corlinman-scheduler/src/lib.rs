//! corlinman-scheduler — cron-based periodic job runner.
//!
//! Phase 2 wave 2-B: this crate's first concrete responsibility is wiring
//! the Python `corlinman-evolution-engine run-once` CLI as a subprocess
//! job that fires on a daily cron. The scheduler:
//!
//! 1. parses each `[[scheduler.jobs]]` cron expression at boot
//!    (7-field format via the `cron` crate);
//! 2. spawns one `tokio` task per job, parked on `tokio::time::sleep_until`
//!    of the next fire time;
//! 3. when a job fires, dispatches by `JobAction` kind. Today only
//!    `Subprocess` is implemented end-to-end; `RunAgent` / `RunTool` log
//!    a `not_implemented` warning and emit `EngineRunFailed` with
//!    `error_kind = "unsupported_action"` so the failure is observable
//!    on the bus instead of silently dropped.
//! 4. emits `HookEvent::EngineRunCompleted` / `EngineRunFailed` so the
//!    `EvolutionObserver` folds outcomes into `evolution_signals`.
//!
//! Shutdown of the gateway cascades into the scheduler via the shared
//! `CancellationToken`: pending sleeps get cancelled and any in-flight
//! subprocess is sent SIGKILL.

pub mod cron;
pub mod jobs;
pub mod runtime;
pub mod subprocess;

pub use runtime::{spawn, SchedulerHandle};
pub use subprocess::{run_subprocess, SubprocessOutcome};
