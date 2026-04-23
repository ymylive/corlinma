# Batch 3 cross-workstream contracts

Observable behaviours pinned by the B3 test suite. Breaking any of these
requires a matching update here and to the owning test.

Tests:
- `rust/crates/corlinman-integration-tests/tests/protocol_matrix.rs`
- `rust/crates/corlinman-integration-tests/tests/epa_subtree_composition.rs`
- `rust/crates/corlinman-integration-tests/tests/v6_epa_offpath_identical.rs`
- `python/packages/corlinman-agent/tests/test_backfill_then_boost.py`

## 1. Dispatcher protocol coercion equivalence (B3-BE1/BE2)

Same logical tool invocation via block envelope vs. OpenAI function-call
must produce byte-identical coerced `.args` `serde_json::Value`s. Matrix
covers string / integer / boolean / object / array / CJK / trailing
whitespace (numeric types only — `"string"` is verbatim) / multi-arg.
Schema-driven coercion (`block::coerce_args`) is the single source of
truth.

## 2. v6 off-path byte-identity (B3-BE3/BE5)

With `HybridParams::boost == None`, a query against a v6 DB returns
byte-identical `Vec<RagHit>` regardless of whether `chunk_epa` is empty
or fully populated. `chunk_epa` presence MUST NOT perturb off-path
queries. Two successive off-path queries with identical params are also
byte-identical (no hidden RRF nondeterminism).

## 3. EPA backfill idempotency (B3-BE4)

Re-running `EpaBackfiller.run()` on an unchanged corpus yields the same
row count and byte-identical `chunk_epa` rows. KMeans is seeded, so the
fitted basis is reproducible.

Row shape (consumed by `SqliteStore::get_chunk_epa`):
- `projections` — little-endian `f32[]`, length `> 0` and `% 4 == 0`.
- `entropy`, `logic_depth` — finite, in `[0, 1]`, sum to `1` (±1e-6).

## 4. Subtree + boost composition (B3-BE3 + B3-BE5)

With both `tag_subtree` and `boost` set, subtree filter applies BEFORE
the boost. Out-of-subtree chunks cannot leak in even when their
`logic_depth` would earn a large boost factor. Within the retained pool,
higher `logic_depth` outranks lower when baseline RRF signals are
comparable — which implies the boosted ordering differs from the
baseline ordering.
