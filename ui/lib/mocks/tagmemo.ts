/**
 * Mock EPA / Residual-Pyramid telemetry for B5-FE1 (tagmemo dashboard).
 *
 * Emulates the shape the upcoming `corlinman-tagmemo` gateway endpoint will
 * emit: each chunk carries low-dim EPA projections, an entropy/logic_depth
 * pair, the top dominant axes and a pyramid decomposition. Keep this file
 * deterministic so snapshot tests stay stable — we use a tiny LCG instead of
 * pulling in a seedrandom dep.
 *
 * TODO(B3-BE4 / B3-BE5): replace with `apiFetch<TagMemoChunk[]>(...)` once
 * the gateway endpoint + backfill land.
 */

export interface AxisProjection {
  label: string;
  energy: number;
  projection: number;
}

export interface PyramidLevel {
  axis_label: string;
  coefficient: number;
  explained_energy: number;
  cumulative_explained: number;
}

export interface TagMemoChunk {
  chunk_id: number;
  /** Full projection vector; first 3 are used for the scatter. */
  projections: number[];
  probabilities: number[];
  /** Normalised 0..1. */
  entropy: number;
  /** Normalised 0..1 (deeper implications ⇒ higher value). */
  logic_depth: number;
  dominant_axes: AxisProjection[];
  pyramid_levels: PyramidLevel[];
  features: { tag_memo_activation: number; coverage: number };
}

// ------------------------- deterministic LCG -------------------------
// Numerical recipes constants; period 2^32. Plenty for 500 rows.
function makeRng(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (Math.imul(1664525, state) + 1013904223) >>> 0;
    return state / 0x1_0000_0000;
  };
}

const AXIS_LABELS = [
  "identity",
  "intent",
  "emotion",
  "tempo",
  "certainty",
  "locality",
  "abstraction",
  "tense",
] as const;

const PROJECTION_DIM = 6;
const PYRAMID_DEPTH = 5;

export const MOCK_CHUNK_COUNT = 500;

/**
 * Build N chunks deterministically. Output is cached because every caller
 * wants the same array.
 */
let CACHE: TagMemoChunk[] | null = null;

export function generateMockChunks(
  count: number = MOCK_CHUNK_COUNT,
): TagMemoChunk[] {
  if (CACHE && count === MOCK_CHUNK_COUNT) return CACHE;
  const rng = makeRng(0xC0FFEE);
  const rows: TagMemoChunk[] = [];
  for (let i = 0; i < count; i += 1) {
    // Projections centred on 0 with light drift by index so the scatter shows
    // structure, not a ball.
    const drift = (i / count) * 2 - 1;
    const projections: number[] = [];
    for (let d = 0; d < PROJECTION_DIM; d += 1) {
      const raw = (rng() - 0.5) * 2 + (d < 3 ? drift * 0.6 : 0);
      projections.push(raw);
    }

    // Probabilities: 8 buckets, normalised.
    const rawProbs: number[] = [];
    let sum = 0;
    for (let k = 0; k < AXIS_LABELS.length; k += 1) {
      const v = rng();
      rawProbs.push(v);
      sum += v;
    }
    const probabilities = rawProbs.map((v) => v / sum);

    // Entropy: Shannon, normalised to 0..1 by log(K).
    const H =
      -probabilities.reduce(
        (acc, p) => acc + (p > 0 ? p * Math.log(p) : 0),
        0,
      ) / Math.log(AXIS_LABELS.length);
    const entropy = Math.max(0, Math.min(1, H));

    // Logic depth correlates loosely with entropy but has its own noise
    // ⇒ dual-line chart is actually interesting.
    const logic_depth = Math.max(
      0,
      Math.min(1, entropy * 0.6 + rng() * 0.5),
    );

    // Dominant axes: sort probabilities desc, pick top 3.
    const ranked = probabilities
      .map((p, idx) => ({ p, idx }))
      .sort((a, b) => b.p - a.p);
    const dominant_axes: AxisProjection[] = ranked.slice(0, 3).map((r) => ({
      label: AXIS_LABELS[r.idx]!,
      energy: r.p,
      projection: projections[r.idx % PROJECTION_DIM]!,
    }));

    // Pyramid levels: decreasing explained_energy that sums ≤ 1.
    const pyramid_levels: PyramidLevel[] = [];
    let cumulative = 0;
    let remaining = 0.95 + rng() * 0.05;
    for (let lvl = 0; lvl < PYRAMID_DEPTH; lvl += 1) {
      const frac = lvl === PYRAMID_DEPTH - 1 ? 1 : 0.35 + rng() * 0.35;
      const explained = remaining * frac;
      remaining -= explained;
      cumulative += explained;
      pyramid_levels.push({
        axis_label: AXIS_LABELS[(i + lvl) % AXIS_LABELS.length]!,
        coefficient: (rng() - 0.5) * 2,
        explained_energy: explained,
        cumulative_explained: cumulative,
      });
    }

    rows.push({
      chunk_id: i,
      projections,
      probabilities,
      entropy,
      logic_depth,
      dominant_axes,
      pyramid_levels,
      features: {
        tag_memo_activation: rng(),
        coverage: 0.4 + rng() * 0.6,
      },
    });
  }
  if (count === MOCK_CHUNK_COUNT) CACHE = rows;
  return rows;
}

/**
 * Aggregate stats for the header row.
 */
export interface TagMemoStats {
  chunkCount: number;
  avgEntropy: number;
  avgLogicDepth: number;
  uniqueAxes: number;
}

export function summariseChunks(chunks: TagMemoChunk[]): TagMemoStats {
  if (chunks.length === 0) {
    return { chunkCount: 0, avgEntropy: 0, avgLogicDepth: 0, uniqueAxes: 0 };
  }
  let eSum = 0;
  let dSum = 0;
  const axes = new Set<string>();
  for (const c of chunks) {
    eSum += c.entropy;
    dSum += c.logic_depth;
    for (const a of c.dominant_axes) axes.add(a.label);
  }
  return {
    chunkCount: chunks.length,
    avgEntropy: eSum / chunks.length,
    avgLogicDepth: dSum / chunks.length,
    uniqueAxes: axes.size,
  };
}
