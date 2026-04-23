# TVStxt/fixed/

This directory is a reserved hook for future **on-disk fixed variables** —
user-authored files that override the built-in Python resolvers in
`corlinman_agent.variables.fixed.FixedRegistry`.

Today, **fixed values are registered programmatically** (e.g. `TimeVar`,
`Date`) and the built-in registry always wins over anything placed here.
Files in this directory are watched by the hot-reload watcher but are
not yet consulted during resolution.

If you drop `<Key>.txt` here today, the cascade will ignore it. This
placeholder only exists so that:

1. `docs/config.example.toml`'s `fixed_dir = "TVStxt/fixed"` points at
   a real directory.
2. The hot-reload watcher has a valid path to bind.

When the on-disk fixed tier lands (tracked on the roadmap), this README
will be replaced with authoring guidance.
