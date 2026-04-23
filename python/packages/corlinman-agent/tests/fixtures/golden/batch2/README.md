# Batch 2 Context-Assembler Golden Fixtures

This directory contains paired `<name>.in.json` / `<name>.out.json`
snapshots driven by `test_context_assembler.py::test_golden_snapshot`.
Each pair pins the full output of `ContextAssembler.assemble()` for one
representative input, so the pipeline can't drift silently across
refactors.

## Adding a new fixture

1. Write `<NN>_<slug>.in.json` using the schema documented on
   `_run_golden` in `test_context_assembler.py`. At minimum:

   ```json
   {
     "messages": [...],
     "session_key": "sess-NN",
     "model_name": "gpt-4",
     "metadata": {}
   }
   ```

   Optional keys: `agents`, `skills`, `tar`, `var`, `sar`, `env`,
   `placeholder_subs`, `single_agent_gate`.

2. Append `"NN_<slug>"` to the `@pytest.mark.parametrize` list in
   `test_golden_snapshot`.

3. Run the tests once:

   ```
   uv run pytest python/packages/corlinman-agent/tests/test_context_assembler.py \
     -k golden -v
   ```

   The harness detects the missing `.out.json` and writes one on the
   first run, then returns (the pass is a materialisation, not a
   comparison). Re-run to confirm the comparison pass is stable.

4. Review the generated `.out.json` by eye before committing — the
   snapshot IS the spec, so a surprising value means either a real
   assembler change or a fixture bug.

## Regenerating after an intentional pipeline change

Set the `_REGENERATE_GOLDENS` env var to `1`:

```
_REGENERATE_GOLDENS=1 uv run pytest \
  python/packages/corlinman-agent/tests/test_context_assembler.py -k golden
```

The writer uses `sort_keys=True` + `ensure_ascii=False`, so a second
run produces byte-identical output (the regen is idempotent; check with
`git status` — it should be clean on the second regen if only
whitespace/ordering was in play).

Never commit with `_REGENERATE_GOLDENS=True` hardcoded in the module; the
env-var gate exists so the default (`False`) is what CI sees.

## Determinism constraints

Fixture outputs must not contain values that change across runs. In
particular:

- Do **not** reference `{{TimeVar}}` in an input: the fixed resolver
  returns `datetime.now(UTC).isoformat()`, so the golden will drift
  every second. `{{Date}}` changes every day for the same reason —
  don't use it either unless you can pin the clock from the test.
- Pin any env var your fixture reads (`Var*` / `SarModel*`) via the
  `env` block on the input JSON. The test harness applies those with
  `monkeypatch.setenv`, so they are scoped to one run and do not leak.
- File-backed tiers (`tar` / `var` / `sar`) are seeded from the input
  JSON into a fresh `tmp_path`. There is no shared on-disk state.
- The placeholder client is stubbed; set `placeholder_subs` to the
  substitutions the stub should apply.
- Chinese / CJK agent names are supported (see `07_chinese_agent_name`);
  the registry expects the yaml filename stem to match the `name:` field
  exactly.
