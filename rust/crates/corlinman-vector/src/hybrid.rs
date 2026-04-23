//! Hybrid HNSW + BM25 retrieval with reciprocal-rank-fusion.
//!
//! # Strategy
//!
//! For each query we run two recall paths in parallel:
//!
//! 1. **Dense (HNSW)** via [`crate::usearch_index::UsearchIndex::search`],
//!    metric = cosine.
//! 2. **Sparse (BM25)** via [`crate::sqlite::SqliteStore::search_bm25`],
//!    using the FTS5 `bm25()` ranker.
//!
//! Each path is queried for `top_k * overfetch_multiplier` candidates so
//! RRF has enough signal to re-rank. We then merge with
//!
//! ```text
//!   rrf_score(doc) = Σ_r  weight_r / (rrf_k + rank_r(doc))
//! ```
//!
//! over rankers `r ∈ {dense, sparse}`. Documents missing from one path
//! contribute `0` from that path — there is no implicit last-rank
//! penalty. The final list is sorted by fused score (descending) and
//! truncated to `top_k`.
//!
//! # Cross-encoder rerank
//!
//! A post-RRF rerank stage is pluggable via [`crate::rerank::Reranker`].
//! The searcher holds an `Arc<dyn Reranker>` (default
//! [`crate::rerank::NoopReranker`]) and — when `params.rerank_enabled` is
//! `true` — hands the fused hits to it before truncating to `top_k`.
//! Sprint 3 T6 shipped the trait + a noop default + a
//! [`crate::rerank::GrpcReranker`] stub; the real client lives in the
//! Python embedding service (see `corlinman_embedding.rerank_client`).
//!
//! # Not yet implemented
//!
//! - LRU unload of `.usearch` files on idle timeout.
//! - `Rerank` gRPC RPC in `proto/embedding.proto` (the stub in
//!   [`crate::rerank::GrpcReranker`] currently returns
//!   `CorlinmanError::Internal("unimplemented: ...")`).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use corlinman_core::metrics::VECTOR_QUERY_DURATION;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::rerank::{NoopReranker, Reranker};
use crate::sqlite::SqliteStore;
use crate::usearch_index::UsearchIndex;

/// Tag-filter predicate pushed down to both recall paths (Sprint 3 T4).
///
/// Semantics (all conditions conjoined):
/// - `required`: chunk must carry *every* tag in the list.
/// - `any_of`: chunk must carry *at least one* tag (ignored when empty).
/// - `excluded`: chunk must carry *none* of the listed tags.
///
/// All tag strings match against `tag_nodes.path`, so a flat v5-style
/// tag like `"rust"` is interpreted as the depth-0 path `"rust"` —
/// backward-compatible with callers that predate the hierarchical tree.
///
/// Subtree filtering (Sprint 9 T-B3-BE3) is expressed via
/// [`HybridParams::tag_subtree`] rather than on this struct so downstream
/// callers that still build `TagFilter { required, excluded, any_of }`
/// positionally keep compiling.
///
/// An all-empty `TagFilter` is equivalent to `None` — callers should not
/// build one in that case (the searcher still short-circuits correctly
/// if they do, it's just wasted work).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TagFilter {
    pub required: Vec<String>,
    pub excluded: Vec<String>,
    pub any_of: Vec<String>,
}

impl TagFilter {
    /// `true` when every constraint list is empty.
    pub fn is_empty(&self) -> bool {
        self.required.is_empty() && self.excluded.is_empty() && self.any_of.is_empty()
    }
}

/// Sprint 9 T-B3-BE5: post-RRF candidate reweighter.
///
/// An implementation returns a multiplicative factor in `(0, ∞)` for a
/// given `chunk_id`; the hybrid searcher multiplies the chunk's RRF
/// score by this factor before the final sort + truncate. `1.0` means
/// "no change" and is what implementations must return for chunks they
/// don't recognise — that way sparse coverage (e.g. EPA rows only exist
/// for some chunks) degrades cleanly rather than zeroing out recall.
///
/// The trait has two entry points:
/// - `prepare(ids)` runs once per query before RRF, async. Default =
///   no-op. Implementations that need to touch I/O (SQLite, HTTP) warm
///   a per-call cache here so [`Self::boost`] can stay sync.
/// - `boost(chunk_id)` runs inside fusion, sync. Must return `1.0` for
///   any id the implementation doesn't recognise.
#[async_trait::async_trait]
pub trait CandidateBoost: Send + Sync + std::fmt::Debug {
    /// Optional async prefetch hook. The searcher calls this with the
    /// union of dense + sparse candidate ids before invoking
    /// [`Self::boost`]. Errors propagate up through `search`.
    async fn prepare(&self, _chunk_ids: &[i64]) -> Result<()> {
        Ok(())
    }

    /// Multiplicative factor applied to the candidate's fused score.
    fn boost(&self, chunk_id: i64) -> f32;
}

/// Pure-Rust port of the Python `dynamic_boost` formula in
/// [`corlinman_tagmemo.boost.dynamic_boost`]. Kept in sync so the Python
/// side isn't in the per-query hot path.
///
/// All three free parameters are clamped to `[0, 1]` before being fed
/// into the formula, matching the Python implementation exactly. The
/// final value is then clamped to `boost_range`.
pub(crate) fn dynamic_boost_rust(
    logic_depth: f32,
    resonance_boost: f32,
    entropy_penalty: f32,
    base_tag_boost: f32,
    boost_range: (f32, f32),
) -> f32 {
    let ld = logic_depth.clamp(0.0, 1.0);
    let rb = resonance_boost.clamp(0.0, 1.0);
    let ep = entropy_penalty.clamp(0.0, 1.0);
    let denom = 1.0 + ep * 0.5; // ep ∈ [0,1] ⇒ denom ∈ [1, 1.5], never zero.
    let factor = ld * (1.0 + rb) / denom;
    (base_tag_boost * factor).clamp(boost_range.0, boost_range.1)
}

/// [`CandidateBoost`] that sources its per-chunk signal from the
/// `chunk_epa` cache (populated by B3-BE4 / the Python backfill job).
///
/// Lookups happen inside [`CandidateBoost::prepare`] — async, once per
/// query — and are stashed in an internal cache so the sync
/// [`CandidateBoost::boost`] hot path can read without touching SQLite.
/// Chunks without an EPA row produce a cached `1.0` (pass-through).
pub struct EpaBoost {
    store: Arc<SqliteStore>,
    base_tag_boost: f32,
    boost_range: (f32, f32),
    cache: std::sync::RwLock<HashMap<i64, f32>>,
}

impl EpaBoost {
    /// Construct an `EpaBoost`. `base_tag_boost` is typically `1.0` so
    /// unclamped factors stay near 1×; `boost_range` defaults to
    /// `(0.5, 2.5)` (same as the Python `dynamic_boost` default).
    pub fn new(store: Arc<SqliteStore>, base_tag_boost: f32, boost_range: (f32, f32)) -> Self {
        Self {
            store,
            base_tag_boost,
            boost_range,
            cache: std::sync::RwLock::new(HashMap::new()),
        }
    }
}

impl std::fmt::Debug for EpaBoost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EpaBoost")
            .field("base_tag_boost", &self.base_tag_boost)
            .field("boost_range", &self.boost_range)
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl CandidateBoost for EpaBoost {
    async fn prepare(&self, chunk_ids: &[i64]) -> Result<()> {
        // Drop any stale per-query entries before refilling so the cache
        // doesn't grow unboundedly across calls.
        {
            let mut c = self
                .cache
                .write()
                .map_err(|_| anyhow::anyhow!("EpaBoost cache poisoned"))?;
            c.clear();
        }
        let mut out: HashMap<i64, f32> = HashMap::with_capacity(chunk_ids.len());
        for &id in chunk_ids {
            let factor = match self
                .store
                .get_chunk_epa(id)
                .await
                .with_context(|| format!("EpaBoost::prepare(chunk_id={id})"))?
            {
                Some(row) => dynamic_boost_rust(
                    row.logic_depth,
                    0.0,
                    0.0,
                    self.base_tag_boost,
                    self.boost_range,
                ),
                None => 1.0,
            };
            out.insert(id, factor);
        }
        let mut c = self
            .cache
            .write()
            .map_err(|_| anyhow::anyhow!("EpaBoost cache poisoned"))?;
        *c = out;
        Ok(())
    }

    fn boost(&self, chunk_id: i64) -> f32 {
        // Poisoned lock ⇒ pass through unchanged rather than panic.
        self.cache
            .read()
            .ok()
            .and_then(|c| c.get(&chunk_id).copied())
            .unwrap_or(1.0)
    }
}

/// Reciprocal-rank-fusion tuning knobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridParams {
    /// Final number of fused hits to return.
    pub top_k: usize,
    /// Each recall path is asked for `top_k * overfetch_multiplier`
    /// candidates. `1` disables overfetch.
    pub overfetch_multiplier: usize,
    /// Weight applied to the BM25 (sparse) ranker in the RRF sum.
    pub bm25_weight: f32,
    /// Weight applied to the HNSW (dense) ranker in the RRF sum.
    pub hnsw_weight: f32,
    /// RRF dampening constant `k` (standard literature default = 60).
    pub rrf_k: f32,
    /// Optional tag-filter predicate. `None` ⇒ no filter; see
    /// [`TagFilter`] for semantics. Pushed down into BM25 (SQL `IN` on
    /// the whitelisted ids) and post-filters HNSW (usearch has no
    /// predicate support, so we over-fetch then prune).
    pub tag_filter: Option<TagFilter>,
    /// Sprint 9 T1: restrict the search to one or more `chunks.namespace`
    /// partitions. `None` preserves legacy behaviour → only the
    /// `"general"` namespace is searched. `Some(vec![])` is treated the
    /// same as `None` to keep JSON callers from accidentally killing
    /// recall. Multi-valued vectors union the listed namespaces.
    pub namespaces: Option<Vec<String>>,
    /// Run the [`HybridSearcher`]'s [`Reranker`] after RRF fusion when
    /// `true`. Default: `false`. When `false` the fused list is simply
    /// truncated to `top_k` (the noop reranker behaviour), so callers
    /// who leave this alone see the legacy RRF-only ordering.
    pub rerank_enabled: bool,
    /// Sprint 9 T-B3-BE3: restrict results to chunks tagged anywhere in
    /// the subtree rooted at the given dotted path — `tag_nodes.path = ?`
    /// OR `tag_nodes.path LIKE ? || '.%'`. `None` ⇒ no subtree constraint.
    /// Combines with `tag_filter` (AND) and `namespaces` (AND) when both
    /// are set.
    #[serde(default)]
    pub tag_subtree: Option<String>,
    /// Sprint 9 T-B3-BE5: optional post-RRF candidate reweighter. When
    /// `Some`, the [`CandidateBoost::boost`] factor is multiplied into
    /// every fused candidate's score before truncation. `None` (the
    /// default) ⇒ byte-identical to pre-B3-BE5 behaviour.
    ///
    /// Skipped by serde — pluggable scorers aren't serialisable.
    #[serde(skip)]
    pub boost: Option<Arc<dyn CandidateBoost>>,
}

impl HybridParams {
    /// Library defaults: `top_k=10`, `overfetch=3`, equal weights, `k=60`,
    /// no tag filter, namespace unset (→ `"general"`), rerank disabled,
    /// no subtree constraint, no boost hook.
    pub const fn new() -> Self {
        Self {
            top_k: 10,
            overfetch_multiplier: 3,
            bm25_weight: 1.0,
            hnsw_weight: 1.0,
            rrf_k: 60.0,
            tag_filter: None,
            namespaces: None,
            rerank_enabled: false,
            tag_subtree: None,
            boost: None,
        }
    }
}

impl Default for HybridParams {
    fn default() -> Self {
        Self::new()
    }
}

/// Which recall path(s) surfaced a given hit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum HitSource {
    /// Dense (HNSW) only.
    Dense,
    /// Sparse (BM25) only.
    Sparse,
    /// Both paths returned the chunk — typically the most trustworthy.
    Both,
}

/// One hit emitted by the hybrid searcher.
///
/// `score` is the fused RRF value (larger = better). Pure-path hits
/// returned by [`HybridSearcher::search_dense_only`] /
/// [`HybridSearcher::search_sparse_only`] carry the raw path score
/// instead (cosine-similarity or negated-bm25).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RagHit {
    pub chunk_id: i64,
    pub file_id: i64,
    pub content: String,
    pub score: f32,
    pub source: HitSource,
    pub path: String,
}

/// Owns the two storage backends + default fusion parameters.
///
/// The usearch index sits behind an `RwLock` so index writes (add /
/// save) can proceed without blocking concurrent reads once we wire the
/// indexer in later milestones.
#[derive(Clone)]
pub struct HybridSearcher {
    sqlite: Arc<SqliteStore>,
    usearch: Arc<RwLock<UsearchIndex>>,
    params: HybridParams,
    reranker: Arc<dyn Reranker>,
}

impl HybridSearcher {
    /// Construct a searcher with the provided default `params`. Callers
    /// can still override per-query via [`Self::search`]'s `override_params`.
    ///
    /// The reranker defaults to [`NoopReranker`]. Use
    /// [`Self::with_reranker`] on the returned value to swap it.
    pub fn new(
        sqlite: Arc<SqliteStore>,
        usearch: Arc<RwLock<UsearchIndex>>,
        params: HybridParams,
    ) -> Self {
        Self {
            sqlite,
            usearch,
            params,
            reranker: Arc::new(NoopReranker),
        }
    }

    /// Replace the reranker. Returns `self` for builder-style chaining:
    ///
    /// ```ignore
    /// let searcher = HybridSearcher::new(sqlite, usearch, params)
    ///     .with_reranker(Arc::new(GrpcReranker::new("http://...", "bge-reranker-v2-m3")));
    /// ```
    ///
    /// Only takes effect for queries that also pass
    /// `params.rerank_enabled = true` (per-query override or via the
    /// searcher default).
    #[must_use]
    pub fn with_reranker(mut self, reranker: Arc<dyn Reranker>) -> Self {
        self.reranker = reranker;
        self
    }

    /// Borrow the active reranker (primarily for tests + introspection).
    pub fn reranker(&self) -> &Arc<dyn Reranker> {
        &self.reranker
    }

    /// Default parameters used when a `search` call passes `None`.
    pub fn params(&self) -> HybridParams {
        self.params.clone()
    }

    /// Hybrid search: HNSW + BM25 + RRF fusion.
    ///
    /// `query_text` drives BM25. `query_vector` drives HNSW. Pass an
    /// empty `query_text` to run dense-only implicitly (BM25 returns no
    /// hits and RRF reduces to the HNSW ranking).
    ///
    /// When `override_params.tag_filter` (or the default `params.tag_filter`)
    /// is `Some`, both recall paths are restricted to the intersection
    /// of `chunks.id` with the filter predicate:
    /// - BM25: SQL-level `rowid IN (...)` pushdown
    ///   ([`SqliteStore::search_bm25_with_filter`]).
    /// - HNSW: we over-fetch `fetch` candidates and drop any whose
    ///   `chunk_id` is not on the whitelist; usearch has no predicate
    ///   support so this is the best we can do without paginating.
    ///
    /// Sprint 9 T1: `params.namespaces` further restricts both paths to
    /// chunks whose `namespace` is on the list. `None` (or empty-vec)
    /// defaults to `["general"]` so legacy callers — none of whom set
    /// the field — see the same single-namespace recall they used
    /// before S9. The namespace whitelist intersects with `tag_filter`
    /// when both are set.
    pub async fn search(
        &self,
        query_text: &str,
        query_vector: &[f32],
        override_params: Option<HybridParams>,
    ) -> Result<Vec<RagHit>> {
        let p = override_params.unwrap_or_else(|| self.params.clone());
        if p.top_k == 0 {
            return Ok(Vec::new());
        }
        let fetch = p.top_k.saturating_mul(p.overfetch_multiplier.max(1));

        // --- Tag filter: resolve once, reuse for both paths. ---------------
        let base_tag_ids: Option<Vec<i64>> = match &p.tag_filter {
            Some(tf) if !tf.is_empty() => Some(
                self.sqlite
                    .filter_chunk_ids_by_tags(tf)
                    .await
                    .context("tag filter pushdown")?,
            ),
            _ => None,
        };
        // --- Subtree filter (B3-BE3): `tag_nodes.path = root OR LIKE root.%`.
        // Intersect with the flat tag filter when both are set so callers
        // can still AND them.
        let tag_ids: Option<Vec<i64>> = match (base_tag_ids, p.tag_subtree.as_deref()) {
            (None, None) => None,
            (Some(v), None) => Some(v),
            (None, Some(root)) => Some(
                self.sqlite
                    .filter_chunk_ids_by_tag_subtree(root)
                    .await
                    .context("tag subtree pushdown")?,
            ),
            (Some(tags), Some(root)) => {
                let sub = self
                    .sqlite
                    .filter_chunk_ids_by_tag_subtree(root)
                    .await
                    .context("tag subtree pushdown")?;
                let sub_set: std::collections::HashSet<i64> = sub.into_iter().collect();
                Some(tags.into_iter().filter(|id| sub_set.contains(id)).collect())
            }
        };

        // --- Namespace filter (S9 T1). Default = ["general"] ---------------
        let ns_ids: Vec<i64> = {
            let effective: Vec<String> = match &p.namespaces {
                Some(v) if !v.is_empty() => v.clone(),
                _ => vec!["general".to_string()],
            };
            self.sqlite
                .filter_chunk_ids_by_namespace(&effective)
                .await
                .context("namespace filter pushdown")?
        };

        // Combine namespace + tag filter. Namespace is always active, so
        // `allowed_ids` is always `Some` from S9 onwards. Intersection
        // preserves the stricter of the two when a caller supplies both.
        let allowed_ids: Option<Vec<i64>> = match tag_ids {
            None => Some(ns_ids),
            Some(tags) => {
                let ns_set: std::collections::HashSet<i64> = ns_ids.into_iter().collect();
                Some(tags.into_iter().filter(|id| ns_set.contains(id)).collect())
            }
        };
        let allowed_set: Option<std::collections::HashSet<i64>> =
            allowed_ids.as_ref().map(|v| v.iter().copied().collect());

        // Active filter + empty whitelist ⇒ no chunks match, skip the work.
        if matches!(&allowed_set, Some(s) if s.is_empty()) {
            return Ok(Vec::new());
        }

        // --- Recall path 1: HNSW (dense) -----------------------------------
        // S7.T3: record `corlinman_vector_query_duration_seconds{stage=hnsw}`.
        let hnsw_start = Instant::now();
        let dense_hits: Vec<(i64, f32)> = {
            let idx = self.usearch.read().await;
            if idx.size() == 0 || query_vector.is_empty() {
                Vec::new()
            } else {
                // Over-fetch when tag-filter is active: HNSW can't predicate,
                // so we pull extra and keep the first `fetch` survivors.
                let hnsw_k = if allowed_set.is_some() {
                    fetch.saturating_mul(4).max(fetch)
                } else {
                    fetch
                };
                let raw = idx.search(query_vector, hnsw_k).context("hnsw search")?;
                let mut out: Vec<(i64, f32)> = raw
                    .into_iter()
                    .map(|(k, dist)| (k as i64, 1.0 - dist))
                    .collect();
                if let Some(set) = &allowed_set {
                    out.retain(|(id, _)| set.contains(id));
                    out.truncate(fetch);
                }
                out
            }
        };
        VECTOR_QUERY_DURATION
            .with_label_values(&["hnsw"])
            .observe(hnsw_start.elapsed().as_secs_f64());

        // --- Recall path 2: BM25 (sparse) ----------------------------------
        let bm25_start = Instant::now();
        let sparse_hits: Vec<(i64, f32)> = self
            .sqlite
            .search_bm25_with_filter(query_text, fetch, allowed_ids.as_deref())
            .await
            .context("bm25 search")?;
        VECTOR_QUERY_DURATION
            .with_label_values(&["bm25"])
            .observe(bm25_start.elapsed().as_secs_f64());

        // --- Fusion --------------------------------------------------------
        //
        // When rerank is disabled we can truncate before hydration (the old
        // path). When rerank is enabled we keep the full fused set so the
        // cross-encoder has real candidates to re-order; truncation to
        // `top_k` happens inside the reranker.
        //
        // Sprint 9 T-B3-BE5: when `p.boost` is `Some`, we prefetch + apply
        // the candidate-level multiplicative boost after RRF but before
        // truncation so the reweight influences which `top_k` survives.
        // When `None` (the default) this branch is a no-op and the fused
        // list is byte-identical to the pre-B3-BE5 output.
        let fuse_start = Instant::now();
        let mut fused = rrf_fuse(&dense_hits, &sparse_hits, &p);
        if let Some(boost) = &p.boost {
            let ids: Vec<i64> = fused.iter().map(|(id, _, _)| *id).collect();
            boost
                .prepare(&ids)
                .await
                .context("candidate boost prepare")?;
            for (id, score, _src) in fused.iter_mut() {
                let factor = boost.boost(*id);
                // Guard against pathological scorers returning NaN/inf.
                if factor.is_finite() && factor > 0.0 {
                    *score *= factor;
                }
            }
            // Re-sort after reweight; tie-break by id so results stay stable.
            fused.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.0.cmp(&b.0))
            });
        }
        let candidates: Vec<(i64, f32, HitSource)> = if p.rerank_enabled {
            fused
        } else {
            fused.into_iter().take(p.top_k).collect()
        };
        VECTOR_QUERY_DURATION
            .with_label_values(&["fuse"])
            .observe(fuse_start.elapsed().as_secs_f64());

        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let ids: Vec<i64> = candidates.iter().map(|(id, _, _)| *id).collect();
        let hits = self.hydrate(&ids, candidates).await?;

        // --- Optional rerank ----------------------------------------------
        if p.rerank_enabled {
            let rerank_start = Instant::now();
            let out = self
                .reranker
                .rerank(query_text, hits, p.top_k)
                .await
                .map_err(|e| anyhow::anyhow!("reranker failed: {e}"));
            VECTOR_QUERY_DURATION
                .with_label_values(&["rerank"])
                .observe(rerank_start.elapsed().as_secs_f64());
            out
        } else {
            Ok(hits)
        }
    }

    /// HNSW-only fallback. Bypasses RRF; score is cosine similarity.
    pub async fn search_dense_only(
        &self,
        query_vector: &[f32],
        top_k: usize,
    ) -> Result<Vec<RagHit>> {
        if top_k == 0 || query_vector.is_empty() {
            return Ok(Vec::new());
        }
        let idx = self.usearch.read().await;
        if idx.size() == 0 {
            return Ok(Vec::new());
        }
        let raw = idx.search(query_vector, top_k).context("hnsw search")?;
        drop(idx);

        let scored: Vec<(i64, f32, HitSource)> = raw
            .into_iter()
            .map(|(k, dist)| (k as i64, 1.0 - dist, HitSource::Dense))
            .collect();
        let ids: Vec<i64> = scored.iter().map(|(id, _, _)| *id).collect();
        self.hydrate(&ids, scored).await
    }

    /// BM25-only fallback. Bypasses RRF; score is the negated-bm25
    /// value (positive, larger = better).
    pub async fn search_sparse_only(&self, query_text: &str, top_k: usize) -> Result<Vec<RagHit>> {
        if top_k == 0 || query_text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let raw = self
            .sqlite
            .search_bm25(query_text, top_k)
            .await
            .context("bm25 search")?;
        let scored: Vec<(i64, f32, HitSource)> = raw
            .into_iter()
            .map(|(id, score)| (id, score, HitSource::Sparse))
            .collect();
        let ids: Vec<i64> = scored.iter().map(|(id, _, _)| *id).collect();
        self.hydrate(&ids, scored).await
    }

    /// Turn (chunk_id, score, source) triples into full [`RagHit`]s by
    /// joining content + file path, preserving the input order.
    async fn hydrate(
        &self,
        chunk_ids: &[i64],
        scored: Vec<(i64, f32, HitSource)>,
    ) -> Result<Vec<RagHit>> {
        let chunks = self
            .sqlite
            .query_chunks_by_ids(chunk_ids)
            .await
            .context("chunk hydration")?;
        if chunks.is_empty() {
            return Ok(Vec::new());
        }

        // Preload the files table — tiny relative to chunks, and avoids
        // an N+1 pattern across distinct file_ids.
        let files = self.sqlite.list_files().await.context("list_files")?;
        let path_by_file: HashMap<i64, String> =
            files.into_iter().map(|f| (f.id, f.path)).collect();

        // Index chunks by id so the output preserves `scored`'s order.
        let chunk_by_id: HashMap<i64, crate::sqlite::ChunkRow> =
            chunks.into_iter().map(|c| (c.id, c)).collect();

        let mut out = Vec::with_capacity(scored.len());
        for (id, score, source) in scored {
            let Some(c) = chunk_by_id.get(&id) else {
                continue; // ghost row: index refers to a missing chunk.
            };
            out.push(RagHit {
                chunk_id: c.id,
                file_id: c.file_id,
                content: c.content.clone(),
                score,
                source,
                path: path_by_file.get(&c.file_id).cloned().unwrap_or_default(),
            });
        }
        Ok(out)
    }
}

impl std::fmt::Debug for HybridSearcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HybridSearcher")
            .field("params", &self.params)
            .field("reranker", &self.reranker)
            .finish_non_exhaustive()
    }
}

/// Fuse two ranked lists with weighted reciprocal-rank-fusion.
///
/// `dense` and `sparse` are ordered best-first; their per-item float
/// scores are ignored — RRF only needs the rank. Returns
/// `(chunk_id, rrf_score, source)` sorted by descending RRF score.
fn rrf_fuse(
    dense: &[(i64, f32)],
    sparse: &[(i64, f32)],
    p: &HybridParams,
) -> Vec<(i64, f32, HitSource)> {
    let mut scores: HashMap<i64, (f32, bool, bool)> = HashMap::new();
    let k = p.rrf_k.max(1.0); // clamp to avoid div-by-zero if caller passes 0.

    for (rank, (id, _)) in dense.iter().enumerate() {
        let contrib = p.hnsw_weight / (k + (rank as f32 + 1.0));
        let entry = scores.entry(*id).or_insert((0.0, false, false));
        entry.0 += contrib;
        entry.1 = true;
    }
    for (rank, (id, _)) in sparse.iter().enumerate() {
        let contrib = p.bm25_weight / (k + (rank as f32 + 1.0));
        let entry = scores.entry(*id).or_insert((0.0, false, false));
        entry.0 += contrib;
        entry.2 = true;
    }

    let mut out: Vec<(i64, f32, HitSource)> = scores
        .into_iter()
        .map(|(id, (score, in_dense, in_sparse))| {
            let source = match (in_dense, in_sparse) {
                (true, true) => HitSource::Both,
                (true, false) => HitSource::Dense,
                (false, true) => HitSource::Sparse,
                (false, false) => unreachable!("score entry must come from at least one ranker"),
            };
            (id, score, source)
        })
        .collect();
    out.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0)) // stable tiebreak by id
    });
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_ranks_doc_in_both_paths_highest() {
        let dense = vec![(10, 0.99), (20, 0.80), (30, 0.50)];
        let sparse = vec![(30, 5.0), (20, 3.0), (40, 1.0)];
        let p = HybridParams::new();
        let fused = rrf_fuse(&dense, &sparse, &p);

        // Docs 20 and 30 appear in both; they should rank above 10 / 40.
        let top_ids: Vec<i64> = fused.iter().take(2).map(|(id, _, _)| *id).collect();
        assert!(
            top_ids.contains(&20) && top_ids.contains(&30),
            "top-2 should be the intersection, got {top_ids:?}"
        );
        // The source tag must reflect the intersection.
        for (id, _, source) in &fused {
            match *id {
                20 | 30 => assert_eq!(*source, HitSource::Both),
                10 => assert_eq!(*source, HitSource::Dense),
                40 => assert_eq!(*source, HitSource::Sparse),
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn rrf_weights_bias_path() {
        let dense = vec![(1, 0.9), (2, 0.8)];
        let sparse = vec![(2, 5.0), (1, 3.0)];

        // Bias heavily toward sparse: doc 2 (rank 1 in sparse) should win.
        let p = HybridParams {
            top_k: 2,
            overfetch_multiplier: 1,
            bm25_weight: 10.0,
            hnsw_weight: 0.1,
            rrf_k: 60.0,
            tag_filter: None,
            namespaces: None,
            rerank_enabled: false,
            tag_subtree: None,
            boost: None,
        };
        let fused = rrf_fuse(&dense, &sparse, &p);
        assert_eq!(fused[0].0, 2);
    }

    #[test]
    fn rrf_handles_empty_inputs() {
        let p = HybridParams::new();
        assert!(rrf_fuse(&[], &[], &p).is_empty());
        let dense = vec![(1, 0.5)];
        let only_dense = rrf_fuse(&dense, &[], &p);
        assert_eq!(only_dense.len(), 1);
        assert_eq!(only_dense[0].2, HitSource::Dense);
    }

    #[test]
    fn rrf_k_clamped_at_one() {
        // rrf_k=0 must not panic with div-by-zero.
        let p = HybridParams {
            top_k: 1,
            overfetch_multiplier: 1,
            bm25_weight: 1.0,
            hnsw_weight: 1.0,
            rrf_k: 0.0,
            tag_filter: None,
            namespaces: None,
            rerank_enabled: false,
            tag_subtree: None,
            boost: None,
        };
        let fused = rrf_fuse(&[(1, 0.0)], &[(1, 0.0)], &p);
        assert_eq!(fused.len(), 1);
        assert!(fused[0].1.is_finite());
    }

    // ---- tag filter integration ---------------------------------------

    use tempfile::TempDir;

    /// Build a tiny hybrid searcher: 3 chunks of 4-d vectors, first two
    /// tagged, the third untagged. Used by the tag-filter tests.
    async fn tagged_store() -> (HybridSearcher, TempDir) {
        let tmp = TempDir::new().unwrap();
        let sqlite = SqliteStore::open(&tmp.path().join("kb.sqlite"))
            .await
            .unwrap();
        let file_id = sqlite
            .insert_file("notes/t.md", "notes", "h", 0, 0)
            .await
            .unwrap();

        let corpus = [
            ("apple banana cherry", [1.0_f32, 0.0, 0.0, 0.0]),
            ("banana dog elephant", [0.9, 0.1, 0.0, 0.0]),
            ("grape honey iris", [0.0, 0.0, 1.0, 0.0]),
        ];
        let mut ids = [0_i64; 3];
        for (i, (text, vec)) in corpus.iter().enumerate() {
            ids[i] = sqlite
                .insert_chunk(file_id, i as i64, text, Some(vec), "general")
                .await
                .unwrap();
        }
        // ids[0] → rust+backend; ids[1] → rust+frontend; ids[2] → untagged.
        sqlite.insert_tag(ids[0], "rust").await.unwrap();
        sqlite.insert_tag(ids[0], "backend").await.unwrap();
        sqlite.insert_tag(ids[1], "rust").await.unwrap();
        sqlite.insert_tag(ids[1], "frontend").await.unwrap();

        let mut index = UsearchIndex::create_with_capacity(4, 16).unwrap();
        for (i, (_, vec)) in corpus.iter().enumerate() {
            index.add(ids[i] as u64, vec).unwrap();
        }

        let hybrid = HybridSearcher::new(
            Arc::new(sqlite),
            Arc::new(RwLock::new(index)),
            HybridParams::new(),
        );
        (hybrid, tmp)
    }

    fn params_with_filter(top_k: usize, tf: TagFilter) -> HybridParams {
        HybridParams {
            top_k,
            overfetch_multiplier: 3,
            bm25_weight: 1.0,
            hnsw_weight: 1.0,
            rrf_k: 60.0,
            tag_filter: Some(tf),
            namespaces: None,
            rerank_enabled: false,
            tag_subtree: None,
            boost: None,
        }
    }

    #[tokio::test]
    async fn tag_filter_required_matches_only_those_tags() {
        let (searcher, _tmp) = tagged_store().await;
        let tf = TagFilter {
            required: vec!["rust".into()],
            ..Default::default()
        };
        // "banana" matches chunks 0 and 1; both carry "rust" so both survive.
        // chunk 2 ("grape honey iris") has no "rust" tag → excluded.
        let hits = searcher
            .search(
                "banana",
                &[1.0, 0.0, 0.0, 0.0],
                Some(params_with_filter(10, tf)),
            )
            .await
            .unwrap();
        assert_eq!(hits.len(), 2);
        for h in &hits {
            assert!(!h.content.contains("grape"));
        }
    }

    #[tokio::test]
    async fn tag_filter_excluded_removes_matches() {
        let (searcher, _tmp) = tagged_store().await;
        let tf = TagFilter {
            excluded: vec!["frontend".into()],
            ..Default::default()
        };
        // chunk 1 is tagged frontend → excluded. chunks 0 and 2 pass.
        let hits = searcher
            .search(
                "banana grape",
                &[1.0, 0.0, 0.0, 0.0],
                Some(params_with_filter(10, tf)),
            )
            .await
            .unwrap();
        let contents: Vec<&str> = hits.iter().map(|h| h.content.as_str()).collect();
        assert!(contents.iter().any(|c| c.contains("apple")));
        // "banana dog elephant" is the frontend-tagged chunk — must be gone.
        assert!(!contents.iter().any(|c| c.contains("dog elephant")));
    }

    #[tokio::test]
    async fn tag_filter_any_of_ors() {
        let (searcher, _tmp) = tagged_store().await;
        let tf = TagFilter {
            any_of: vec!["backend".into(), "frontend".into()],
            ..Default::default()
        };
        // chunks 0 (backend) and 1 (frontend) qualify; chunk 2 does not.
        let hits = searcher
            .search(
                "banana",
                &[1.0, 0.0, 0.0, 0.0],
                Some(params_with_filter(10, tf)),
            )
            .await
            .unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[tokio::test]
    async fn tag_filter_empty_equivalent_to_no_filter() {
        let (searcher, _tmp) = tagged_store().await;
        let with_empty = searcher
            .search(
                "banana",
                &[1.0, 0.0, 0.0, 0.0],
                Some(params_with_filter(10, TagFilter::default())),
            )
            .await
            .unwrap();
        let without = searcher
            .search("banana", &[1.0, 0.0, 0.0, 0.0], None)
            .await
            .unwrap();
        assert_eq!(with_empty.len(), without.len());
    }

    #[tokio::test]
    async fn tag_filter_combined_required_and_excluded() {
        let (searcher, _tmp) = tagged_store().await;
        let tf = TagFilter {
            required: vec!["rust".into()],
            excluded: vec!["frontend".into()],
            ..Default::default()
        };
        // Only chunk 0 satisfies rust ∧ ¬frontend.
        let hits = searcher
            .search(
                "banana",
                &[1.0, 0.0, 0.0, 0.0],
                Some(params_with_filter(10, tf)),
            )
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].content.contains("apple"));
    }

    // ---- subtree filter (Sprint 9 T-B3-BE3) ---------------------------

    /// Seed a searcher with hierarchical tag paths so subtree filters
    /// have real depth to walk. Corpus layout:
    ///  - chunk 0 "alpha" → role.protagonist.voice
    ///  - chunk 1 "bravo" → role.antagonist
    ///  - chunk 2 "charlie" → mood.calm
    ///  - chunk 3 "delta" → (untagged)
    async fn subtree_tagged_store() -> (HybridSearcher, TempDir) {
        let tmp = TempDir::new().unwrap();
        let sqlite = SqliteStore::open(&tmp.path().join("kb.sqlite"))
            .await
            .unwrap();
        let file_id = sqlite
            .insert_file("notes/st.md", "notes", "h", 0, 0)
            .await
            .unwrap();

        let corpus: &[(&str, [f32; 4], &str)] = &[
            ("alpha word", [1.0, 0.0, 0.0, 0.0], "role.protagonist.voice"),
            ("bravo word", [0.9, 0.1, 0.0, 0.0], "role.antagonist"),
            ("charlie word", [0.0, 0.0, 1.0, 0.0], "mood.calm"),
            ("delta word", [0.0, 1.0, 0.0, 0.0], ""),
        ];
        let mut ids = [0_i64; 4];
        let mut index = UsearchIndex::create_with_capacity(4, 16).unwrap();
        for (i, (text, v, path)) in corpus.iter().enumerate() {
            ids[i] = sqlite
                .insert_chunk(file_id, i as i64, text, Some(v), "general")
                .await
                .unwrap();
            if !path.is_empty() {
                sqlite.attach_chunk_to_tag_path(ids[i], path).await.unwrap();
            }
            index.add(ids[i] as u64, v).unwrap();
        }
        let hybrid = HybridSearcher::new(
            Arc::new(sqlite),
            Arc::new(RwLock::new(index)),
            HybridParams::new(),
        );
        (hybrid, tmp)
    }

    fn params_with_subtree(top_k: usize, root: &str) -> HybridParams {
        HybridParams {
            top_k,
            overfetch_multiplier: 3,
            bm25_weight: 1.0,
            hnsw_weight: 1.0,
            rrf_k: 60.0,
            tag_filter: None,
            namespaces: None,
            rerank_enabled: false,
            tag_subtree: Some(root.to_string()),
            boost: None,
        }
    }

    #[tokio::test]
    async fn subtree_filter_matches_nested_paths() {
        let (searcher, _tmp) = subtree_tagged_store().await;
        // Both `role.protagonist.voice` (descendant) and `role.antagonist`
        // (direct child) are in the `role` subtree; `mood.calm` must not leak.
        let hits = searcher
            .search(
                "word",
                &[1.0, 0.0, 0.0, 0.0],
                Some(params_with_subtree(10, "role")),
            )
            .await
            .unwrap();
        let contents: Vec<&str> = hits.iter().map(|h| h.content.as_str()).collect();
        assert_eq!(hits.len(), 2, "got: {contents:?}");
        assert!(contents.iter().any(|c| c.contains("alpha")));
        assert!(contents.iter().any(|c| c.contains("bravo")));
        assert!(!contents.iter().any(|c| c.contains("charlie")));
        assert!(!contents.iter().any(|c| c.contains("delta")));
    }

    #[tokio::test]
    async fn subtree_filter_does_not_leak_across_roots() {
        let (searcher, _tmp) = subtree_tagged_store().await;
        // `mood` subtree: only `mood.calm` → chunk 2.
        let hits = searcher
            .search(
                "word",
                &[1.0, 0.0, 0.0, 0.0],
                Some(params_with_subtree(10, "mood")),
            )
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].content.contains("charlie"));
    }

    #[tokio::test]
    async fn flat_filter_still_works_post_migration() {
        // Runs on a schema-v6 DB (fresh SqliteStore::open) but exercises
        // the flat v5-style TagFilter API. After the v5→v6 retarget this
        // must still behave identically to the pre-migration implementation.
        let (searcher, _tmp) = tagged_store().await;
        let tf = TagFilter {
            required: vec!["backend".into()],
            ..Default::default()
        };
        let hits = searcher
            .search(
                "banana",
                &[1.0, 0.0, 0.0, 0.0],
                Some(params_with_filter(10, tf)),
            )
            .await
            .unwrap();
        // Only chunk 0 has the "backend" tag.
        assert_eq!(hits.len(), 1);
        assert!(hits[0].content.contains("apple"));
    }

    // ---- namespace filter (Sprint 9 T1) -------------------------------

    /// Seed a searcher with 4 chunks split across two namespaces:
    /// - ids[0..2] → `general` ("apple banana cherry", "banana dog")
    /// - ids[2..4] → `diary:a`  ("banana rain", "banana snow")
    async fn namespaced_store() -> (HybridSearcher, TempDir) {
        let tmp = TempDir::new().unwrap();
        let sqlite = SqliteStore::open(&tmp.path().join("kb.sqlite"))
            .await
            .unwrap();
        let file_id = sqlite.insert_file("ns.md", "ns", "h", 0, 0).await.unwrap();

        let rows: &[(&str, [f32; 4], &str)] = &[
            ("apple banana cherry", [1.0, 0.0, 0.0, 0.0], "general"),
            ("banana dog", [0.9, 0.1, 0.0, 0.0], "general"),
            ("banana rain", [0.8, 0.0, 0.2, 0.0], "diary:a"),
            ("banana snow", [0.0, 1.0, 0.0, 0.0], "diary:a"),
        ];
        let mut ids = [0_i64; 4];
        let mut index = UsearchIndex::create_with_capacity(4, 16).unwrap();
        for (i, (text, v, ns)) in rows.iter().enumerate() {
            ids[i] = sqlite
                .insert_chunk(file_id, i as i64, text, Some(v), ns)
                .await
                .unwrap();
            index.add(ids[i] as u64, v).unwrap();
        }
        let hybrid = HybridSearcher::new(
            Arc::new(sqlite),
            Arc::new(RwLock::new(index)),
            HybridParams::new(),
        );
        (hybrid, tmp)
    }

    fn ns_params(namespaces: Option<Vec<String>>) -> HybridParams {
        HybridParams {
            top_k: 10,
            overfetch_multiplier: 3,
            bm25_weight: 1.0,
            hnsw_weight: 1.0,
            rrf_k: 60.0,
            tag_filter: None,
            namespaces,
            rerank_enabled: false,
            tag_subtree: None,
            boost: None,
        }
    }

    #[tokio::test]
    async fn namespace_filter_restricts_to_named_namespace() {
        let (searcher, _tmp) = namespaced_store().await;
        let hits = searcher
            .search(
                "banana",
                &[1.0, 0.0, 0.0, 0.0],
                Some(ns_params(Some(vec!["diary:a".into()]))),
            )
            .await
            .unwrap();
        // Only the two diary:a rows should survive.
        assert_eq!(
            hits.len(),
            2,
            "got: {:?}",
            hits.iter().map(|h| &h.content).collect::<Vec<_>>()
        );
        for h in &hits {
            assert!(
                h.content.contains("rain") || h.content.contains("snow"),
                "unexpected leakage: {}",
                h.content
            );
        }
    }

    #[tokio::test]
    async fn namespace_none_defaults_to_general_only() {
        // Legacy callers who don't set `namespaces` must continue to see
        // the pre-S9 behaviour: only the `general` namespace is searched.
        let (searcher, _tmp) = namespaced_store().await;
        let hits = searcher
            .search("banana", &[1.0, 0.0, 0.0, 0.0], Some(ns_params(None)))
            .await
            .unwrap();
        // 2 general rows — 0 diary:a rows.
        assert_eq!(hits.len(), 2);
        for h in &hits {
            assert!(
                h.content.contains("apple") || h.content.contains("dog"),
                "non-general leaked: {}",
                h.content
            );
        }
    }

    #[tokio::test]
    async fn namespace_empty_vec_treated_as_none() {
        let (searcher, _tmp) = namespaced_store().await;
        let hits = searcher
            .search(
                "banana",
                &[1.0, 0.0, 0.0, 0.0],
                Some(ns_params(Some(vec![]))),
            )
            .await
            .unwrap();
        assert_eq!(hits.len(), 2); // same as None → general only.
    }

    #[tokio::test]
    async fn namespace_multi_value_union() {
        let (searcher, _tmp) = namespaced_store().await;
        let hits = searcher
            .search(
                "banana",
                &[1.0, 0.0, 0.0, 0.0],
                Some(ns_params(Some(vec!["general".into(), "diary:a".into()]))),
            )
            .await
            .unwrap();
        // All 4 rows match "banana".
        assert_eq!(hits.len(), 4);
    }

    #[tokio::test]
    async fn list_namespaces_counts_rows_per_namespace() {
        let (searcher, _tmp) = namespaced_store().await;
        let nss = searcher.sqlite.list_namespaces().await.unwrap();
        assert_eq!(
            nss,
            vec![("diary:a".to_string(), 2u64), ("general".to_string(), 2u64),]
        );
    }

    // ---- reranker integration (Sprint 3 T6) ----------------------------

    use crate::rerank::Reranker;
    use async_trait::async_trait;

    /// Reverses the order RRF produced, so we can observe whether the
    /// searcher actually consulted the injected reranker.
    #[derive(Debug, Default)]
    struct ReversingReranker;

    #[async_trait]
    impl Reranker for ReversingReranker {
        async fn rerank(
            &self,
            _query: &str,
            mut hits: Vec<RagHit>,
            top_k: usize,
        ) -> Result<Vec<RagHit>, corlinman_core::error::CorlinmanError> {
            hits.reverse();
            hits.truncate(top_k);
            Ok(hits)
        }
    }

    fn rerank_params(top_k: usize, enabled: bool) -> HybridParams {
        HybridParams {
            top_k,
            overfetch_multiplier: 3,
            bm25_weight: 1.0,
            hnsw_weight: 1.0,
            rrf_k: 60.0,
            tag_filter: None,
            namespaces: None,
            rerank_enabled: enabled,
            tag_subtree: None,
            boost: None,
        }
    }

    #[tokio::test]
    async fn rerank_disabled_preserves_rrf_order() {
        let (searcher, _tmp) = tagged_store().await;
        // Even with a reversing reranker installed, `rerank_enabled=false`
        // must leave the RRF ordering intact.
        let searcher = searcher.with_reranker(Arc::new(ReversingReranker));
        let hits = searcher
            .search(
                "banana",
                &[1.0, 0.0, 0.0, 0.0],
                Some(rerank_params(10, false)),
            )
            .await
            .unwrap();
        assert!(!hits.is_empty());
        // "apple banana cherry" is the closest dense match (vector [1,0,0,0])
        // and also wins BM25 on "banana" → it should lead the RRF output.
        assert!(
            hits[0].content.contains("apple"),
            "expected RRF top to be the apple chunk, got {:?}",
            hits.iter().map(|h| &h.content).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn rerank_enabled_uses_injected_reranker() {
        let (searcher, _tmp) = tagged_store().await;
        let baseline = searcher
            .search(
                "banana",
                &[1.0, 0.0, 0.0, 0.0],
                Some(rerank_params(10, false)),
            )
            .await
            .unwrap();

        let searcher = searcher.with_reranker(Arc::new(ReversingReranker));
        let reranked = searcher
            .search(
                "banana",
                &[1.0, 0.0, 0.0, 0.0],
                Some(rerank_params(10, true)),
            )
            .await
            .unwrap();

        assert_eq!(baseline.len(), reranked.len());
        assert!(baseline.len() >= 2, "need ≥2 hits to test reversal");
        // Reranker reverses: the former last should be first, former first last.
        assert_eq!(
            reranked.first().unwrap().chunk_id,
            baseline.last().unwrap().chunk_id
        );
        assert_eq!(
            reranked.last().unwrap().chunk_id,
            baseline.first().unwrap().chunk_id
        );
    }

    /// S7.T3: each `search()` call records into
    /// `corlinman_vector_query_duration_seconds` for the three core
    /// stages (`hnsw`, `bm25`, `fuse`). Counters are process-global so
    /// other concurrent tests may also observe — we assert the deltas
    /// are non-zero rather than exact.
    #[tokio::test]
    async fn search_records_stage_metrics() {
        let (searcher, _tmp) = tagged_store().await;

        let hnsw_before = VECTOR_QUERY_DURATION
            .with_label_values(&["hnsw"])
            .get_sample_count();
        let bm25_before = VECTOR_QUERY_DURATION
            .with_label_values(&["bm25"])
            .get_sample_count();
        let fuse_before = VECTOR_QUERY_DURATION
            .with_label_values(&["fuse"])
            .get_sample_count();

        let _ = searcher
            .search("banana", &[1.0, 0.0, 0.0, 0.0], None)
            .await
            .unwrap();

        let hnsw_after = VECTOR_QUERY_DURATION
            .with_label_values(&["hnsw"])
            .get_sample_count();
        let bm25_after = VECTOR_QUERY_DURATION
            .with_label_values(&["bm25"])
            .get_sample_count();
        let fuse_after = VECTOR_QUERY_DURATION
            .with_label_values(&["fuse"])
            .get_sample_count();

        assert!(
            hnsw_after > hnsw_before,
            "hnsw stage must record at least one observation"
        );
        assert!(
            bm25_after > bm25_before,
            "bm25 stage must record at least one observation"
        );
        assert!(
            fuse_after > fuse_before,
            "fuse stage must record at least one observation"
        );
    }

    #[tokio::test]
    async fn rerank_enabled_truncates_to_top_k() {
        let (searcher, _tmp) = tagged_store().await;
        let searcher = searcher.with_reranker(Arc::new(ReversingReranker));
        // Corpus has 3 chunks; ask for top_k=2 with rerank on.
        let hits = searcher
            .search(
                "banana grape",
                &[1.0, 0.0, 0.0, 0.0],
                Some(rerank_params(2, true)),
            )
            .await
            .unwrap();
        assert!(hits.len() <= 2);
    }

    // ---- EPA boost (Sprint 9 T-B3-BE5) -------------------------------

    #[test]
    fn dynamic_boost_clamps_to_range() {
        // logic_depth=1, base=10 ⇒ unclamped would be 10, clamp to 2.5 top.
        let hi = dynamic_boost_rust(1.0, 0.0, 0.0, 10.0, (0.5, 2.5));
        assert!((hi - 2.5).abs() < 1e-6, "expected 2.5, got {hi}");
        // logic_depth=0 ⇒ factor=0 ⇒ clamp to floor 0.5.
        let lo = dynamic_boost_rust(0.0, 0.0, 0.0, 1.0, (0.5, 2.5));
        assert!((lo - 0.5).abs() < 1e-6, "expected 0.5, got {lo}");
        // Out-of-range inputs (>1) are clamped to 1 before the formula runs,
        // so a huge logic_depth doesn't blow past the ceiling.
        let capped = dynamic_boost_rust(99.0, 99.0, 0.0, 1.0, (0.5, 2.5));
        assert!((capped - 2.0).abs() < 1e-6, "expected 2.0, got {capped}");
    }

    async fn seed_epa_store() -> (Arc<SqliteStore>, [i64; 3], tempfile::TempDir) {
        let tmp = TempDir::new().unwrap();
        let sqlite = Arc::new(
            SqliteStore::open(&tmp.path().join("kb.sqlite"))
                .await
                .unwrap(),
        );
        let file_id = sqlite
            .insert_file("epa.md", "epa", "h", 0, 0)
            .await
            .unwrap();
        let mut ids = [0_i64; 3];
        let vecs = [
            [1.0_f32, 0.0, 0.0, 0.0],
            [0.9_f32, 0.1, 0.0, 0.0],
            [0.8_f32, 0.2, 0.0, 0.0],
        ];
        for (i, v) in vecs.iter().enumerate() {
            ids[i] = sqlite
                .insert_chunk(
                    file_id,
                    i as i64,
                    &format!("banana chunk {i}"),
                    Some(v),
                    "general",
                )
                .await
                .unwrap();
        }
        // Only seed EPA for ids[0] and ids[2]; ids[1] stays missing so we
        // can exercise the `None ⇒ pass-through` branch.
        sqlite
            .upsert_chunk_epa(ids[0], &[0.5_f32, 0.1], 0.3, 0.9)
            .await
            .unwrap();
        sqlite
            .upsert_chunk_epa(ids[2], &[0.2_f32, 0.4], 0.8, 0.1)
            .await
            .unwrap();
        (sqlite, ids, tmp)
    }

    #[tokio::test]
    async fn epa_boost_returns_one_for_missing_row() {
        let (sqlite, ids, _tmp) = seed_epa_store().await;
        let booster = EpaBoost::new(sqlite, 1.0, (0.5, 2.5));
        // Prepare only the missing id so the cache is primed with 1.0.
        booster.prepare(&[ids[1]]).await.unwrap();
        let factor = booster.boost(ids[1]);
        assert!(
            (factor - 1.0).abs() < 1e-6,
            "missing EPA row must pass through; got {factor}"
        );
        // An id that wasn't even prepared also returns 1.0 (cache miss).
        assert!((booster.boost(9999) - 1.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn epa_boost_uses_logic_depth() {
        let (sqlite, ids, _tmp) = seed_epa_store().await;
        let booster = EpaBoost::new(sqlite, 1.0, (0.5, 2.5));
        booster.prepare(&[ids[0], ids[2]]).await.unwrap();
        let b_high_ld = booster.boost(ids[0]); // logic_depth = 0.9
        let b_low_ld = booster.boost(ids[2]); // logic_depth = 0.1
        assert!(
            b_high_ld > b_low_ld,
            "higher logic_depth must yield a larger boost; got {b_high_ld} vs {b_low_ld}"
        );
        // Concrete numeric check vs the reference formula.
        let expected_hi = dynamic_boost_rust(0.9, 0.0, 0.0, 1.0, (0.5, 2.5));
        assert!((b_high_ld - expected_hi).abs() < 1e-6);
    }

    fn boost_params(top_k: usize, boost: Option<Arc<dyn CandidateBoost>>) -> HybridParams {
        HybridParams {
            top_k,
            overfetch_multiplier: 3,
            bm25_weight: 1.0,
            hnsw_weight: 1.0,
            rrf_k: 60.0,
            tag_filter: None,
            namespaces: None,
            rerank_enabled: false,
            tag_subtree: None,
            boost,
        }
    }

    /// Baseline identity check: `boost = None` must produce the exact
    /// same hit list (ids + scores + order) as pre-B3-BE5.
    #[tokio::test]
    async fn hybrid_search_without_boost_is_byte_identical_to_baseline() {
        let (searcher, _tmp) = tagged_store().await;
        let baseline = searcher
            .search(
                "banana",
                &[1.0, 0.0, 0.0, 0.0],
                Some(boost_params(10, None)),
            )
            .await
            .unwrap();
        // Same params again — should produce byte-identical output.
        let repeat = searcher
            .search(
                "banana",
                &[1.0, 0.0, 0.0, 0.0],
                Some(boost_params(10, None)),
            )
            .await
            .unwrap();
        assert_eq!(baseline.len(), repeat.len());
        for (a, b) in baseline.iter().zip(repeat.iter()) {
            assert_eq!(a.chunk_id, b.chunk_id);
            assert!(
                (a.score - b.score).abs() < 1e-9,
                "baseline drift: {a:?} vs {b:?}"
            );
            assert_eq!(a.source, b.source);
        }
    }

    /// With EPA rows attached and the boost hook active, a chunk with
    /// higher `logic_depth` must be able to outrank a baseline-equal
    /// competitor. We construct two chunks whose pre-boost RRF scores are
    /// within a hair of each other and seed EPA rows with wildly different
    /// logic_depth to make the reweight observable.
    #[tokio::test]
    async fn hybrid_search_with_epa_boost_reranks_higher_logic_depth_chunks() {
        let tmp = TempDir::new().unwrap();
        let sqlite = Arc::new(
            SqliteStore::open(&tmp.path().join("kb.sqlite"))
                .await
                .unwrap(),
        );
        let file_id = sqlite.insert_file("r.md", "r", "h", 0, 0).await.unwrap();

        // Two chunks with near-identical content ("banana") + very close
        // vectors so RRF will rank them similarly before the boost kicks in.
        let low_id = sqlite
            .insert_chunk(
                file_id,
                0,
                "banana one",
                Some(&[1.0, 0.0, 0.0, 0.0]),
                "general",
            )
            .await
            .unwrap();
        let high_id = sqlite
            .insert_chunk(
                file_id,
                1,
                "banana two",
                Some(&[0.99, 0.01, 0.0, 0.0]),
                "general",
            )
            .await
            .unwrap();

        // Seed EPA so high_id carries max logic_depth (near 1.0) and
        // low_id carries near-zero. With base_tag_boost=1 + range (0.5, 2.5)
        // that yields ~2.0 vs ~0.5 factors — a 4× gap, plenty to flip order.
        sqlite
            .upsert_chunk_epa(low_id, &[0.1_f32, 0.2], 0.95, 0.05)
            .await
            .unwrap();
        sqlite
            .upsert_chunk_epa(high_id, &[0.3_f32, 0.4], 0.05, 0.95)
            .await
            .unwrap();

        let mut index = UsearchIndex::create_with_capacity(4, 16).unwrap();
        index.add(low_id as u64, &[1.0_f32, 0.0, 0.0, 0.0]).unwrap();
        index
            .add(high_id as u64, &[0.99_f32, 0.01, 0.0, 0.0])
            .unwrap();

        let searcher = HybridSearcher::new(
            sqlite.clone(),
            Arc::new(RwLock::new(index)),
            HybridParams::new(),
        );

        // Baseline (no boost): low_id ranks first because its vector is a
        // tighter match to the query and it wins BM25 tie-break at rank 1.
        let baseline = searcher
            .search("banana", &[1.0, 0.0, 0.0, 0.0], None)
            .await
            .unwrap();
        assert!(baseline.len() >= 2, "need both chunks in baseline");
        assert_eq!(
            baseline[0].chunk_id,
            low_id,
            "baseline should rank low_id first, got {:?}",
            baseline.iter().map(|h| h.chunk_id).collect::<Vec<_>>()
        );

        // With EPA boost: high_id's logic_depth=0.95 boosts its score ~2×
        // while low_id's logic_depth=0.05 drops to the floor factor, so
        // high_id must climb above low_id.
        let booster: Arc<dyn CandidateBoost> =
            Arc::new(EpaBoost::new(sqlite.clone(), 1.0, (0.5, 2.5)));
        let boosted = searcher
            .search(
                "banana",
                &[1.0, 0.0, 0.0, 0.0],
                Some(boost_params(10, Some(booster))),
            )
            .await
            .unwrap();
        assert_eq!(
            boosted[0].chunk_id,
            high_id,
            "EPA boost should flip order so high_id is first, got {:?}",
            boosted.iter().map(|h| h.chunk_id).collect::<Vec<_>>()
        );
    }
}
