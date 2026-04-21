# corlinman 0.2.0 — custom LLM + embedding parameters (end to end)

Released 2026-04-21. Major release: dynamic provider registry, per-alias
model params, first-class embedding configuration, and the admin UI to
manage all of it.

## Highlights

- **Custom providers via config**. Declare any number of first-party or
  OpenAI-compatible providers under `[providers.<name>]` with a `kind`
  discriminator. Providers gain a `params` map for user-tunable defaults
  and a per-kind JSON Schema that the admin UI renders as a dynamic
  form.
- **Per-alias model params**. `[models.aliases.<alias>]` now carries a
  `params` table (temperature / top_p / max_tokens / system_prompt /
  timeout_ms + provider-specific extras like `reasoning_effort` on
  OpenAI o1/o3 or `safety_settings` on Google) that flows into the
  reasoning loop. Precedence is `request > alias > provider`.
- **Embedding as a first-class section**. New `[embedding]` config
  (provider, model, dimension, enabled, params) wired to a dedicated
  `CorlinmanEmbeddingProvider` abstraction with an openai-compatible
  primary target and a Google reference implementation.
- **Benchmark tool**. `POST /admin/embedding/benchmark` (server-side
  wiring pending, UI-ready) returns p50/p99 latency, an N×N cosine
  similarity matrix, and dimension cross-check warnings for a batch
  of sample strings.
- **Admin UI**: two new pages (`/providers`, `/embedding`) plus an
  inline-accordion on `/models` for per-alias param editing. All driven
  by a new `<DynamicParamsForm>` component that walks JSON Schema draft
  2020-12 into controls (slider / select / switch / textarea / nested
  fieldset) with inline validation.

## Added

### Rust

- `corlinman-core::config`: `ProviderKind` enum (`anthropic | openai |
  google | deepseek | qwen | glm | openai_compatible`); `ParamsMap` on
  every `ProviderEntry`; `AliasEntry` as an untagged enum (shorthand
  string | full struct with `model`/`provider?`/`params`); new
  `EmbeddingConfig` with cross-field validation.
- `corlinman-gateway` admin routes:
  - `GET/POST/PATCH/DELETE /admin/providers` — list, upsert, patch,
    delete-with-409-reference-guard. JSON Schema per kind is baked
    into Rust (Python is canonical; see module docs).
  - `GET/POST /admin/embedding` — config CRUD.
  - `POST /admin/embedding/benchmark` — currently 501
    `pending_python_implementation`; UI handles the fallback gracefully.
  - `POST /admin/models/aliases` extended with single-row upsert
    alongside legacy bulk-replace (untagged enum keeps pre-0.2 configs
    parsing); `DELETE /admin/models/aliases/:name` added.
- Hand-rolled scalar JSON Schema validator for upsert payloads —
  no new workspace dep.
- Atomic config persistence + `ArcSwap` hot-swap after every successful
  write.

### Python

- `corlinman_providers.specs.ProviderSpec` + `EmbeddingSpec` pydantic
  models.
- `params_schema()` classmethod on every provider class (Anthropic,
  OpenAI, Google, DeepSeek, Qwen, GLM, OpenAI-compatible).
- Fully rewritten `corlinman_providers.registry.ProviderRegistry`
  (spec-driven) with a legacy prefix-table fallback for raw model ids.
- New `corlinman_embedding.provider.CorlinmanEmbeddingProvider` ABC +
  `openai_compatible.OpenAICompatibleEmbeddingProvider` +
  `google.GoogleEmbeddingProvider`.
- `corlinman_embedding.benchmark.benchmark_embedding()` helper.
- Agent servicer threads merged params through the reasoning loop via
  a new optional `extra: dict` on `ChatStart`.
- Config channel: Python reads `CORLINMAN_PY_CONFIG` → JSON file path.
  Unset = empty config = legacy-fallback behaviour (preserves existing
  deployments that rely on prefix matching).

### UI

- `/providers/page.tsx` — table + upsert/delete modal with the dynamic
  form bound to the provider kind's schema; 409 references surfaced as
  a blocking confirm with an unbind checklist.
- `/embedding/page.tsx` — provider + model + dimension + enabled picker
  with a benchmark panel (CSS-grid cosine heatmap + p50/p99 + warnings).
- `/models/page.tsx` — inline accordion per alias row with the dynamic
  form; legacy `Record<string,string>` alias shape still renders.
- `components/dynamic-params-form.tsx` — hand-rolled JSON-Schema →
  form walker (6 vitest cases).
- Sidebar: Providers + Embedding entries. i18n: ~145 new keys across
  both zh-CN and en (parity enforced by `satisfies LocaleBundle`).
- `api.ts` — typed helpers for every new endpoint.

## Fixed

- **`/admin/approvals` 503 → 200 in production**: the boot path now
  constructs `ApprovalGate` from the live config handle + the already-
  open RAG SQLite (which carries the `pending_approvals` table via
  vector migration v3) and attaches it to `AdminState`. See commit
  `3246fe1`.

## Changed

- **Docker image drops the ui-builder stage.** Building the Next.js
  export inside the container under Rosetta 2 / QEMU user-mode on
  Apple Silicon segfaults the emulated node on some runs. Production
  already served the static bundle from nginx on the host; bundling
  it was dead weight. Operators who want the gateway to serve the UI
  directly can still bind-mount a prebuilt bundle at `/app/ui-static`.

## Stability

- Rust: `cargo fmt / clippy / test` — 154 gateway tests green.
- Python: `ruff / mypy / pytest -m "not live_*"` — 77 passed, 1 skipped.
- UI: `pnpm typecheck / lint / test / build` — 13 vitest cases green.
- All Playwright E2E selectors preserved.

## Known issues / next steps

- `/admin/embedding/benchmark` returns `501 pending_python_implementation`.
  UI renders it gracefully; the gRPC plumbing to the Python helper is
  the next commit.
- Rust gateway doesn't yet write `CORLINMAN_PY_CONFIG` for its Python
  subprocess, so the Python-side config-driven registry currently reads
  empty — chat still works through the legacy prefix fallback, but the
  new params-merge path is dormant until the boot handshake lands.

## Upgrade

```bash
# Pull + rebuild image (host must have docker + buildx).
git pull
docker buildx build --platform linux/amd64 \
  -f docker/Dockerfile -t corlinman:v0.2.0 --target runtime --load .

# Build + rsync the UI bundle (served by nginx, NOT by the container).
pnpm -C ui build
rsync -az --delete ui/out/ <user>@<host>:/path/to/ui-static/

# Restart.
docker compose -f docker/compose/docker-compose.yml up -d
```
