# corlinman-persona

Phase 3 W3-C — persona persistence across sessions.

`agent_state.sqlite` stores per-agent runtime state (`mood`, `fatigue`,
`recent_topics`, plus a free-form `state_json` extension point) so it
survives restarts and session boundaries.

## Surfaces

- `PersonaStore` — async sqlite wrapper (`store.py`).
- `PersonaState` — dataclass projection (`state.py`).
- `apply_decay` — pure-function fatigue / topic ageing (`decay.py`).
- `seed_from_card` — first-sight YAML seeder; never overwrites existing rows
  (`seeder.py`).
- `PersonaResolver` — read-only `{{persona.*}}` placeholder lookup
  (`placeholders.py`).
- `corlinman-persona` CLI — `decay-once` / `show` / `reset` subcommands
  (`cli.py`).

## Mutation policy

The seeder is the only writer outside the EvolutionLoop. It only inserts
when an agent has no row; existing rows are left untouched. All other
state changes flow through the EvolutionLoop's `agent_card` kind (Phase 4).
The hourly `decay-once` job is a deterministic, additive recovery
function — not a free-form mutation.
