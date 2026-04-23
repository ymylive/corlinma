"use client";

import * as React from "react";
import { AnimatePresence, motion } from "framer-motion";
import { useRouter, useSearchParams } from "next/navigation";

import { LiveDot } from "@/components/ui/live-dot";
import { AnimatedNumber } from "@/components/ui/animated-number";
import { EmptyState } from "@/components/ui/empty-state";
import { useMotion } from "@/components/ui/motion-safe";
import { cn } from "@/lib/utils";
import {
  useMockHookStream,
  ALL_HOOK_KINDS,
  kindCategory,
  type HookCategory,
  type HookEvent,
  type HookEventKind,
} from "@/lib/hooks/use-mock-hook-stream";
import { EventRow, eventColor } from "@/components/hooks/event-row";
import { EventSparkline } from "@/components/hooks/event-sparkline";

const ALL_KINDS: HookEventKind[] = ALL_HOOK_KINDS;

const CATEGORY_ORDER: HookCategory[] = [
  "all",
  "message",
  "session",
  "agent",
  "lifecycle",
  "approval",
  "rate_limit",
  "tool",
  "config",
];

const CATEGORY_LABELS: Record<HookCategory, string> = {
  all: "All",
  message: "Messages",
  session: "Session",
  agent: "Agent",
  lifecycle: "Lifecycle",
  approval: "Approval",
  rate_limit: "RateLimit",
  tool: "Tool",
  config: "Config",
};

function kindsForCategory(cat: HookCategory): HookEventKind[] {
  if (cat === "all") return ALL_KINDS;
  return ALL_KINDS.filter((k) => kindCategory(k) === cat);
}

function parseExcluded(raw: string | null): Set<string> {
  if (!raw) return new Set();
  return new Set(
    raw
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean),
  );
}

function serializeExcluded(set: Set<string>): string | null {
  if (set.size === 0) return null;
  return Array.from(set).sort().join(",");
}

function parseCategory(raw: string | null): HookCategory {
  const valid: HookCategory[] = CATEGORY_ORDER;
  if (raw && (valid as string[]).includes(raw)) return raw as HookCategory;
  return "all";
}

const ALERT_STORAGE_KEY = "corlinman.hooks.alert-approvals.v1";

/** Hooks · Event Stream — admin observability page. */
export default function HooksPage() {
  const { events, connected, eps, epsHistory } = useMockHookStream();
  const { reduced, motionSafe } = useMotion();
  const router = useRouter();
  const searchParams = useSearchParams();
  const excluded = React.useMemo(
    () => parseExcluded(searchParams?.get("exclude") ?? null),
    [searchParams],
  );
  const category = React.useMemo(
    () => parseCategory(searchParams?.get("cat") ?? null),
    [searchParams],
  );

  // Persisted "Alert on approval requests" checkbox.
  const [alertApprovals, setAlertApprovals] = React.useState(false);
  React.useEffect(() => {
    if (typeof window === "undefined") return;
    try {
      const raw = window.localStorage.getItem(ALERT_STORAGE_KEY);
      if (raw === "1") setAlertApprovals(true);
    } catch {
      /* ignore */
    }
  }, []);
  const toggleAlert = React.useCallback(() => {
    setAlertApprovals((prev) => {
      const next = !prev;
      try {
        window.localStorage.setItem(ALERT_STORAGE_KEY, next ? "1" : "0");
      } catch {
        /* ignore */
      }
      return next;
    });
  }, []);

  const updateQuery = React.useCallback(
    (mutator: (params: URLSearchParams) => void) => {
      const params = new URLSearchParams(searchParams?.toString() ?? "");
      mutator(params);
      const query = params.toString();
      router.replace(`/hooks${query ? `?${query}` : ""}`, { scroll: false });
    },
    [router, searchParams],
  );

  const setExcluded = React.useCallback(
    (next: Set<string>) => {
      updateQuery((params) => {
        const serialized = serializeExcluded(next);
        if (serialized) params.set("exclude", serialized);
        else params.delete("exclude");
      });
    },
    [updateQuery],
  );

  const setCategory = React.useCallback(
    (next: HookCategory) => {
      updateQuery((params) => {
        if (next === "all") params.delete("cat");
        else params.set("cat", next);
      });
    },
    [updateQuery],
  );

  const toggleKind = React.useCallback(
    (kind: HookEventKind) => {
      const next = new Set(excluded);
      if (next.has(kind)) next.delete(kind);
      else next.add(kind);
      setExcluded(next);
    },
    [excluded, setExcluded],
  );

  const visibleKinds = React.useMemo(
    () => kindsForCategory(category),
    [category],
  );
  const visibleKindSet = React.useMemo(
    () => new Set<string>(visibleKinds),
    [visibleKinds],
  );

  const visibleEvents = React.useMemo(
    () =>
      events.filter(
        (e) => visibleKindSet.has(e.kind) && !excluded.has(e.kind),
      ),
    [events, visibleKindSet, excluded],
  );

  // Track newly arrived approval.requested event ids for this session, so
  // <EventRow> only gets the one-shot `alertBoost` flag on the row that just
  // arrived (not on every re-render).
  const seenApprovalIdsRef = React.useRef<Set<string>>(new Set());
  const [boostedIds, setBoostedIds] = React.useState<Set<string>>(new Set());
  React.useEffect(() => {
    if (!alertApprovals) return;
    let changed = false;
    const nextBoosted = new Set(boostedIds);
    for (const evt of events) {
      if (evt.kind !== "approval.requested") continue;
      if (seenApprovalIdsRef.current.has(evt.id)) continue;
      seenApprovalIdsRef.current.add(evt.id);
      nextBoosted.add(evt.id);
      changed = true;
      // Clear boost after 800ms so layout stabilises.
      setTimeout(() => {
        setBoostedIds((prev) => {
          if (!prev.has(evt.id)) return prev;
          const copy = new Set(prev);
          copy.delete(evt.id);
          return copy;
        });
      }, 800);
    }
    if (changed) setBoostedIds(nextBoosted);
    // `boostedIds` intentionally omitted — we read from ref + functional update.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [events, alertApprovals]);

  // Ripple: fire a single-shot animation whenever the connection flips
  // disconnected → connected.
  const prevConnectedRef = React.useRef(connected);
  const [rippleKey, setRippleKey] = React.useState(0);
  React.useEffect(() => {
    if (!prevConnectedRef.current && connected) {
      setRippleKey((k) => k + 1);
    }
    prevConnectedRef.current = connected;
  }, [connected]);

  // Aggregate per-kind counts across the full event buffer.
  const kindCounts = React.useMemo(() => {
    const counts = new Map<HookEventKind, number>();
    for (const k of ALL_KINDS) counts.set(k, 0);
    for (const e of events) {
      counts.set(e.kind, (counts.get(e.kind) ?? 0) + 1);
    }
    return counts;
  }, [events]);

  // Pending approvals: approval.requested ids not yet resolved by a matching
  // approval.decided in the buffer.
  const pendingApprovalCount = React.useMemo(() => {
    const decided = new Set<string>();
    for (const e of events) {
      if (e.kind === "approval.decided" && typeof e.payload?.id === "string") {
        decided.add(e.payload.id);
      }
    }
    let pending = 0;
    for (const e of events) {
      if (e.kind !== "approval.requested") continue;
      const id = e.payload?.id;
      if (typeof id !== "string" || !decided.has(id)) pending += 1;
    }
    return pending;
  }, [events]);

  // Rate-limit triggers in last 60s. Recomputed on every render — tiny buffer.
  const [statsNow, setStatsNow] = React.useState(() => Date.now());
  React.useEffect(() => {
    const id = setInterval(() => setStatsNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);
  const rateLimitLast60s = React.useMemo(() => {
    const cutoff = statsNow - 60_000;
    return events.filter(
      (e) => e.kind === "rate_limit.triggered" && e.ts >= cutoff,
    ).length;
  }, [events, statsNow]);

  // Tool call success rate across the last 60 `tool.called` events.
  const toolSuccessRate = React.useMemo(() => {
    const recent = events.filter((e) => e.kind === "tool.called").slice(0, 60);
    if (recent.length === 0) return null;
    const ok = recent.filter((e) => e.payload?.ok === true).length;
    return ok / recent.length;
  }, [events]);

  // Tablist keyboard arrow navigation.
  const onCategoryKeyDown = React.useCallback(
    (e: React.KeyboardEvent<HTMLButtonElement>, idx: number) => {
      if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
      e.preventDefault();
      const delta = e.key === "ArrowRight" ? 1 : -1;
      const nextIdx =
        (idx + delta + CATEGORY_ORDER.length) % CATEGORY_ORDER.length;
      const next = CATEGORY_ORDER[nextIdx]!;
      setCategory(next);
      // Focus the new tab after the route replace settles.
      requestAnimationFrame(() => {
        const el = document.querySelector<HTMLButtonElement>(
          `[data-category-tab="${next}"]`,
        );
        el?.focus();
      });
    },
    [setCategory],
  );

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-wrap items-center justify-between gap-4">
        <div className="space-y-1">
          <h1 className="text-2xl font-semibold tracking-tight">
            Hooks · Event Stream
          </h1>
          <p className="text-sm text-muted-foreground">
            Live firehose of gateway + agent lifecycle events.
          </p>
        </div>
        <div className="flex items-center gap-4">
          <label className="flex cursor-pointer items-center gap-1.5 text-xs text-muted-foreground">
            <input
              type="checkbox"
              checked={alertApprovals}
              onChange={toggleAlert}
              className="h-3.5 w-3.5 accent-warn"
              data-testid="alert-approvals-toggle"
            />
            Alert on approval requests
          </label>
          <div className="relative flex items-center gap-2 rounded-md border border-border bg-card/60 px-3 py-1.5">
            <LiveDot
              variant={connected ? "ok" : "err"}
              pulse
              label={connected ? "Live" : "Reconnecting"}
            />
            <span className="text-xs font-medium">
              {connected ? "Live" : "Reconnecting"}
            </span>
            {!reduced ? (
              <AnimatePresence>
                <motion.span
                  key={rippleKey}
                  aria-hidden="true"
                  initial={{ scale: 0, opacity: 0.4 }}
                  animate={{ scale: 8, opacity: 0 }}
                  transition={{ duration: 0.8, ease: "easeOut" }}
                  className="pointer-events-none absolute left-3 top-1/2 h-2 w-2 -translate-y-1/2 rounded-full bg-ok"
                />
              </AnimatePresence>
            ) : null}
          </div>
          <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <AnimatedNumber
              value={eps}
              format="number"
              formatOptions={{ maximumFractionDigits: 1 }}
              className="font-mono text-sm text-foreground"
              data-testid="eps"
            />
            <span>events / s</span>
          </div>
        </div>
      </header>

      {/* Category filter row — tablist with arrow-key nav. */}
      <div
        role="tablist"
        aria-label="Event category filter"
        className="flex flex-wrap gap-1.5"
      >
        {CATEGORY_ORDER.map((cat, idx) => {
          const selected = category === cat;
          return (
            <button
              key={cat}
              type="button"
              role="tab"
              aria-selected={selected}
              tabIndex={selected ? 0 : -1}
              data-category-tab={cat}
              data-testid={`category-${cat}`}
              onClick={() => setCategory(cat)}
              onKeyDown={(e) => onCategoryKeyDown(e, idx)}
              className={cn(
                "rounded-full border px-2.5 py-0.5 font-mono text-[11px] transition-colors",
                selected
                  ? "border-accent bg-accent/30 text-foreground"
                  : "border-border bg-card text-muted-foreground hover:bg-accent/20",
              )}
            >
              {CATEGORY_LABELS[cat]}
            </button>
          );
        })}
      </div>

      <div className="grid gap-4 lg:grid-cols-[70fr_30fr]">
        <section
          aria-label="Hook event stream"
          className="flex min-h-[480px] flex-col rounded-lg border border-border bg-card/40"
        >
          <div className="border-b border-border px-4 py-2 text-xs uppercase tracking-wider text-muted-foreground">
            Recent events · {visibleEvents.length} shown
          </div>
          <div className="flex-1 overflow-y-auto p-3">
            {visibleEvents.length === 0 ? (
              <EmptyState
                title="No events yet"
                description="Waiting for the first hook to fire…"
              />
            ) : (
              <ol
                role="log"
                aria-live="polite"
                aria-atomic="false"
                className="flex flex-col gap-2"
              >
                <AnimatePresence initial={false}>
                  {visibleEvents.map((evt: HookEvent) => (
                    <EventRow
                      key={evt.id}
                      event={evt}
                      alertBoost={
                        alertApprovals &&
                        motionSafe &&
                        boostedIds.has(evt.id) &&
                        evt.kind === "approval.requested"
                      }
                    />
                  ))}
                </AnimatePresence>
              </ol>
            )}
          </div>
        </section>

        <aside className="flex flex-col gap-4">
          <div className="rounded-lg border border-border bg-card/40 p-4">
            <div className="mb-2 text-xs uppercase tracking-wider text-muted-foreground">
              Events / second · last 60s
            </div>
            <EventSparkline
              samples={epsHistory}
              width={240}
              height={48}
              className="w-full"
              label={`EPS sparkline, current ${eps.toFixed(1)}`}
            />
          </div>

          <div className="rounded-lg border border-border bg-card/40 p-4">
            <div className="mb-3 text-xs uppercase tracking-wider text-muted-foreground">
              Filter kinds
            </div>
            <div className="flex flex-wrap gap-1.5">
              {visibleKinds.map((kind) => {
                const muted = excluded.has(kind);
                const colour = eventColor(kind);
                return (
                  <button
                    key={kind}
                    type="button"
                    role="checkbox"
                    aria-checked={!muted}
                    onClick={() => toggleKind(kind)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        toggleKind(kind);
                      }
                    }}
                    className={cn(
                      "rounded-full border px-2 py-0.5 font-mono text-[10px] transition-colors",
                      muted
                        ? "border-border bg-muted text-muted-foreground line-through"
                        : "border-border bg-card text-foreground hover:bg-accent/40",
                      !muted && colour === "accent" && "border-accent/60",
                      !muted && colour === "accent-2" && "border-accent-2/60",
                      !muted && colour === "accent-3" && "border-accent-3/60",
                      !muted && colour === "ok" && "border-ok/60",
                      !muted && colour === "warn" && "border-warn/60",
                      !muted && colour === "err" && "border-err/60",
                    )}
                    data-testid={`filter-${kind}`}
                  >
                    {kind}
                  </button>
                );
              })}
            </div>
          </div>

          <div className="rounded-lg border border-border bg-card/40 p-4">
            <div className="mb-3 text-xs uppercase tracking-wider text-muted-foreground">
              Stats
            </div>
            <dl className="space-y-1.5 text-xs">
              <div className="flex items-baseline justify-between">
                <dt className="text-muted-foreground">Total buffered</dt>
                <dd className="font-mono tabular-nums">{events.length}</dd>
              </div>
              <div className="flex items-baseline justify-between">
                <dt className="text-muted-foreground">Visible</dt>
                <dd className="font-mono tabular-nums">
                  {visibleEvents.length}
                </dd>
              </div>
              <div className="flex items-baseline justify-between">
                <dt className="text-muted-foreground">EPS (5s avg)</dt>
                <dd className="font-mono tabular-nums">{eps.toFixed(2)}</dd>
              </div>
              <div
                className="flex items-baseline justify-between"
                data-testid="stat-pending-approvals"
              >
                <dt className="text-muted-foreground">Pending approvals</dt>
                <dd className="font-mono tabular-nums">
                  {pendingApprovalCount}
                </dd>
              </div>
              <div
                className="flex items-baseline justify-between"
                data-testid="stat-rate-limits-60s"
              >
                <dt className="text-muted-foreground">Rate-limit · 60s</dt>
                <dd className="font-mono tabular-nums">{rateLimitLast60s}</dd>
              </div>
              <div
                className="flex items-baseline justify-between"
                data-testid="stat-tool-success"
              >
                <dt className="text-muted-foreground">Tool success · last 60</dt>
                <dd className="font-mono tabular-nums">
                  {toolSuccessRate === null
                    ? "—"
                    : `${(toolSuccessRate * 100).toFixed(0)}%`}
                </dd>
              </div>
            </dl>
            <div className="mt-3 max-h-40 space-y-1 overflow-y-auto border-t border-border pt-2">
              {ALL_KINDS.map((kind) => (
                <div
                  key={kind}
                  className="flex items-baseline justify-between text-[11px]"
                >
                  <span className="truncate font-mono text-muted-foreground">
                    {kind}
                  </span>
                  <span className="font-mono tabular-nums">
                    {kindCounts.get(kind) ?? 0}
                  </span>
                </div>
              ))}
            </div>
          </div>
        </aside>
      </div>
    </div>
  );
}
