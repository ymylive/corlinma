//! B3 cross-workstream contract test: byte-identical off-path baseline.
//!
//! Contract being pinned: when `HybridParams::boost` is `None` the query
//! path must be byte-identical regardless of whether the v6 `chunk_epa`
//! table is empty or fully populated. Put differently: a fully-backfilled
//! corpus must NOT perturb queries that leave EPA disabled — that would
//! break every legacy caller that hasn't opted in to B3-BE5.
//!
//! This is the dual of `epa_subtree_composition.rs`: there we prove the
//! boost *does* change results when enabled; here we prove the storage
//! presence *doesn't* change results when disabled. Both contracts are
//! necessary to keep B3-BE5 safely opt-in.

use std::sync::Arc;

use corlinman_vector::{
    hybrid::{HybridParams, HybridSearcher, RagHit},
    SqliteStore, UsearchIndex,
};
use tempfile::TempDir;
use tokio::sync::RwLock;

/// Seed a v6 store with a small, deterministic corpus. No EPA rows yet —
/// callers choose when to populate them.
async fn seed_no_epa() -> (HybridSearcher, Arc<SqliteStore>, Vec<i64>, TempDir) {
    let tmp = TempDir::new().unwrap();
    let sqlite = Arc::new(
        SqliteStore::open(&tmp.path().join("kb.sqlite"))
            .await
            .unwrap(),
    );
    let file_id = sqlite
        .insert_file("off.md", "notes", "h", 0, 0)
        .await
        .unwrap();

    let corpus: &[(&str, [f32; 4], &str)] = &[
        ("banana apple cherry", [1.00, 0.00, 0.00, 0.0], "topic.rust"),
        ("banana dog elephant", [0.97, 0.03, 0.00, 0.0], "topic.rust"),
        (
            "banana grape honey",
            [0.94, 0.00, 0.06, 0.0],
            "topic.python",
        ),
        ("banana ice juice", [0.91, 0.05, 0.04, 0.0], "topic.python"),
        ("banana kiwi lemon", [0.88, 0.08, 0.00, 0.0], "topic.rust"),
    ];
    let mut ids: Vec<i64> = Vec::with_capacity(corpus.len());
    let mut index = UsearchIndex::create_with_capacity(4, 32).unwrap();
    for (i, (text, v, path)) in corpus.iter().enumerate() {
        let id = sqlite
            .insert_chunk(file_id, i as i64, text, Some(v), "general")
            .await
            .unwrap();
        ids.push(id);
        sqlite.attach_chunk_to_tag_path(id, path).await.unwrap();
        index.add(id as u64, v).unwrap();
    }

    let searcher = HybridSearcher::new(
        sqlite.clone(),
        Arc::new(RwLock::new(index)),
        HybridParams::new(),
    );
    (searcher, sqlite, ids, tmp)
}

fn off_path_params() -> HybridParams {
    HybridParams {
        top_k: 10,
        overfetch_multiplier: 3,
        bm25_weight: 1.0,
        hnsw_weight: 1.0,
        rrf_k: 60.0,
        tag_filter: None,
        namespaces: None,
        rerank_enabled: false,
        tag_subtree: None,
        // The critical knob for this test: explicitly off.
        boost: None,
    }
}

fn hits_byte_equal(a: &[RagHit], b: &[RagHit]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).all(|(x, y)| {
        x.chunk_id == y.chunk_id
            && x.file_id == y.file_id
            && x.content == y.content
            && x.source == y.source
            && x.path == y.path
            // Scores are f32; compare bits so a drifting NaN (pathologically)
            // would fail rather than silently paper over.
            && x.score.to_bits() == y.score.to_bits()
    })
}

/// Contract A: repeated off-path queries are byte-identical. This catches
/// hidden non-determinism (HashMap iteration, timestamp-dependent scoring,
/// etc) more than anything else — if this ever starts flaking, the RRF
/// path has grown a source of nondeterminism.
#[tokio::test]
async fn repeated_offpath_queries_are_byte_identical() {
    let (searcher, _sqlite, _ids, _tmp) = seed_no_epa().await;
    let first = searcher
        .search("banana", &[1.0, 0.0, 0.0, 0.0], Some(off_path_params()))
        .await
        .unwrap();
    let second = searcher
        .search("banana", &[1.0, 0.0, 0.0, 0.0], Some(off_path_params()))
        .await
        .unwrap();
    assert!(
        hits_byte_equal(&first, &second),
        "off-path query is nondeterministic\nfirst:  {first:?}\nsecond: {second:?}"
    );
}

/// Contract B: populating `chunk_epa` MUST NOT perturb an off-path query.
/// This is the real B3-BE5 opt-in guarantee — legacy callers who don't
/// set `boost` must see pre-B3 behaviour even on a fully-backfilled DB.
#[tokio::test]
async fn populating_chunk_epa_does_not_perturb_offpath_queries() {
    let (searcher, sqlite, ids, _tmp) = seed_no_epa().await;

    // Capture the baseline with chunk_epa EMPTY.
    let baseline = searcher
        .search("banana", &[1.0, 0.0, 0.0, 0.0], Some(off_path_params()))
        .await
        .unwrap();

    // Now populate chunk_epa for every chunk with wildly varying values.
    // If the off-path query accidentally consulted the cache, the score
    // distribution would shift. A spread of 0.05..0.95 logic_depth
    // guarantees different candidates would get different boost factors
    // *if* the boost were active.
    for (i, id) in ids.iter().enumerate() {
        let ld = 0.05 + (i as f32) * 0.20; // 0.05, 0.25, 0.45, 0.65, 0.85
        let entropy = 1.0 - ld;
        sqlite
            .upsert_chunk_epa(*id, &[0.3_f32, 0.4, 0.5], entropy, ld)
            .await
            .unwrap();
    }

    // Re-run the SAME off-path query. Results must be byte-identical to
    // the baseline captured against an empty chunk_epa table.
    let after = searcher
        .search("banana", &[1.0, 0.0, 0.0, 0.0], Some(off_path_params()))
        .await
        .unwrap();

    assert!(
        hits_byte_equal(&baseline, &after),
        "chunk_epa presence perturbed an off-path query\n\
         baseline: {baseline:?}\n\
         after:    {after:?}"
    );
}

/// Contract C: even with `chunk_epa` populated AND a subtree filter
/// active, the off-path query (no boost) must remain byte-identical
/// between two runs. Pins the composition of subtree-filter + no-boost.
#[tokio::test]
async fn subtree_filtered_offpath_is_byte_identical_with_or_without_epa_rows() {
    let (searcher, sqlite, ids, _tmp) = seed_no_epa().await;
    let mut p = off_path_params();
    p.tag_subtree = Some("topic.rust".to_string());

    let before = searcher
        .search("banana", &[1.0, 0.0, 0.0, 0.0], Some(p.clone()))
        .await
        .unwrap();

    for (i, id) in ids.iter().enumerate() {
        let ld = if i % 2 == 0 { 0.1_f32 } else { 0.9_f32 };
        sqlite
            .upsert_chunk_epa(*id, &[0.3_f32, 0.4], 1.0 - ld, ld)
            .await
            .unwrap();
    }

    let after = searcher
        .search("banana", &[1.0, 0.0, 0.0, 0.0], Some(p))
        .await
        .unwrap();

    assert!(
        hits_byte_equal(&before, &after),
        "subtree-filtered off-path query diverged after chunk_epa populated\n\
         before: {before:?}\n\
         after:  {after:?}"
    );
}
