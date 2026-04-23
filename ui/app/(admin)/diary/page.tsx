"use client";

import * as React from "react";
import { useQuery } from "@tanstack/react-query";
import { useRouter, useSearchParams } from "next/navigation";
import { motion, useScroll, useTransform } from "framer-motion";
import { BookOpen } from "lucide-react";

import { cn } from "@/lib/utils";
import { Skeleton } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/ui/empty-state";
import { GlassPanel } from "@/components/ui/glass-panel";
import { useMotion } from "@/components/ui/motion-safe";

function timeAgo(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.round(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.round(m / 60);
  if (h < 48) return `${h}h ago`;
  return `${Math.round(h / 24)}d ago`;
}
import {
  fetchDiary,
  groupByDate,
  type DiaryDateGroup,
  type DiaryEntry,
} from "@/lib/mocks/diary";
import { DiaryEntryCard } from "./DiaryEntry";
import { DiaryReader } from "./DiaryReader";

// TODO(virtualization): at 1000+ entries switch to react-window; 30 is fine.

/**
 * Diary page (B5-FE2).
 *
 * A vertical timeline of session recaps grouped by date. Each date group is
 * a sticky header that subtly shrinks as it approaches the viewport top
 * (driven by framer-motion `useScroll`). Entries fade up one-by-one via
 * IntersectionObserver (`useInView`) and morph into a full-body reader
 * modal via shared `layoutId` when clicked. URL is synced to
 * `?tag=…&from=…&to=…`.
 *
 * Backend is not yet implemented — data comes from a local mock via
 * `fetchDiary()`. Swap to `apiFetch` once the endpoint lands.
 */
export default function DiaryPage() {
  const router = useRouter();
  const searchParams = useSearchParams();

  const tag = searchParams?.get("tag") ?? "";
  const from = searchParams?.get("from") ?? "";
  const to = searchParams?.get("to") ?? "";

  const query = useQuery<DiaryEntry[]>({
    queryKey: ["admin", "diary"],
    queryFn: fetchDiary,
  });

  // Collect the union of tags for the filter pill row.
  const allTags = React.useMemo(() => {
    const set = new Set<string>();
    for (const e of query.data ?? []) for (const t of e.tags) set.add(t);
    return Array.from(set).sort();
  }, [query.data]);

  // Apply tag + date-range filter.
  const filtered = React.useMemo(() => {
    const entries = query.data ?? [];
    return entries.filter((e) => {
      if (tag && !e.tags.includes(tag)) return false;
      if (from) {
        const fromMs = Date.parse(`${from}T00:00:00Z`);
        if (!Number.isNaN(fromMs) && e.created_at_ms < fromMs) return false;
      }
      if (to) {
        const toMs = Date.parse(`${to}T23:59:59Z`);
        if (!Number.isNaN(toMs) && e.created_at_ms > toMs) return false;
      }
      return true;
    });
  }, [query.data, tag, from, to]);

  const groups = React.useMemo<DiaryDateGroup[]>(
    () => groupByDate(filtered),
    [filtered],
  );

  const [openEntry, setOpenEntry] = React.useState<DiaryEntry | null>(null);

  // URL helpers ---------------------------------------------------------
  const writeUrl = React.useCallback(
    (next: { tag?: string; from?: string; to?: string }) => {
      const params = new URLSearchParams(searchParams?.toString() ?? "");
      const set = (k: string, v: string | undefined) => {
        if (v && v !== "") params.set(k, v);
        else params.delete(k);
      };
      if ("tag" in next) set("tag", next.tag);
      if ("from" in next) set("from", next.from);
      if ("to" in next) set("to", next.to);
      const qs = params.toString();
      router.replace(qs ? `?${qs}` : "?", { scroll: false });
    },
    [router, searchParams],
  );

  function onTagClick(t: string) {
    writeUrl({ tag: tag === t ? "" : t });
  }

  function onFromChange(e: React.ChangeEvent<HTMLInputElement>) {
    writeUrl({ from: e.target.value });
  }

  function onToChange(e: React.ChangeEvent<HTMLInputElement>) {
    writeUrl({ to: e.target.value });
  }

  function onClearFilters() {
    writeUrl({ tag: "", from: "", to: "" });
  }

  const hasActiveFilter = tag !== "" || from !== "" || to !== "";

  const entryCount = filtered.length;
  const dayCount = groups.length;
  const latest = filtered[0];
  const latestLabel = latest
    ? timeAgo(Date.now() - latest.created_at_ms)
    : null;

  return (
    <>
      <GlassPanel
        as="section"
        variant="strong"
        className="relative overflow-hidden p-7"
      >
        <div
          aria-hidden
          className="pointer-events-none absolute -bottom-20 -right-10 h-[240px] w-[360px] rounded-full opacity-60 blur-3xl"
          style={{
            background:
              "radial-gradient(closest-side, var(--tp-amber-glow), transparent 70%)",
          }}
        />
        <div className="relative flex flex-col gap-2">
          <h1 className="font-serif text-[36px] font-normal leading-[1.1] tracking-[-0.02em] text-tp-ink">
            Diary · <span className="italic">会话日记</span>
          </h1>
          <p className="max-w-[64ch] text-[14px] leading-relaxed text-tp-ink-2">
            {entryCount > 0 ? (
              <>
                <b className="font-medium text-tp-ink">{entryCount}</b> entries across{" "}
                <b className="font-medium text-tp-ink">{dayCount}</b> day
                {dayCount === 1 ? "" : "s"}.{" "}
                {latestLabel ? (
                  <>The last was <span className="text-tp-amber">{latestLabel}</span>.</>
                ) : null}{" "}
                Scroll-driven recap timeline — click any entry to expand into the full reader.
              </>
            ) : (
              <>No entries {hasActiveFilter ? "match the current filter" : "yet"}. Filter by tag or date range — the URL stays in sync.</>
            )}
          </p>
        </div>
      </GlassPanel>

      {/* Filters -------------------------------------------------------- */}
      <GlassPanel
        as="section"
        variant="soft"
        aria-label="Diary filters"
        className="flex flex-col gap-3 px-4 py-3"
      >
        <div className="flex flex-wrap items-center gap-3">
          <label className="flex items-center gap-2 text-[12px] text-tp-ink-3">
            From
            <input
              type="date"
              value={from}
              onChange={onFromChange}
              data-testid="diary-filter-from"
              className="rounded-md border border-tp-glass-edge bg-tp-glass-inner px-2 py-1 font-mono text-[11.5px] text-tp-ink-2 focus:border-tp-amber/40 focus:outline-none"
            />
          </label>
          <label className="flex items-center gap-2 text-[12px] text-tp-ink-3">
            To
            <input
              type="date"
              value={to}
              onChange={onToChange}
              data-testid="diary-filter-to"
              className="rounded-md border border-tp-glass-edge bg-tp-glass-inner px-2 py-1 font-mono text-[11.5px] text-tp-ink-2 focus:border-tp-amber/40 focus:outline-none"
            />
          </label>
          {hasActiveFilter ? (
            <button
              type="button"
              onClick={onClearFilters}
              className="text-[12px] text-tp-ink-3 underline-offset-2 hover:text-tp-ink hover:underline"
              data-testid="diary-filter-clear"
            >
              Clear
            </button>
          ) : null}
        </div>
        {allTags.length > 0 ? (
          <div className="flex flex-wrap gap-1.5" role="group" aria-label="Tag filters">
            {allTags.map((t) => {
              const active = tag === t;
              return (
                <button
                  key={t}
                  type="button"
                  onClick={() => onTagClick(t)}
                  aria-pressed={active}
                  data-testid={`diary-tag-${t}`}
                  className={cn(
                    "rounded-full border px-2.5 py-0.5 font-mono text-[11px] transition-colors",
                    active
                      ? "border-tp-amber/35 bg-tp-amber-soft text-tp-amber"
                      : "border-tp-glass-edge bg-tp-glass-inner text-tp-ink-3 hover:bg-tp-glass-inner-hover hover:text-tp-ink-2",
                  )}
                >
                  #{t}
                </button>
              );
            })}
          </div>
        ) : null}
      </GlassPanel>

      {/* Timeline ------------------------------------------------------- */}
      {query.isPending ? (
        <div className="space-y-4">
          {Array.from({ length: 3 }).map((_, i) => (
            <Skeleton key={i} className="h-24 w-full rounded-lg" />
          ))}
        </div>
      ) : query.isError ? (
        <EmptyState
          icon={<BookOpen />}
          title="Could not load diary"
          description={(query.error as Error).message}
        />
      ) : groups.length === 0 ? (
        <EmptyState
          icon={<BookOpen />}
          title="No entries match"
          description={
            hasActiveFilter
              ? "Adjust or clear the filters above to see more."
              : "No diary entries have been recorded yet."
          }
        />
      ) : (
        <div
          role="list"
          aria-label="Diary timeline"
          className="relative flex flex-col gap-6"
          data-testid="diary-timeline"
        >
          {groups.map((g) => (
            <DateGroup
              key={g.date}
              group={g}
              onOpen={setOpenEntry}
            />
          ))}
        </div>
      )}

      <DiaryReader entry={openEntry} onClose={() => setOpenEntry(null)} />
    </>
  );
}

// ---------------------------------------------------------------------------

interface DateGroupProps {
  group: DiaryDateGroup;
  onOpen: (entry: DiaryEntry) => void;
}

/**
 * One date bucket. Owns its own sticky header + scroll-driven transform so
 * each header shrinks independently as it docks at the viewport top. The
 * transform is tuned to be *subtle* — scale 1.0 → 0.85, opacity 1.0 → 0.75
 * over the final 80px of travel.
 */
function DateGroup({ group, onOpen }: DateGroupProps) {
  const { reduced } = useMotion();
  const ref = React.useRef<HTMLDivElement>(null);
  // `start end` = header bottom passes viewport bottom; `end start` = header
  // top passes viewport top. We want the header to shrink as it APPROACHES
  // the top — so the interesting window is roughly `start start` → `end
  // start`. Using those offsets keeps the transform responsive only while
  // the group is within the sticky zone.
  const { scrollYProgress } = useScroll({
    target: ref,
    offset: ["start start", "end start"],
  });
  // 0 → 1 across the group. For subtlety, we clamp the effective range to
  // the first 15% of scroll (≈ 80px on a tall block) so the shrink feels
  // locked to the dock-at-top moment, not the whole scroll.
  const scale = useTransform(scrollYProgress, [0, 0.15], [1, 0.85], {
    clamp: true,
  });
  const opacity = useTransform(scrollYProgress, [0, 0.15], [1, 0.75], {
    clamp: true,
  });

  return (
    <section ref={ref} data-testid={`diary-date-${group.date}`}>
      <motion.div
        className="sticky top-0 z-10 -mx-2 flex items-center bg-tp-glass-inner px-2 py-2 backdrop-blur-glass"
        style={
          reduced
            ? undefined
            : { scale, opacity, transformOrigin: "left center" }
        }
      >
        <h2 className="font-mono text-[13px] font-semibold tabular-nums text-tp-ink">
          {group.date}
        </h2>
        <span className="ml-3 rounded bg-tp-glass-inner-strong px-1.5 py-0.5 font-mono text-[10px] text-tp-ink-3">
          {group.entries.length}
        </span>
      </motion.div>
      <div className="relative mt-2 border-l border-tp-glass-edge pl-0">
        <div className="flex flex-col gap-3 py-1">
          {group.entries.map((e, i) => (
            <DiaryEntryCard
              key={e.id}
              entry={e}
              indexInGroup={i}
              onOpen={onOpen}
            />
          ))}
        </div>
      </div>
    </section>
  );
}
