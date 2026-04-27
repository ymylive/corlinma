//! `corlinman-shadow-tester` CLI — Phase 3 Wave 1-A Step 4.
//!
//! Thin wrapper that loads a corlinman config, opens
//! `evolution.sqlite`, registers the shipping per-kind simulators, and
//! runs one [`ShadowRunner::run_once`] pass. Designed to be invoked by
//! `corlinman-scheduler` as a subprocess job — same pattern as the
//! Phase 2 wave 2-B `evolution_engine` job.
//!
//! Subcommands ship as a clap derive enum so future kinds can add their
//! own (`run-case`, `replay`, ...) without churning the top-level
//! plumbing.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Parser, Subcommand};
use corlinman_core::config::{Config, ShadowSandboxKind};
use corlinman_evolution::{EvolutionStore, ProposalsRepo};
use corlinman_shadow_tester::simulator::MemoryOpSimulator;
use corlinman_shadow_tester::ShadowRunner;
use tracing::{error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Top-level CLI. Mirrors `corlinman-cli`'s style — a single `Cli` with
/// a `Cmd` subcommand enum so each subcommand carries its own args.
#[derive(Debug, Parser)]
#[command(
    name = "corlinman-shadow-tester",
    version,
    about = "ShadowTester — shadow-runs medium/high-risk EvolutionProposals against an in-process eval set."
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Run one shadow pass: claim pending medium/high-risk proposals,
    /// execute their eval sets, persist `shadow_metrics` + baseline +
    /// `eval_run_id`, then exit. Designed for cron invocation by
    /// `corlinman-scheduler`.
    RunOnce(RunOnceArgs),
}

#[derive(Debug, Parser)]
struct RunOnceArgs {
    /// Path to the corlinman config (`corlinman.toml`). Reads
    /// `[evolution.shadow]` + `[evolution.observer].db_path` +
    /// `[server].data_dir`.
    #[arg(long)]
    config: PathBuf,

    /// Per-run cap on proposals claimed; overrides the runner default
    /// (10) when set. Useful for one-off backfills.
    #[arg(long)]
    max_proposals: Option<usize>,
}

/// Single-threaded tokio runtime — this is a short-lived job; no need
/// to pay the multi-threaded scheduler tax.
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let cli = Cli::parse();
    match cli.command {
        Cmd::RunOnce(args) => run_once(args).await,
    }
}

/// Wire `tracing_subscriber` with a `RUST_LOG` env-filter (default
/// `info`). Matches the gateway / cli pattern so operators get the
/// same log format / filter syntax everywhere.
fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_target(true))
        .init();
}

/// `run-once` flow:
///
/// 1. Load `Config` from `--config`.
/// 2. Bail if `[evolution.shadow].enabled = false` — the operator must
///    explicitly opt in. Silent no-op would hide a misconfigured cron.
/// 3. Reject `sandbox_kind = docker` (Phase 4 reservation).
/// 4. Open `EvolutionStore` at `[evolution.observer].db_path`, build
///    `ProposalsRepo`.
/// 5. Resolve kb.sqlite at `<data_dir>/kb.sqlite`, with
///    `$CORLINMAN_DATA_DIR` overriding `[server].data_dir` (mirrors
///    `corlinman-gateway`'s `resolve_data_dir`).
/// 6. Build `ShadowRunner`, register the shipping simulators
///    (currently `memory_op` only), apply `--max-proposals` if set.
/// 7. Call `runner.run_once()`, log a one-line summary, exit 0.
///
/// Per-case failures are recorded by the runner in DB and DO NOT cause
/// a non-zero exit — a cron green / red signal must reflect *infra*
/// health, not eval-case outcomes.
async fn run_once(args: RunOnceArgs) -> anyhow::Result<()> {
    let config = Config::load_from_path(&args.config).map_err(|e| {
        error!(path = %args.config.display(), error = %e, "shadow_tester: failed to load config");
        anyhow::anyhow!("load config {}: {e}", args.config.display())
    })?;

    let shadow_cfg = &config.evolution.shadow;
    if !shadow_cfg.enabled {
        error!(
            "shadow_tester: [evolution.shadow].enabled = false — refusing to run. \
             Set it to true once you've authored the eval set, or remove the cron job."
        );
        anyhow::bail!("shadow disabled");
    }

    if shadow_cfg.sandbox_kind != ShadowSandboxKind::InProcess {
        error!(
            sandbox_kind = ?shadow_cfg.sandbox_kind,
            "shadow_tester: only `in_process` sandbox is supported in Phase 3 — `docker` is reserved for Phase 4."
        );
        anyhow::bail!("unsupported sandbox_kind");
    }

    let evolution_db = config.evolution.observer.db_path.clone();
    let eval_set_dir = shadow_cfg.eval_set_dir.clone();
    let kb_path = resolve_kb_path(&config.server.data_dir);

    info!(
        evolution_db = %evolution_db.display(),
        kb_path = %kb_path.display(),
        eval_set_dir = %eval_set_dir.display(),
        "shadow_tester: opening evolution store"
    );

    let store = EvolutionStore::open(&evolution_db).await.map_err(|e| {
        error!(path = %evolution_db.display(), error = %e, "shadow_tester: open evolution.sqlite failed");
        anyhow::anyhow!("open {}: {e}", evolution_db.display())
    })?;
    let proposals = ProposalsRepo::new(store.pool().clone());

    let mut runner = ShadowRunner::new(proposals, kb_path, eval_set_dir);
    if let Some(n) = args.max_proposals {
        runner = runner.with_max_proposals_per_run(n);
    }
    // Future kinds register here. Keep one line per simulator so adding
    // skill_update / prompt_update is a one-line diff.
    runner.register_simulator(Arc::new(MemoryOpSimulator));

    let summary = runner.run_once().await;

    // One-line summary modeled after the Python evolution-engine CLI's
    // `_print_summary` (key: value pairs) but folded into a single
    // tracing event so scheduler log capture sees one record per run.
    info!(
        proposals_claimed = summary.proposals_claimed,
        proposals_completed = summary.proposals_completed,
        proposals_failed = summary.proposals_failed,
        cases_run = summary.cases_run,
        errors = summary.errors,
        "shadow_tester: run-once complete"
    );

    Ok(())
}

/// Resolve kb.sqlite the same way the gateway does: env override
/// (`CORLINMAN_DATA_DIR`) wins so dev / test invocations don't have to
/// rewrite the config; otherwise fall back to `[server].data_dir`.
fn resolve_kb_path(config_data_dir: &Path) -> PathBuf {
    if let Ok(env_dir) = std::env::var("CORLINMAN_DATA_DIR") {
        return PathBuf::from(env_dir).join("kb.sqlite");
    }
    config_data_dir.join("kb.sqlite")
}
