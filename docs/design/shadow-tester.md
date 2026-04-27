# ShadowTester — Design

**Status**: Draft · **Phase**: 3 Wave 1-A · **Owner**: TBD · **Last revised**: 2026-04-27

> Companion to `docs/design/auto-evolution.md` and `docs/design/phase3-roadmap.md`.
> Operator-facing upgrade notes live in `docs/migration/v2-to-v3.md`.

---

## 1. Purpose

The ShadowTester is the safety gate Phase 2 deferred. Phase 2 closed the
EvolutionLoop on `memory_op` only — a low-risk kind where the engine can
write a proposal, the operator can read it, and the worst-case applied
outcome is "merged two chunks that probably weren't dupes." Phase 3
unblocks medium and high-risk kinds (`skill_update`, `agent_card`,
`prompt_template`, `tool_policy`) where the same casual review surface
is no longer enough — the operator needs *measured* deltas, not just
diff text. The ShadowTester runs every medium/high-risk proposal
through a frozen eval set in a sandboxed copy of `kb.sqlite`, captures
pre/post metrics, and attaches them to the proposal row before the
operator sees it. It does not approve, reject, or roll anything back —
it only annotates.

## 2. Position in the EvolutionLoop

```
hook → Observer → signals → Engine → pending proposal
                                          │
                                          ▼
                                ShadowTester (medium/high-risk only)
                                          │
                                          ▼
                                shadow_done → operator queue
                                          │
                                          ▼
                                      Applier
```

Low-risk kinds (today only `memory_op`) bypass ShadowTester entirely:
they remain on the Phase 2 `pending → approved → applied` path, which
keeps the existing v0.2 flow byte-for-byte unchanged when
`[evolution.shadow].enabled = false` (the shipped default).

The Engine itself is unaffected by this wave. ShadowTester is a
*reader* of `evolution_proposals` rows: it claims pending high-risk
rows, runs the eval set, writes back the metrics, and transitions the
status. The Engine doesn't know the tester exists.

## 3. Data model

W1-A Step 1 added three columns to `evolution_proposals`. The
authoritative DDL lives in
[`rust/crates/corlinman-evolution/src/schema.rs`](../../rust/crates/corlinman-evolution/src/schema.rs)
(`SCHEMA_SQL` for fresh DBs, `MIGRATIONS` for v0.2 → v0.3 ALTERs).

| Column | Type | Role |
|---|---|---|
| `shadow_metrics` | TEXT (JSON, nullable) | Aggregate post-change measurements across all eval cases. Operator-rendered as the "after" side of the delta. |
| `baseline_metrics_json` | TEXT (JSON, nullable) | Aggregate pre-change measurements taken from the same sandboxed kb before each case applied. Operator-rendered as the "before" side of the delta. |
| `eval_run_id` | TEXT (nullable) | Opaque ULID-ish tag identifying one ShadowRunner invocation. Joins back to runner-emitted log lines and metrics. |

All three are nullable because (a) low-risk rows skip the tester
entirely, and (b) v0.2 rows pre-dating the migration have `NULL` for
the new columns and must continue to render correctly.

Example `shadow_done` row (truncated to relevant columns):

| id | kind | risk | status | shadow_metrics | baseline_metrics_json | eval_run_id |
|---|---|---|---|---|---|---|
| evol-2026-04-27-003 | memory_op | high | shadow_done | `{"pass_rate":1.0,"p50_latency_ms":12,"p95_latency_ms":33,"failed_cases":[]}` | `{"chunks_total":4,"target_chunk_ids":[1,2]}` | `01HKX...` |

The `shadow_metrics` keyset is per-kind (memory_op uses `chunks_total`
/ `target_chunk_ids`; future skill_update will use `success_rate` /
`p95_latency_ms`). The aggregator (Section 7) wraps per-case maps with
fixed cross-case keys (`pass_rate`, latency percentiles, `failed_cases`).

## 4. Eval-case spec

Eval cases live as YAML files under `<eval_set_dir>/<kind>/*.yaml`.
The per-kind subdir is the contract — the loader defaults each case's
`kind` from its directory, so authors don't repeat themselves, and
`ls memory_op/` is the way to audit coverage. The shipped seed set
lives at
[`rust/crates/corlinman-shadow-tester/tests/fixtures/eval/memory_op/`](../../rust/crates/corlinman-shadow-tester/tests/fixtures/eval/memory_op/).
Authoritative types are in
[`rust/crates/corlinman-shadow-tester/src/eval.rs`](../../rust/crates/corlinman-shadow-tester/src/eval.rs).

YAML shape (one case = one file):

| Field | Required | Purpose |
|---|---|---|
| `name` | optional | Defaults to the YAML file stem. Used for sort order + log lines. |
| `kind` | optional | Defaults to the directory name. If set explicitly and disagrees, the loader errors. |
| `description` | required | Free-form prose. Surfaced in the operator UI as the "why this case exists" hover. |
| `kb_seed` | optional | List of raw SQL statements run against the sandbox kb before the proposal applies. |
| `proposal` | required | `target` + `reasoning` + optional `risk` (default `high`) + optional `signal_ids`. |
| `expected` | required | Tagged enum (`outcome: merged \| no_op` today) with kind-specific fields. |

A real case (
[`case-001-near-duplicate-merge.yaml`](../../rust/crates/corlinman-shadow-tester/tests/fixtures/eval/memory_op/case-001-near-duplicate-merge.yaml)
):

```yaml
description: >
  Two paraphrase chunks differing by one synonym (Jaccard ~0.97). The
  proposal targets a merge of chunks 1 and 2; the simulator should keep
  chunk 1 as canonical and retire chunk 2.
kb_seed:
  - "INSERT INTO files(...) VALUES (1, 'fx-001.md', ...);"
  - "INSERT INTO chunks(...) VALUES (1, 1, 0, 'The quick brown fox jumps ...', 'general');"
  - "INSERT INTO chunks(...) VALUES (2, 1, 1, 'The quick brown fox leaps ...', 'general');"
proposal:
  target: "merge_chunks:1,2"
  reasoning: "Near-duplicate paraphrase pair detected by EvolutionEngine"
  risk: high
expected:
  outcome: merged
  rows_merged: 1
  surviving_chunk_id: 1
```

Loader rules of note (see `load_eval_set` in `eval.rs`):

- Files prefixed with `_` are skipped (drafts in flight).
- An empty `<dir>/<kind>/` is an error (`EvalLoadError::EmptySet`) —
  silently shadowing zero cases would render proposals "green" forever.
- Cases sort by `name` for deterministic ordering across runs.
- A `kind:` field that disagrees with the directory is rejected up front.

**Operators add cases by**: dropping a YAML file under
`<eval_set_dir>/<kind>/`, or — once the closed-loop ships in W1-A
follow-up — flagging an approved proposal in the admin UI which
distills a case from the captured trace and writes it to the same
directory. The tester does not auto-curate.

## 5. `KindSimulator` trait

A simulator handles one kind. The runner holds a registry keyed by
`EvolutionKind` and dispatches at run time. Authoritative trait + impl
live at
[`rust/crates/corlinman-shadow-tester/src/simulator.rs`](../../rust/crates/corlinman-shadow-tester/src/simulator.rs).

The contract:

- **Input**: an `EvalCase` plus a path to a tempdir copy of `kb.sqlite`
  the runner has already created and seeded with `case.kb_seed`.
- **Output**: a `SimulatorOutput { case_name, passed, baseline,
  shadow, latency_ms, error }`.

The simulator's job is to (1) read pre-state into `baseline`, (2)
apply the proposal's operation against the tempdir DB only, (3) read
post-state into `shadow`, (4) compare against `case.expected` to set
`passed`, and (5) return. It must not open any file other than
`kb_path`; the runner enforces the sandbox by handing it a tempdir,
but the simulator authors the discipline.

The baseline/shadow split is the whole point of the gate. Phase 2's
`shadow_metrics` was a single post-change blob — the operator could
see "after the merge, 1 chunk remains" but not "before the merge, 2
chunks existed." Phase 3 splits the same JSON into two columns so the
operator UI can render `before → after` diffs (Section 7 aggregates).

`baseline` and `shadow` are free-form `serde_json::Map`s on purpose:
each kind picks its own metric vocabulary. memory_op uses
`chunks_total` and `target_chunk_ids`; skill_update will use
`success_rate` and `p95_latency_ms`; the runner aggregates without
inspecting the keys.

`SimulatorError` is recoverable: the runner downgrades it to
`SimulatorOutput { passed: false, error: Some(...) }` rather than
aborting the eval run. One bad case must not poison the rest.

## 6. Sandbox model

Phase 3 ships `sandbox_kind = "in_process"` only. For each case, the
runner:

1. Creates a fresh tempdir (Rust `tempfile::TempDir`, auto-cleaned).
2. Copies the production `kb.sqlite` into it (or starts blank if the
   case wants a from-scratch fixture; `kb_seed` decides).
3. Replays `case.kb_seed` against the tempdir DB.
4. Hands the tempdir path to the simulator.
5. Drops the tempdir on case completion.

Production `kb.sqlite` is opened **read-only** by the copy step and
never opened at all by the simulator. There is no path by which a
shadow run can mutate prod state — violating that is a runner-policy
bug, not a simulator one, and the test suite gates the invariant.

The reason this is "good enough" for Phase 3 is that the only kind
shadowing today is `memory_op`, whose simulator runs SQL against a
SQLite file. There's no prompt evaluation, no LLM call, no tool
invocation — nothing that can leak state through a global. When Phase
4 adds `prompt_template` / `tool_policy`, the in-process leak surface
gets real (cached HTTP clients, tokenizer state, file handles), and
that's when `ShadowSandboxKind::Docker` starts paying for itself. The
enum variant exists today (
[`config.rs`](../../rust/crates/corlinman-core/src/config.rs)
) but the loader rejects `docker` as unimplemented.

## 7. Aggregation

A proposal owns N eval cases (typically 4-20). The runner produces
N `SimulatorOutput`s and aggregates them into two proposal-level JSON
blobs:

`baseline_metrics_json` (pre-change, written before the first case
applies). Shape:

```json
{
  "case_count": 4,
  "per_case": [
    { "case_name": "case-001-near-duplicate-merge", "metrics": { "chunks_total": 2, "target_chunk_ids": [1, 2] } },
    ...
  ]
}
```

`shadow_metrics` (post-change). Shape:

```json
{
  "pass_rate": 1.0,
  "passed_count": 4,
  "case_count": 4,
  "p50_latency_ms": 12,
  "p95_latency_ms": 33,
  "failed_cases": [],
  "per_case": [
    { "case_name": "case-001-near-duplicate-merge", "passed": true, "metrics": { ... } },
    ...
  ]
}
```

`pass_rate` is the headline number the operator UI sorts by.
`failed_cases` is the list of `case_name`s where `passed = false`,
each with the simulator's `error` field if any — that's where the
operator looks first when triaging a < 1.0 pass rate.

`per_case` is included in both blobs for traceability; the UI
collapses it behind "show details" by default. p50/p95 are computed
across `case.latency_ms` values regardless of pass/fail (a fast
failure is still a fast failure).

## 8. Status transitions

```
pending ──claim──► shadow_running ──run+write──► shadow_done
   │                     │
   │                     └── (race-loss) → leave alone, log INFO
   │
   └── (low-risk path, unchanged from Phase 2) ──► approved
```

The runner claims a row by an atomic
`UPDATE evolution_proposals SET status='shadow_running' WHERE id=? AND status='pending'`
returning rowcount. A returned `0` means another runner instance won
the race; the loser logs INFO and moves on. There is exactly one
ShadowTester scheduler in v0.3, but the claim pattern is cheap and
makes the future "two operators trigger a run by hand at the same
moment" case safe.

`shadow_done` does **not** mean "passed." It means "we ran the eval
set and wrote the metrics." A `pass_rate` of 0.0 is a perfectly
valid `shadow_done` row — that's the signal the operator needs to
deny the proposal.

There is intentionally no `shadow_failed` status. Failure modes
that Phase 2-style designs would split out (case errors, simulator
panics, sandbox setup blew up) are recorded *inside* `shadow_metrics`
so the operator review surface is one place, not two. A whole-run
abort (e.g. eval-set dir missing) leaves the row at `shadow_running`
with a runner-emitted alert; the operator can re-trigger or manually
move it back to `pending`.

## 9. Configuration

The block lives in `[evolution.shadow]`. Authoritative type:
`EvolutionShadowConfig` in
[`rust/crates/corlinman-core/src/config.rs`](../../rust/crates/corlinman-core/src/config.rs).

```toml
[evolution.shadow]
enabled = false                         # ships off; opt in after authoring eval cases
eval_set_dir = "/data/eval/evolution"   # <dir>/<kind>/*.yaml is the contract
sandbox_kind = "in_process"             # 'in_process' | 'docker' (Phase 4)
```

`enabled = false` is the shipped default for v0.3. The Phase 2 flow
on `memory_op` keeps working byte-for-byte: the runner doesn't
schedule, the columns stay NULL, the operator surface looks the same
as v0.2. Operators flip this to `true` after they've reviewed (and
optionally extended) the bundled eval set under `eval_set_dir`. The
operator runbook for that flip lives in `docs/migration/v2-to-v3.md`.

`sandbox_kind = "docker"` parses but the loader emits a
config-validation error pointing at this design doc — the runner
doesn't yet support it.

## 10. What ShadowTester does NOT do

Explicit non-goals:

- **Does not run for low-risk kinds.** `memory_op` rows go straight to
  `pending → approved` exactly as Phase 2 shipped. The risk filter is
  applied at claim time, not at scheduler time.
- **Does not auto-approve or auto-reject.** A 0.0 pass rate annotates
  the row; it does not change `status` to `denied`. The operator decides.
- **Does not modify production kb / config / agent state.** Tempdir
  in, tempdir out. Violations are runner-policy bugs.
- **Does not run during Phase 2's existing memory_op flow when shadow is
  disabled.** With `[evolution.shadow].enabled = false` the runner
  doesn't schedule; the v0.3 binary on a v0.2 config is a no-op delta.
- **Does not provide rollback.** Metrics-degradation auto-revert is
  W1-B AutoRollback's job. ShadowTester is a *pre-apply* gate; AutoRollback
  is a *post-apply* watchdog. They share the proposal row but not the
  responsibility.
- **Does not author eval cases.** Operators (or the closed-loop
  flagging UX, when it lands) write YAML files. The tester loads what's
  on disk.

## 11. What's left for W1-A Step 4

Step 4 is the only remaining piece of W1-A:

1. **Scheduler wiring.** Register `ShadowRunner::run` as a periodic
   `corlinman-scheduler` job. Cadence is roughly "30 minutes after the
   evolution-engine cron" — the engine writes pending proposals on its
   tick; the runner catches them up before the operator's next morning
   review window. Exact crontab + jitter TBD in the Step 4 PR.
2. **End-to-end test.** A test that drives the full path: produce a
   signal, run the engine to write a `pending` row, fire the scheduler
   tick, assert the row transitions `pending → shadow_running →
   shadow_done`, assert `shadow_metrics` and `baseline_metrics_json`
   are populated and the operator API endpoint returns a renderable
   delta. This proves the contract holds across the language boundary
   (Python writes, Rust reads, admin UI fetches).

Once Step 4 lands, W1-A closes and W1-B (AutoRollback) can start.
W1-C (Budget enforcement) is independent and can run in parallel.
