"use client";

import * as React from "react";
import { useTranslation } from "react-i18next";
import { AnimatePresence, motion } from "framer-motion";
import { ArrowUp, Pause, Play, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { openEventStream } from "@/lib/sse";
import { useMotionVariants } from "@/lib/motion";

/**
 * Live log viewer — SSE /admin/logs/stream.
 *
 * Events are kept in a ring buffer (RING_MAX) and filtered client-side by
 * level / subsystem / substring. The stream pauses when `paused` is true.
 * Expanding a row reveals the structured fields as a tree.
 */

interface LogEvent {
  ts: string;
  level: "debug" | "info" | "warn" | "error";
  subsystem: string;
  trace_id: string;
  message: string;
  [extra: string]: unknown;
}

const RING_MAX = 500;

function levelTone(level: LogEvent["level"]) {
  switch (level) {
    case "error":
      return "text-err bg-err/10 border-err/30";
    case "warn":
      return "text-warn bg-warn/10 border-warn/30";
    case "info":
      return "text-primary bg-primary/10 border-primary/30";
    case "debug":
    default:
      return "text-muted-foreground bg-muted border-border";
  }
}

/** CSS custom-property color token for the left-rail per log level. */
function levelRailColor(level: LogEvent["level"]): string {
  switch (level) {
    case "error":
      return "var(--err)";
    case "warn":
      return "var(--warn)";
    case "info":
      return "var(--accent)";
    case "debug":
    default:
      return "var(--muted)";
  }
}

/** How long a newly-appended row keeps the pulse-glow class. */
const PULSE_MS = 400;
/** Max rail-pulse registrations per second under fire-hose bursts. */
const PULSE_MAX_PER_SEC = 30;

export default function LogsPage() {
  const { t } = useTranslation();
  const variants = useMotionVariants();
  const [events, setEvents] = React.useState<LogEvent[]>([]);
  const [levelFilter, setLevelFilter] = React.useState<string>("all");
  const [subsystemFilter, setSubsystemFilter] = React.useState<string>("");
  const [search, setSearch] = React.useState<string>("");
  const [paused, setPaused] = React.useState(false);
  const [expanded, setExpanded] = React.useState<Set<string>>(() => new Set());
  const [copiedId, setCopiedId] = React.useState<string | null>(null);
  const [recentIds, setRecentIds] = React.useState<Set<string>>(() => new Set());
  const [atBottom, setAtBottom] = React.useState(true);
  const pausedRef = React.useRef(paused);
  pausedRef.current = paused;

  // Rail-pulse throttle: cap registrations to PULSE_MAX_PER_SEC/s.
  const pulseWindowRef = React.useRef<{ start: number; count: number }>({
    start: 0,
    count: 0,
  });
  const listRef = React.useRef<HTMLUListElement | null>(null);
  // Sentinel at the "latest" edge. Because this list renders newest-first
  // (prepends), the latest row lives at the top, so the sentinel is a top
  // marker — its visibility means the user is pinned to the newest events.
  const sentinelRef = React.useRef<HTMLLIElement | null>(null);

  const registerPulse = React.useCallback((id: string) => {
    const now = Date.now();
    const win = pulseWindowRef.current;
    if (now - win.start > 1000) {
      win.start = now;
      win.count = 0;
    }
    if (win.count >= PULSE_MAX_PER_SEC) return;
    win.count += 1;
    setRecentIds((prev) => {
      const next = new Set(prev);
      next.add(id);
      return next;
    });
    window.setTimeout(() => {
      setRecentIds((prev) => {
        if (!prev.has(id)) return prev;
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }, PULSE_MS);
  }, []);

  // Keep a stable ref to `registerPulse` so the SSE effect below doesn't
  // re-subscribe on every render.
  const registerPulseRef = React.useRef(registerPulse);
  registerPulseRef.current = registerPulse;

  React.useEffect(() => {
    const close = openEventStream<LogEvent>("/admin/logs/stream", {
      events: ["log", "message"],
      onMessage: ({ data }) => {
        if (pausedRef.current) return;
        setEvents((prev) => {
          const next = [data, ...prev];
          if (next.length > RING_MAX) next.length = RING_MAX;
          return next;
        });
        // Pulse the row that will render at index 0 for this event.
        registerPulseRef.current(`${data.trace_id}-0`);
      },
      mock: (push) => {
        const id = setInterval(() => {
          push({
            event: "log",
            data: {
              ts: new Date().toISOString(),
              level: "info",
              subsystem: "gateway",
              trace_id: Math.random().toString(16).slice(2, 18),
              message: "inline mock tick",
            },
          });
        }, 1000);
        return () => clearInterval(id);
      },
    });
    return () => close();
  }, []);

  const subsystems = React.useMemo(() => {
    const s = new Set<string>();
    for (const e of events) s.add(e.subsystem);
    return Array.from(s).sort();
  }, [events]);

  const visible = React.useMemo(() => {
    const q = search.trim().toLowerCase();
    return events.filter((e) => {
      if (levelFilter !== "all" && e.level !== levelFilter) return false;
      if (subsystemFilter && !e.subsystem.includes(subsystemFilter))
        return false;
      if (q && !e.message.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [events, levelFilter, subsystemFilter, search]);

  const toggleExpand = (key: string) => {
    setExpanded((prev) => {
      const n = new Set(prev);
      if (n.has(key)) n.delete(key);
      else n.add(key);
      return n;
    });
  };

  async function copyTrace(traceId: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(traceId);
      setCopiedId(traceId);
      setTimeout(() => setCopiedId((c) => (c === traceId ? null : c)), 1200);
    } catch {
      /* clipboard unavailable */
    }
  }

  // Observe the bottom sentinel so we can surface a "jump to latest" pill when
  // the user has scrolled up and is no longer pinned to the newest entries.
  React.useEffect(() => {
    const root = listRef.current;
    const target = sentinelRef.current;
    if (!root || !target || typeof IntersectionObserver === "undefined") return;
    const io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          setAtBottom(entry.isIntersecting);
        }
      },
      { root, threshold: 0.01 },
    );
    io.observe(target);
    return () => io.disconnect();
  }, []);

  const jumpToLatest = React.useCallback(() => {
    const root = listRef.current;
    if (!root) return;
    root.scrollTo({ top: 0, behavior: "smooth" });
  }, []);

  return (
    <div className="flex flex-1 flex-col space-y-4">
      <header className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">
          {t("logs.title")}
        </h1>
        <p className="text-sm text-muted-foreground">
          {t("logs.subtitle", { max: RING_MAX })}
        </p>
      </header>

      <section className="flex flex-wrap items-center gap-2 rounded-lg border border-border bg-panel p-3">
        <select
          className="h-8 rounded-md border border-input bg-background px-2 font-mono text-xs outline-none focus-visible:ring-1 focus-visible:ring-ring"
          value={levelFilter}
          onChange={(e) => setLevelFilter(e.target.value)}
          aria-label={t("logs.levelAria")}
        >
          <option value="all">{t("logs.levelAll")}</option>
          <option value="debug">debug</option>
          <option value="info">info</option>
          <option value="warn">warn</option>
          <option value="error">error</option>
        </select>
        <select
          className="h-8 rounded-md border border-input bg-background px-2 font-mono text-xs outline-none focus-visible:ring-1 focus-visible:ring-ring"
          value={subsystemFilter}
          onChange={(e) => setSubsystemFilter(e.target.value)}
          aria-label={t("logs.subsystemAria")}
        >
          <option value="">{t("logs.subsystemAll")}</option>
          {subsystems.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
        <Input
          placeholder={t("logs.searchPlaceholder")}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="h-8 max-w-[320px] font-mono text-xs"
        />
        <div className="ml-auto flex items-center gap-2">
          <Button
            variant={paused ? "default" : "outline"}
            size="sm"
            onClick={() => setPaused((p) => !p)}
          >
            {paused ? (
              <Play className="h-3 w-3" />
            ) : (
              <Pause className="h-3 w-3" />
            )}
            {paused ? t("logs.resume") : t("logs.pause")}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setEvents([])}
            disabled={events.length === 0}
          >
            <Trash2 className="h-3 w-3" />
            {t("logs.clear")}
          </Button>
          <span className="font-mono text-[11px] text-muted-foreground">
            {visible.length} / {events.length}
          </span>
        </div>
      </section>

      <section className="relative flex-1 overflow-hidden rounded-lg border border-border bg-panel">
        <ul
          ref={listRef}
          className="max-h-[70vh] divide-y divide-border overflow-auto font-mono text-xs"
        >
          {/* Top sentinel — its visibility means the user is pinned to the
              newest entries (this list is rendered newest-first). */}
          <li
            ref={sentinelRef}
            aria-hidden="true"
            className="h-px w-full"
          />
          {visible.length === 0 ? (
            <li className="p-6 text-center text-sm text-muted-foreground">
              {paused ? t("logs.paused") : t("logs.waitingForEvents")}
            </li>
          ) : (
            visible.map((e, i) => {
              const key = `${e.trace_id}-${i}`;
              const isExpanded = expanded.has(key);
              const isRecent = recentIds.has(key);
              const extras = Object.entries(e).filter(
                ([k]) =>
                  !["ts", "level", "subsystem", "trace_id", "message"].includes(
                    k,
                  ),
              );
              return (
                <li
                  key={key}
                  className="relative transition-colors hover:bg-accent/20"
                >
                  <div
                    aria-hidden="true"
                    className={cn(
                      "absolute left-0 top-0 h-full w-[2px]",
                      isRecent && "animate-pulse-glow",
                    )}
                    style={{ backgroundColor: levelRailColor(e.level) }}
                  />
                  <div
                    className="flex items-start gap-2 px-3 py-2 pl-4 cursor-pointer"
                    role="button"
                    tabIndex={0}
                    onClick={() => extras.length > 0 && toggleExpand(key)}
                    onKeyDown={(ev) => {
                      if ((ev.key === "Enter" || ev.key === " ") && extras.length > 0) {
                        ev.preventDefault();
                        toggleExpand(key);
                      }
                    }}
                  >
                    <span className="shrink-0 text-muted-foreground">
                      {e.ts.slice(11, 23)}
                    </span>
                    <span
                      className={cn(
                        "shrink-0 rounded border px-1 text-[10px] font-semibold uppercase tracking-wider",
                        levelTone(e.level),
                      )}
                    >
                      {e.level}
                    </span>
                    <span className="shrink-0 text-muted-foreground">
                      {e.subsystem}
                    </span>
                    <button
                      type="button"
                      onClick={(ev) => {
                        ev.stopPropagation();
                        copyTrace(e.trace_id);
                      }}
                      className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                      title={t("logs.copyTrace")}
                    >
                      {copiedId === e.trace_id
                        ? t("logs.copied")
                        : e.trace_id.slice(0, 8)}
                    </button>
                    <span className="flex-1 whitespace-pre-wrap break-all text-foreground">
                      {e.message}
                    </span>
                    {extras.length > 0 ? (
                      <span className="shrink-0 text-[10px] text-muted-foreground">
                        {isExpanded ? "▾" : "▸"} {extras.length}
                      </span>
                    ) : null}
                  </div>
                  {isExpanded && extras.length > 0 ? (
                    <pre className="overflow-auto bg-surface/60 px-10 py-2 text-[10px] text-muted-foreground">
                      {JSON.stringify(Object.fromEntries(extras), null, 2)}
                    </pre>
                  ) : null}
                </li>
              );
            })
          )}
        </ul>
        <AnimatePresence>
          {!atBottom ? (
            <motion.button
              key="jump-to-latest"
              type="button"
              onClick={jumpToLatest}
              aria-label="Jump to latest log entry"
              variants={variants.springPop}
              initial="hidden"
              animate="visible"
              exit="hidden"
              className={cn(
                "absolute bottom-4 right-4 inline-flex items-center gap-1.5",
                "rounded-full border border-border bg-panel/95 px-3 py-1.5",
                "text-xs font-medium shadow-2 backdrop-blur",
                "transition-colors hover:bg-accent/40",
                "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
              )}
            >
              <ArrowUp className="h-3 w-3" />
              {t("logs.jumpToLatest", { defaultValue: "Jump to latest" })}
            </motion.button>
          ) : null}
        </AnimatePresence>
      </section>
    </div>
  );
}
