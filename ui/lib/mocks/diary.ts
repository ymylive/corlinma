/**
 * Mock diary entries for B5-FE2 (Diary timeline page).
 *
 * Backend surface is not yet implemented — the page consumes this stub until
 * a future `GET /admin/diary` lands. Shape is already close to what the
 * endpoint is expected to return.
 *
 * TODO(B5-BE?): swap to `apiFetch<DiaryEntry[]>("/admin/diary")`.
 */

export interface DiaryEntry {
  id: string;
  /** Milliseconds since epoch. Used both for sort + grouping. */
  created_at_ms: number;
  title: string;
  body_markdown: string;
  tags: string[];
  /** Optional session key link — opens the relevant log when present. */
  session_key?: string;
}

/**
 * Deterministic generator so tests can count entries / dates exactly.
 *
 * Base date is 2026-04-22 (today per the project fixture). We emit entries
 * spanning the last 10 calendar days with 3 entries per day at varying
 * local times.
 */
const BASE_UTC_MIDNIGHT = Date.parse("2026-04-22T00:00:00Z");
const DAY_MS = 86_400_000;

const TAG_POOL = [
  "rust",
  "python",
  "ui",
  "bugfix",
  "refactor",
  "research",
  "meeting",
  "debug",
  "perf",
  "docs",
];

const TITLE_POOL = [
  "Shipped the async SSE backfill",
  "Chased down the ArcSwap ordering bug",
  "Carved out the diary timeline shell",
  "Pair-debugged the Napcat auth flow",
  "Benchmarked the embedding pipeline",
  "Reviewed the approvals RFC",
  "Plumbed the hook runtime fan-out",
  "Rewrote the sidebar keyboard nav",
  "Hardened the scheduler retry path",
  "Triaged the Friday incident",
  "Drafted the vector namespace plan",
  "Swept the reduced-motion coverage",
  "Patched the Telegram webhook drift",
  "Tuned the topology graph layout",
  "Staked out the runbook skeleton",
];

const BODY_SNIPPETS = [
  "Landed the mechanism end-to-end. Follow-up: extract the retry budget into config.",
  "Root cause turned out to be a stale `Arc<Config>` snapshot held across an await point. Fix: re-pull after `.await`.",
  "Wrote the scaffolding — sticky headers, scroll-driven transforms, shared-layout reader. Tests next.",
  "Two sessions with the device shell. Result: token refresh works, QR rotation still flaky, opened #142.",
  "p50 steady at ~140ms, p99 spiked to 820 on the Qwen path. Suspect cold-connect pool. Filed follow-up.",
  "Greenlit with comments. Main concern: empty-result UX when the rule set is empty. Added to the tracker.",
  "Fan-out now respects `priority` + `parallel_limit`. Still need the dead-letter plumbing for stuck workers.",
  "Arrow keys now move focus within groups; ARIA tree semantics intact. Ran the axe audit — zero violations.",
  "Backoff was doubling per-attempt instead of per-job. Fixed; the nightly re-tries a reasonable number now.",
  "Postmortem draft circulated. Contributing factors: missing alert on the disk queue + operator muscle memory.",
  "Namespacing keys with a `<tenant>:` prefix; migration is additive so we can roll back without data loss.",
  "Found three components still animating under `prefers-reduced-motion: reduce`. Patched + locked with tests.",
  "Webhook URL regeneration on bot token change now rotates cleanly. Added a guard for accidental overwrites.",
  "Ring orbit radius was too tight on small screens; added a breakpoint and re-ran the Playwright snapshots.",
  "Outline is in: preamble, playbook, rollback table, contact list. Will flesh out each section next week.",
];

function pickTags(seed: number): string[] {
  const n = 1 + (seed % 3); // 1..3 tags
  const out: string[] = [];
  for (let i = 0; i < n; i++) {
    out.push(TAG_POOL[(seed * 7 + i * 13) % TAG_POOL.length]);
  }
  // Dedup while preserving order.
  return Array.from(new Set(out));
}

function buildEntries(): DiaryEntry[] {
  const entries: DiaryEntry[] = [];
  // 10 dates × 3 entries = 30 entries total.
  for (let d = 0; d < 10; d++) {
    const dayMidnight = BASE_UTC_MIDNIGHT - d * DAY_MS;
    // Three times of day — morning / midday / evening local-ish.
    const offsets = [9 * 3600 + 17 * 60, 14 * 3600 + 32 * 60, 18 * 3600 + 55 * 60];
    for (let i = 0; i < 3; i++) {
      const idx = d * 3 + i;
      entries.push({
        id: `diary-${d.toString().padStart(2, "0")}-${i}`,
        created_at_ms: dayMidnight + offsets[i] * 1000,
        title: TITLE_POOL[idx % TITLE_POOL.length],
        body_markdown:
          `# ${TITLE_POOL[idx % TITLE_POOL.length]}\n\n` +
          `${BODY_SNIPPETS[idx % BODY_SNIPPETS.length]}\n\n` +
          `- entry index: ${idx}\n- day offset: ${d}\n`,
        tags: pickTags(idx),
        session_key: idx % 4 === 0 ? `sess-${idx.toString(16)}` : undefined,
      });
    }
  }
  // Newest first.
  entries.sort((a, b) => b.created_at_ms - a.created_at_ms);
  return entries;
}

export const MOCK_DIARY: readonly DiaryEntry[] = Object.freeze(buildEntries());

/** Returns the ISO date string (YYYY-MM-DD) in UTC for a given ms timestamp. */
export function dateKey(ms: number): string {
  const d = new Date(ms);
  const y = d.getUTCFullYear();
  const m = (d.getUTCMonth() + 1).toString().padStart(2, "0");
  const day = d.getUTCDate().toString().padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/** Returns HH:MM in UTC. Stable across timezones so test snapshots match. */
export function timeLabel(ms: number): string {
  const d = new Date(ms);
  const h = d.getUTCHours().toString().padStart(2, "0");
  const m = d.getUTCMinutes().toString().padStart(2, "0");
  return `${h}:${m}`;
}

/** Groups an already-sorted (desc) list of entries by UTC date. */
export interface DiaryDateGroup {
  date: string;
  entries: DiaryEntry[];
}

export function groupByDate(entries: readonly DiaryEntry[]): DiaryDateGroup[] {
  const groups: DiaryDateGroup[] = [];
  let cur: DiaryDateGroup | null = null;
  for (const e of entries) {
    const k = dateKey(e.created_at_ms);
    if (cur === null || cur.date !== k) {
      cur = { date: k, entries: [] };
      groups.push(cur);
    }
    cur.entries.push(e);
  }
  return groups;
}

/**
 * Thin fetcher so the page can swap to a real endpoint later without
 * touching component code. Adds a tiny delay so skeleton states are
 * observable in dev.
 */
export async function fetchDiary(): Promise<DiaryEntry[]> {
  await new Promise((r) => setTimeout(r, 40));
  return MOCK_DIARY.slice();
}
