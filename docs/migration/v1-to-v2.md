# Migrating from v0.1.x to v0.2.x

For engineers who deployed **v0.1.2** (or earlier) and want to upgrade.

## 1. Summary

v0.2 is backwards-compatible on every touched contract: v1 plugin
manifests load unchanged (auto-migrate to v2 in memory), v5 vector
DBs open unchanged (migrate idempotently to v6 on first open), and
v0.1 `config.toml` files parse unchanged (every new section defaults).
The visible deltas are a richer manifest (`protocols` / `hooks` /
`skill_refs`), hierarchical tags + EPA cache in the vector store,
four new on-disk authoring surfaces (`skills/`, `agents/`, `TVStxt/*`),
eight reserved placeholder namespaces, and an optional block-shaped
tool-call protocol alongside OpenAI function-calling.

## 2. Backend changes

### 2.1 Manifest v1 → v2

Three new fields: `protocols`, `hooks`, `skill_refs` (plus the
discriminator `manifest_version`). v1 files parse unchanged; absent
`manifest_version` is stamped to `1` and migrated in memory to `2`
with default `protocols = ["openai_function"]`.

v1 (still valid):

```toml
name = "echo"
version = "1.0.0"
plugin_type = "sync"
entry_point = { command = "python", args = ["echo.py"] }
```

v2 (new fields surfaced):

```toml
manifest_version = 2
name = "echo"
version = "1.0.0"
plugin_type = "sync"
entry_point = { command = "python", args = ["echo.py"] }
protocols  = ["openai_function", "block"]
hooks      = ["message.received", "session.patch"]
skill_refs = ["skill.core"]
```

Unknown `protocols` are rejected at load; unknown `hooks` entries
warn but don't fail (forward-compat).

### 2.2 Vector schema v5 → v6

`V5ToV6TagNodesAndEpa` runs inside a transaction on first open:
creates `tag_nodes` (hierarchical tag tree: `id / parent_id / name
/ path / depth`) and `chunk_epa` (EPA projection cache), retargets
`chunk_tags.tag_node_id` onto `tag_nodes.id` (flat v5 tags
materialise as depth-0 nodes so legacy queries keep working), and
bumps `kv_store('schema_version') = 6`. Idempotent; already-v6 DBs
are no-ops. `schema_version > 6` is refused with a forward-compat
guard.

### 2.3 Config additions

Every new section is `#[serde(default)]` — existing `config.toml`
loads with no edits. Additive diff against a v0.1.2 config:

```toml
[hooks]            capacity = 1024, enabled = true
[skills]           dir = "skills", autoload = true
[variables]        tar_dir / var_dir / sar_dir / fixed_dir + hot_reload
[agents]           dir = "agents", single_agent_gate = true
[tools.block]      enabled = false, fallback_to_function_call = true
[telegram.webhook] public_url = "", secret_token = "", drop_updates_on_reconnect = false
[vector.tags]      hierarchy_enabled = false, max_depth = 6
[wstool]           bind = "127.0.0.1:18790", auth_token = "", heartbeat_secs = 15
[canvas]           host_endpoint_enabled = false, session_ttl_secs = 1800
[nodebridge]       listen = "127.0.0.1:18788", accept_unsigned = false
```

Fully annotated shapes live in
[`docs/config.example.toml`](../config.example.toml).

## 3. Filesystem additions

Paths are relative to `[server].data_dir` (default `~/.corlinman`).

| Directory | File type | Purpose |
|-----------|-----------|---------|
| `skills/` | `*.md` + YAML frontmatter | openclaw-style reusable capabilities. |
| `agents/` | `*.yaml` | Character cards (`{{角色}}` targets). Filename stem must match `name:`. |
| `TVStxt/tar/` | `*.txt` | Top-tier cascade (`CurrentProject.txt → {{TarCurrentProject}}`). |
| `TVStxt/var/` | `*.txt` | Env-indexed (`{{VarX}}` reads `os.environ["VarX"]`). |
| `TVStxt/sar/` | `*.txt` | Model-gated (`{{SarPromptN}}` iff current model matches `os.environ["SarModelN"]`). |
| `TVStxt/fixed/` | reserved | Fixed tier; `{{TimeVar}}` / `{{Date}}` serve it in-process today. |

Sample files ship in-repo
(`skills/{web_search,code_review,memory}.md`,
`agents/{mentor,researcher,editor}.yaml`,
`TVStxt/tar/{CurrentProject,HouseStyle}.txt`,
`TVStxt/sar/SarPrompt1.txt`). Authoring rules and loader invariants
live in
[`docs/guides/skills-and-agents.md`](../guides/skills-and-agents.md) —
not duplicated here.

## 4. Placeholder syntax changes

Eight reserved namespaces; `{{namespace.key}}` is the strict form,
bare `{{X}}` still routes by tier-prefix (`Tar…`, `Var…`, `Sar…`).

| Namespace | Routes to |
|-----------|-----------|
| `var` / `sar` / `tar` | TVStxt tiers (env-indexed / model-gated / always). |
| `agent` | Expanding agent card's `variables.*`. |
| `session` | Per-session runtime KV. |
| `tool` / `vector` / `skill` | Tool catalog / RAG hits / skill registry. |

**`{{角色}}` expansion** — a bare token matching an agent card name
expands to that card's `system_prompt`. With
`[agents].single_agent_gate = true` (default) the first card wins;
subsequent card references in the same prompt are left literal.

**Cycle detection** — the engine tracks an in-flight key set and
aborts on self-reference with `PlaceholderError::Cycle`. Unknown
placeholders are left literal so authoring typos surface to the
model.

## 5. Protocol additions

Dual-track tool invocation:

- **OpenAI function-calling** (default) — unchanged. Standard
  `tool_calls` JSON dispatched through the existing plugin runtime.
- **Block protocol** (opt-in) — agents emit
  `<<<[TOOL_REQUEST]>>> … <<<[END_TOOL_REQUEST]>>>` blocks with
  `「始」…「末」` value fencing. The gateway parses, schema-validates,
  and dispatches through the same runtime.

**Opt-in**: list `"block"` in the manifest's `protocols` **and** set
`[tools.block].enabled = true`. `fallback_to_function_call = true`
keeps legacy plugins reachable from block-aware agents.

**Gradual rollout**: (1) ship a block-aware agent alongside the
legacy one with block disabled globally; (2) enable `[tools.block]`
and restrict `protocols` per-agent; verify in the admin UI's
`/playground/protocol` page; (3) expand one agent at a time. The
`protocol_matrix` integration tests
(`rust/crates/corlinman-integration-tests/tests/protocol_matrix.rs`)
exercise both tracks against the same plugin set.

## 6. Step-by-step upgrade

```bash
# 1. Back up existing data (config + sessions + vector DB + logs).
cp -r ~/.corlinman ~/.corlinman.backup-v1

# 2. Update binaries. Native build:
./scripts/dev-setup.sh
cargo build --release -p corlinman-gateway -p corlinman-cli
uv sync --frozen
pnpm -C ui install && pnpm -C ui build

# (Docker: rebuild the image from the 0.2.x tag.)

# 3. Validate the existing config — new sections have defaults so this
#    should be green without edits.
./target/release/corlinman config validate

# 4. First run — the vector DB migrates v5 → v6 automatically on open.
#    Plugin manifests with no manifest_version are accepted as v1 and
#    migrated in memory to v2.
./target/release/corlinman dev
```

Smoke checks after the first run:

- `GET /health` returns `200` and reports the gateway + Python agent
  both healthy.
- `corlinman doctor` stays green (all 20+ checks pass).
- In the admin UI, `/skills`, `/characters`, `/hooks`,
  `/playground/protocol`, and `/channels/telegram` all render.

## 7. Rollback

**There is no down-path shipped for the v5→v6 vector migration.** If
v6 misbehaves, stop the gateway gracefully (SIGTERM, never kill -9
mid-migration), swap the data-dir backup back into place, and deploy
the prior binary or image:

```bash
systemctl stop corlinman            # or: docker compose down
mv ~/.corlinman ~/.corlinman.failed-v2
mv ~/.corlinman.backup-v1 ~/.corlinman
./target/release-v0.1.x/corlinman dev
```

v1 manifests and v0.1 configs still parse under v0.2 unchanged, so
config rollback needs no file edits.

## 8. Troubleshooting

1. **`manifest_version 99 is not supported (this gateway supports 1..=2)`** — a
   plugin ships a newer manifest than this gateway. Upgrade the
   gateway or pin the plugin to a compatible release.
2. **`schema_version=7 is newer than registry target=6`** — the DB
   was touched by a newer corlinman. Restore the backup or upgrade
   the gateway before reopening.
3. **Agent card expansion silently skipped.** `single_agent_gate`
   let the first card win. Check `RUST_LOG=corlinman_core::placeholder=debug`
   for the substitution trace.
4. **Block-protocol tool call ignored.** Either `[tools.block].enabled`
   is `false`, or the plugin's `protocols` list doesn't include
   `"block"`. Confirm via `/admin/plugins` and the manifest diff.
5. **`{{VarX}}` stays literal in the prompt.** Expected when the env
   var `VarX` is unset or points at a file that does not exist in
   `TVStxt/var/`. The engine leaves unknowns literal so authoring
   bugs are visible; set the env var or adjust the stem.
