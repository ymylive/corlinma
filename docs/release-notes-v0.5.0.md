# corlinman 0.5.0 — free-form named providers

Unreleased. Tracks features merged after the v0.4.0 Tidepool UI rollup.

## Highlights

- **Free-form provider configuration.** `[providers.*]` is now a
  free-form `BTreeMap<String, ProviderEntry>` keyed by an
  operator-chosen name instead of a fixed slot table. Add OpenRouter,
  SiliconFlow, Ollama, vLLM, or any other OpenAI-wire-compatible
  vendor by writing two TOML lines — no Rust patch required.
- **Seven new market `kind` variants** alongside the existing seven:
  `mistral`, `cohere`, `together`, `groq`, `replicate`, `bedrock`,
  `azure`. The first five route through the shared
  `OpenAICompatibleProvider` Python adapter with sensible default
  base URLs; `bedrock` and `azure` are declared in the schema but
  raise `NotImplementedError` at build time pending real SigV4 /
  deployment-routing support.
- **New providers reference doc** ([`docs/providers.md`](providers.md))
  covering the schema, the per-kind capability table (auth shape,
  default base URL, embedding support), the recipe for adding a new
  vendor in operator-config alone, and four end-to-end recipes
  (OpenRouter + OpenAI embedding, fully-local Ollama, CN-resident
  SiliconFlow, Groq alongside OpenAI).

## Added

### Rust

- `corlinman_core::config::ProvidersConfig` is now a transparent
  `BTreeMap<String, ProviderEntry>` wrapper exposing `iter` /
  `iter_mut` / `get` / `insert` / `remove` / `enabled_names` /
  `kind_for` (the legacy slot-name kind inference).
- `ProviderKind` gains seven variants: `Mistral`, `Cohere`,
  `Together`, `Groq`, `Replicate`, `Bedrock`, `Azure`. `ProviderKind::all()`
  returns the canonical declaration order so the admin UI's "Add
  provider" modal stays in sync without a parallel list.
- `Config::default()` is hand-written (not derived) and seeds a single
  disabled `[providers.openai]` entry with `kind = "openai"`. A fresh
  `corlinman config init` keeps the documented onboarding flow ("export
  `OPENAI_API_KEY` and you're done") working.
- New cross-field validator: free-form provider names without an
  explicit `kind` field produce a `missing_kind` error pointing at the
  offending entry name and listing every valid kind.

### Python

- `corlinman_providers.specs.ProviderKind` mirrors the Rust enum with
  the seven new variants.
- `corlinman_providers.market_providers` ships thin adapter classes
  (`MistralProvider`, `CohereProvider`, `TogetherProvider`,
  `GroqProvider`, `ReplicateProvider`) that wrap
  `OpenAICompatibleProvider` with documented default base URLs.
- `BedrockProvider` / `AzureProvider` are declared placeholder classes
  whose `build()` raises `NotImplementedError` with a workaround hint
  (use `kind = "openai_compatible"` against a SigV4 proxy / Azure
  deployment URL).

### Docs

- New: [`docs/providers.md`](providers.md) — the working reference for
  the provider model + 14 supported `kind`s + four end-to-end recipes.
- Updated: [`docs/config.example.toml`](config.example.toml) — leads
  with `[providers.openai]` and ships six commented-out recipes
  (Anthropic, Gemini under a friendly name, OpenRouter, SiliconFlow,
  Ollama, Groq); adds two `[embedding]` examples that reference named
  providers; adds two `[models.aliases.*]` examples that pin aliases to
  named entries.
- Updated: [`docs/architecture.md`](architecture.md) §7 — the inline
  config sample now reflects the free-form named-provider shape; the
  reading list points at `docs/providers.md`.
- Updated: [`README.md`](../README.md) — the "Configuration" section
  shows the new `kind = "..."` shape; the documentation map links to
  `docs/providers.md`.

## Migration / breaking changes

- **No data migration is required.** Existing configs with the six
  legacy slot names (`anthropic`, `openai`, `google`, `deepseek`,
  `qwen`, `glm`) continue to load: the validator falls back to
  `ProviderKind::from_slot_name(name)` when an entry has no explicit
  `kind`, so pre-refactor configs round-trip unchanged.
- **New entries MUST set `kind` explicitly.** Any operator-chosen name
  (`siliconflow`, `openrouter`, `ollama-local`, `my-vllm`, …) without
  a `kind` field fails `corlinman config validate` with a
  `missing_kind` error pointing at the offending entry. The error
  message lists every valid kind so the fix is one keystroke.
- **`bedrock` and `azure` raise at runtime today.** Configs that
  declare them parse and validate, but the build-time adapter
  factory raises `NotImplementedError` until real SigV4 /
  deployment-routing lands. Operators who need either today should
  declare `kind = "openai_compatible"` against a compatible proxy.

## Stability

- Rust: `cargo fmt / clippy / test` — all 687+ workspace tests green.
  `docs_example_toml_still_parses` keeps the example file under
  `deny_unknown_fields` enforcement.
- Python: `ruff / mypy / pytest` — 178+ pass.
- `corlinman config validate --path docs/config.example.toml` exits
  zero (one `no_provider_enabled` warning is expected — every entry
  in the sample is `enabled = false` so operators can opt in
  one at a time).

## Upgrade

```bash
git pull
cargo build --release -p corlinman-cli -p corlinman-gateway

# Run validate against your existing config to catch any free-form
# entries that need a kind.
corlinman config validate

# If validate flags `missing_kind`, edit the offending entries to
# add `kind = "..."`. See docs/providers.md §2 for the table of
# valid values.
```
