# Evolution loop — scheduled engine runs

Phase 2 wave 2-B closes the auto-evolution loop by wiring the Python
`corlinman-evolution-engine` CLI as a scheduled subprocess job:

```
┌────────────────────┐  daily 03:00 UTC   ┌──────────────────────────┐
│ corlinman-scheduler│ ─────────────────▶ │ corlinman-evolution-engine │
│  (Rust, in-process)│                    │ (Python, subprocess)        │
└──────────┬─────────┘                    └──────────────┬──────────────┘
           │ exit code + duration                        │ writes proposals
           ▼                                              ▼
┌────────────────────┐    HookBus     ┌────────────────────────────────┐
│ EngineRun{Completed│ ─────────────▶ │ EvolutionObserver               │
│        ,Failed}    │                │  → evolution_signals (sqlite)   │
└────────────────────┘                └────────────────────────────────┘
```

Each run's outcome is itself a signal the *next* run sees. Two consecutive
`engine.run.failed` rows are now visible at `/admin/evolution/signals`.

## Default schedule

```toml
[[scheduler.jobs]]
name = "evolution_engine"
cron = "0 0 3 * * * *"   # 03:00:00 UTC daily, 7-field cron
action = { type = "subprocess", command = "corlinman-evolution-engine",
           args = ["run-once"], timeout_secs = 600, working_dir = "/data",
           env = { CORLINMAN_EVOLUTION_DB = "/data/evolution.sqlite",
                   CORLINMAN_KB_DB = "/data/kb.sqlite" } }
```

To shift the schedule, edit the `cron` field in your `corlinman.toml`. The
expression uses the 7-field grammar (`sec min hour day month weekday year`)
that `corlinman doctor`'s `scheduler` check already validates.

To pin different DB paths or pass extra environment variables to the
engine, add them under `env`. The map merges over the gateway's inherited
environment.

## Diagnostics

Two surfaces report engine-run health:

1. **Logs** — `/admin/logs?source=scheduler` shows the per-line stdout
   (info) and stderr (warn) forwarded from the child, tagged with the
   `job` and `run_id` fields. Spawn / timeout / non-zero exits log as
   `error` records.
2. **Signals** — `/admin/evolution/signals?event_kind=engine.run.failed`
   filters down to failed runs. Each row carries the `error_kind` (one of
   `exit_code`, `timeout`, `spawn_failed`, `unsupported_action`) and the
   `exit_code` (when applicable) in its JSON payload.

A failure path checklist:

- `error_kind = spawn_failed` — `corlinman-evolution-engine` is not on
  `$PATH`. In production this is the venv at `/opt/venv/bin/`; in dev
  invoke via `uv run` (see below).
- `error_kind = exit_code` — the engine itself raised. The stderr lines
  in `/admin/logs` show the Python traceback.
- `error_kind = timeout` — the engine ran past `timeout_secs` and was
  hard-killed. Either bump the timeout or look for an upstream RPC hang.

## Dev quickstart

In dev environments where the Python package is not installed into a
venv on `$PATH`, point the job's `command` at `uv` and use the
[`run`](https://docs.astral.sh/uv/reference/cli/#uv-run) subcommand:

```toml
action = { type = "subprocess", command = "uv",
           args = ["run", "--package", "corlinman-evolution-engine",
                   "corlinman-evolution-engine", "run-once"],
           timeout_secs = 600, working_dir = "<repo root>" }
```

In production the Docker image bakes `corlinman-evolution-engine` into
`/opt/venv/bin/`, which is already on `$PATH` for the gateway entrypoint;
the default config above works as-is.
