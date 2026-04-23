"use client";

import * as React from "react";
import { useQuery } from "@tanstack/react-query";
import { useRouter, useSearchParams } from "next/navigation";
import { motion, useScroll, useTransform } from "framer-motion";
import { BookOpen } from "lucide-react";

import { cn } from "@/lib/utils";
import { Skeleton } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/ui/empty-state";
import { useMotion } from "@/components/ui/motion-safe";
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

  return (
    <>
      <header className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">
          Diary · 会话日记
        </h1>
        <p className="text-sm text-muted-foreground">
          Scroll-driven recap timeline. Click any entry to expand into the full
          reader. Filter by tag or date range — the URL stays in sync.
        </p>
      </header>

      {/* Filters -------------------------------------------------------- */}
      <section
        aria-label="Diary filters"
        className="flex flex-col gap-3 rounded-lg border border-border bg-card/30 px-4 py-3"
      >
        <div className="flex flex-wrap items-center gap-3">
          <label className="flex items-center gap-2 text-xs text-muted-foreground">
            From
            <input
              type="date"
              value={from}
              onChange={onFromChange}
              data-testid="diary-filter-from"
              className="rounded border border-border bg-background px-2 py-1 font-mono text-xs"
            />
          </label>
          <label className="flex items-center gap-2 text-xs text-muted-foreground">
            To
            <input
              type="date"
              value={to}
              onChange={onToChange}
              data-testid="diary-filter-to"
              className="rounded border border-border bg-background px-2 py-1 font-mono text-xs"
            />
          </label>
          {hasActiveFilter ? (
            <button
              type="button"
              onClick={onClearFilters}
              className="text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
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
                      ? "border-primary bg-primary/15 text-primary"
                      : "border-border text-muted-foreground hover:bg-accent hover:text-foreground",
                  )}
                >
                  #{t}
                </button>
              );
            })}
          </div>
        ) : null}
      </section>

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
        className="sticky top-0 z-10 -mx-2 flex items-center bg-background/85 px-2 py-2 backdrop-blur"
        style={
          reduced
            ? undefined
            : { scale, opacity, transformOrigin: "left center" }
        }
      >
        <h2 className="font-mono text-sm font-semibold tabular-nums text-foreground">
          {group.date}
        </h2>
        <span className="ml-3 rounded bg-accent/60 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
          {group.entries.length}
        </span>
      </motion.div>
      <div className="relative mt-2 border-l border-border pl-0">
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
