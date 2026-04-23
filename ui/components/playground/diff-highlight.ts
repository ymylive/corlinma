/**
 * Naive line-by-line diff for the Protocol Playground (B3-FE1).
 *
 * Intentionally small — no libs, no LCS. We walk both line arrays in
 * lockstep: at index `i` a line is "differs" iff `a[i] !== b[i]`. Lines past
 * the end of the other side are also "differs". Good enough to pulse-glow
 * divergent rows once both streams finish; the visual cue is subtle so
 * occasional mis-alignments don't confuse the reader.
 */

export type DiffMark = "same" | "differs";

export interface DiffRow {
  index: number;
  left: string | null;
  right: string | null;
  mark: DiffMark;
}

export function diffLines(a: string, b: string): DiffRow[] {
  const left = a.split("\n");
  const right = b.split("\n");
  const n = Math.max(left.length, right.length);
  const rows: DiffRow[] = [];
  for (let i = 0; i < n; i += 1) {
    const l = i < left.length ? left[i] : null;
    const r = i < right.length ? right[i] : null;
    rows.push({
      index: i,
      left: l,
      right: r,
      mark: l === r ? "same" : "differs",
    });
  }
  return rows;
}

/** Return the set of zero-based line indexes where the two texts differ. */
export function diffLineIndexes(a: string, b: string): Set<number> {
  const out = new Set<number>();
  for (const row of diffLines(a, b)) {
    if (row.mark === "differs") out.add(row.index);
  }
  return out;
}
