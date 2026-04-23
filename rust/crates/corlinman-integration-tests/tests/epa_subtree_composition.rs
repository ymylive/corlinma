//! B3 cross-workstream contract test: tag-subtree filter + EPA boost compose.
//!
//! Pins the contract that
//! (1) `HybridParams::tag_subtree` restricts the candidate pool BEFORE
//!     the candidate boost is applied, and
//! (2) `EpaBoost` reranks within that pool by `logic_depth` — higher
//!     logic_depth must yield a higher final score than a competitor with
//!     lower logic_depth when all other baseline signals are comparable.
//!
//! This composition lives across B3-BE3 (schema v6 subtree filter),
//! B3-BE4 (EPA basis + projections), and B3-BE5 (CandidateBoost hook),
//! which is why the assertion belongs in the integration-tests crate
//! rather than any one owning crate.

use std::sync::Arc;

use corlinman_vector::{
    hybrid::{CandidateBoost, EpaBoost, HybridParams, HybridSearcher},
    SqliteStore, UsearchIndex,
};
use tempfile::TempDir;
use tokio::sync::RwLock;

/// Build a 4-d vector biased along one axis. The larger `bias`, the
/// closer to the canonical query vector `[1,0,0,0]` — we use this to
/// shape a deterministic baseline ranking.
fn vec4(bias: f32, noise: f32) -> [f32; 4] {
    [bias, noise, 0.0, 0.0]
}

/// Seed a v6 SqliteStore with 20 chunks and matching HNSW index:
///   - ids[0..10] tagged `topic.rust`
///   - ids[10..20] tagged `topic.python`
///
/// Baseline ranking is controlled by the `bias` column: the smaller the
/// index within each subtree, the closer its vector is to the query,
/// therefore the higher it ranks pre-boost.
///
/// EPA rows are seeded so that *within each subtree*, the bottom-half
/// (best baseline rank) carries `logic_depth=0.3` and the top-half
/// (worst baseline rank) carries `logic_depth=0.9`. This guarantees the
/// baseline and EPA-expected orderings disagree — making the boost's
/// effect observable.
async fn seed() -> (HybridSearcher, Arc<SqliteStore>, Vec<i64>, TempDir) {
    let tmp = TempDir::new().unwrap();
    let sqlite = Arc::new(
        SqliteStore::open(&tmp.path().join("kb.sqlite"))
            .await
            .expect("open v6 sqlite"),
    );
    let file_id = sqlite
        .insert_file("kb.md", "notes", "h", 0, 0)
        .await
        .unwrap();

    // `ensure_tag_path` is idempotent; we still call it explicitly here so
    // the contract that `topic.rust` / `topic.python` exist as tag_nodes
    // prior to attachment is visible in the test log.
    sqlite.ensure_tag_path("topic.rust").await.unwrap();
    sqlite.ensure_tag_path("topic.python").await.unwrap();

    let mut index = UsearchIndex::create_with_capacity(4, 64).unwrap();
    let mut ids: Vec<i64> = Vec::with_capacity(20);

    // 20 chunks: 10 rust + 10 python. Content is identical up to index so
    // BM25 scores cannot fully dominate the ranking — the dense path is
    // what differentiates rows.
    for i in 0..20_usize {
        // Bias: decreases monotonically 0.99, 0.98, …, 0.80 so chunk[0] is
        // the best vector match for the query [1,0,0,0] across the whole
        // corpus. This makes the baseline order deterministic.
        let bias = 0.99 - 0.01 * (i as f32);
        let v = vec4(bias, 0.01);
        let content = format!("banana rust python chunk number {i}");
        let id = sqlite
            .insert_chunk(file_id, i as i64, &content, Some(&v), "general")
            .await
            .unwrap();
        ids.push(id);
        index.add(id as u64, &v).unwrap();

        let path = if i < 10 { "topic.rust" } else { "topic.python" };
        sqlite.attach_chunk_to_tag_path(id, path).await.unwrap();

        // EPA seeding — the key move. Within `topic.rust`, the *first five*
        // (best baseline rank) carry low logic_depth, the *last five* carry
        // high logic_depth. So if the boost works, the subtree ordering
        // after boost will NOT equal the baseline ordering.
        let logic_depth = if i < 10 {
            // rust subtree: i in [0,5) → 0.3; i in [5,10) → 0.9
            if i < 5 {
                0.3_f32
            } else {
                0.9_f32
            }
        } else {
            // python subtree ordering doesn't matter for the subtree test,
            // but seeding keeps the backfill-populated-v6 contract honest.
            if i < 15 {
                0.3_f32
            } else {
                0.9_f32
            }
        };
        let entropy = 1.0 - logic_depth;
        // `projections` values are irrelevant for the boost formula — only
        // logic_depth is consumed by the `EpaBoost::prepare` → dynamic_boost
        // path. We still seed 2 floats to mirror typical backfill output.
        sqlite
            .upsert_chunk_epa(id, &[0.1_f32, 0.2], entropy, logic_depth)
            .await
            .unwrap();
    }

    let searcher = HybridSearcher::new(
        sqlite.clone(),
        Arc::new(RwLock::new(index)),
        HybridParams::new(),
    );
    (searcher, sqlite, ids, tmp)
}

fn params(subtree: Option<&str>, boost: Option<Arc<dyn CandidateBoost>>) -> HybridParams {
    HybridParams {
        top_k: 10,
        overfetch_multiplier: 4,
        bm25_weight: 1.0,
        hnsw_weight: 1.0,
        rrf_k: 60.0,
        tag_filter: None,
        namespaces: None,
        rerank_enabled: false,
        tag_subtree: subtree.map(|s| s.to_string()),
        boost,
    }
}

/// Contract: subtree filter runs BEFORE the boost, so the returned hit
/// set contains only chunks inside the subtree regardless of whether
/// EPA boosting is enabled.
#[tokio::test]
async fn subtree_filter_runs_before_boost_and_restricts_candidate_pool() {
    let (searcher, sqlite, rust_and_python_ids, _tmp) = seed().await;
    let rust_ids: std::collections::HashSet<i64> =
        rust_and_python_ids[..10].iter().copied().collect();

    let booster: Arc<dyn CandidateBoost> = Arc::new(EpaBoost::new(sqlite.clone(), 1.0, (0.5, 2.5)));

    let hits = searcher
        .search(
            "banana",
            &[1.0, 0.0, 0.0, 0.0],
            Some(params(Some("topic.rust"), Some(booster))),
        )
        .await
        .unwrap();

    assert!(!hits.is_empty(), "subtree query should return some hits");
    assert_eq!(
        hits.len(),
        10,
        "expected all 10 rust chunks to survive the subtree filter; got {}",
        hits.len()
    );
    for h in &hits {
        assert!(
            rust_ids.contains(&h.chunk_id),
            "python chunk {} leaked through the subtree filter",
            h.chunk_id
        );
    }
}

/// Contract: EPA boost reranks within the subtree so that chunks with
/// `logic_depth=0.9` outrank chunks with `logic_depth=0.3`. Equivalently,
/// the boosted ordering must NOT equal the baseline ordering — proving
/// the boost hook actually influenced the top_k.
#[tokio::test]
async fn epa_boost_reranks_inside_subtree_relative_to_baseline() {
    let (searcher, sqlite, _ids, _tmp) = seed().await;
    let baseline = searcher
        .search(
            "banana",
            &[1.0, 0.0, 0.0, 0.0],
            Some(params(Some("topic.rust"), None)),
        )
        .await
        .unwrap();
    assert_eq!(baseline.len(), 10);
    // Because the bias decreases by chunk index, baseline[0] must be the
    // lowest-index rust chunk (id 1) — i.e. the low-logic_depth half.
    let first_baseline_id = baseline[0].chunk_id;

    let booster: Arc<dyn CandidateBoost> = Arc::new(EpaBoost::new(sqlite.clone(), 1.0, (0.5, 2.5)));
    let boosted = searcher
        .search(
            "banana",
            &[1.0, 0.0, 0.0, 0.0],
            Some(params(Some("topic.rust"), Some(booster))),
        )
        .await
        .unwrap();
    assert_eq!(
        boosted.len(),
        10,
        "boost must not change the candidate pool size"
    );

    // --- assertion 1: high-logic_depth half must dominate the boosted top.
    // The first five chunks (logic_depth=0.3) must not appear at the very
    // top anymore; the last five (logic_depth=0.9) should. We check the
    // top-5 of the boosted list are exactly the high-ld half of the rust
    // subtree, in some order.
    let boosted_top5: std::collections::HashSet<i64> =
        boosted.iter().take(5).map(|h| h.chunk_id).collect();
    // Under seed(), the high-logic_depth rust ids are the 6th through 10th
    // inserted — whatever autoincrement gave them. We recover that by
    // filtering baseline (which is ordered by baseline score) and taking
    // chunks whose logic_depth row says 0.9.
    let mut expected_high_ld: Vec<i64> = Vec::with_capacity(5);
    for h in &baseline {
        let epa = sqlite.get_chunk_epa(h.chunk_id).await.unwrap().unwrap();
        if (epa.logic_depth - 0.9).abs() < 1e-6 {
            expected_high_ld.push(h.chunk_id);
        }
    }
    let expected_set: std::collections::HashSet<i64> = expected_high_ld.iter().copied().collect();
    assert_eq!(
        boosted_top5, expected_set,
        "top-5 after boost should be the five high-logic_depth rust chunks"
    );

    // --- assertion 2: the boosted ordering must differ from baseline.
    // If logic_depth were ignored, baseline and boosted would be identical.
    let baseline_order: Vec<i64> = baseline.iter().map(|h| h.chunk_id).collect();
    let boosted_order: Vec<i64> = boosted.iter().map(|h| h.chunk_id).collect();
    assert_ne!(
        baseline_order, boosted_order,
        "EPA boost did not change the ranking — the hook may be disconnected"
    );
    // And the first element must have moved: baseline's top was a low-ld
    // chunk by construction; boosted's top must be a high-ld chunk.
    assert_ne!(
        boosted[0].chunk_id, first_baseline_id,
        "boosted top is still baseline top — boost had no effect"
    );
}
