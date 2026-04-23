import type { SchedulerJob } from "@/lib/api";

/**
 * Derived row status for a scheduler job — the backend today exposes only
 * `last_status` (free-form string) and `next_fire_at` (nullable), so we
 * derive a 3-way status the UI can filter on.
 *
 *   - `errored`  — last_status indicates failure (err / fail / …)
 *   - `paused`   — no upcoming fire time (cron inert / disabled)
 *   - `enabled`  — scheduled and not errored
 *
 * Keep the derivation pure + exported so tests and the page share the same
 * branch logic.
 */
export type SchedulerStatus = "enabled" | "paused" | "errored";

export function deriveStatus(job: SchedulerJob): SchedulerStatus {
  const ls = (job.last_status ?? "").toLowerCase();
  if (ls.includes("err") || ls.includes("fail")) return "errored";
  if (!job.next_fire_at) return "paused";
  return "enabled";
}

/** Relative-time string aligned to the plugins page helper. */
export function formatRelative(
  iso: string,
  t: (key: string, opts?: Record<string, unknown>) => string,
): string {
  try {
    const then = new Date(iso).getTime();
    if (!Number.isFinite(then)) return iso;
    const now = Date.now();
    const s = Math.round((now - then) / 1000);
    if (s < 60) return t("common.secondsAgo", { n: Math.max(s, 0) });
    if (s < 3600) return t("common.minutesAgo", { n: Math.round(s / 60) });
    if (s < 86400) return t("common.hoursAgo", { n: Math.round(s / 3600) });
    return t("common.daysAgo", { n: Math.round(s / 86400) });
  } catch {
    return iso;
  }
}

/**
 * Compact "3m 12s" / "2h 14m" / "5s" formatter for positive future deltas.
 * Returns `null` when `ms <= 0` — the caller decides how to render "due".
 */
export function formatCountdown(ms: number): string | null {
  if (!Number.isFinite(ms) || ms <= 0) return null;
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = s % 60;
  if (h >= 1) return `${h}h ${m}m`;
  if (m >= 1) return `${m}m ${ss}s`;
  return `${ss}s`;
}

/** Pick the soonest upcoming job; returns `null` if none are scheduled. */
export function pickNextUpcoming(
  jobs: readonly SchedulerJob[],
  now: number,
): { job: SchedulerJob; deltaMs: number } | null {
  let best: { job: SchedulerJob; deltaMs: number } | null = null;
  for (const job of jobs) {
    if (!job.next_fire_at) continue;
    const then = new Date(job.next_fire_at).getTime();
    if (!Number.isFinite(then)) continue;
    const delta = then - now;
    if (delta <= 0) continue;
    if (best === null || delta < best.deltaMs) {
      best = { job, deltaMs: delta };
    }
  }
  return best;
}
