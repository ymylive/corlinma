"use client";

import * as React from "react";
import { ChevronDown, ChevronUp, X } from "lucide-react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/utils";
import { GlassPanel } from "@/components/ui/glass-panel";
import { JsonView } from "@/components/ui/json-view";
import {
  FilterChipGroup,
  type FilterChipOption,
} from "@/components/ui/filter-chip-group";
import type { CanvasEvent, CanvasFrameKind } from "@/lib/mocks/canvas";

/**
 * Canvas-page protocol inspector (Tidepool cutover).
 *
 * Shares DNA with the Logs page's `<LogsControlBar>` + `<LogRow>` stack but
 * collapsed into a single bottom-docked glass-soft panel so the Canvas page
 * stays compact. Rows are dense (font-mono 12.5px, tight padding); clicking a
 * row selects it and reveals a `<JsonView>` payload block in the side detail
 * panel when the viewport is wide, or inline-expanded underneath the row on
 * narrow screens.
 *
 * Must-keep contract (from the legacy inspector):
 *   - outer panel carries `data-testid="canvas-inspector"`
 *   - the toggle carries `data-testid="canvas-inspector-toggle"`
 *   - each rendered event row is a `<li>` (page.test.tsx counts them)
 *   - when ended, a 'session ended' badge renders on the header bar
 */

// ─── filter categories ───────────────────────────────────────────────

export type FrameFilter =
  | "all"
  | "a2ui"
  | "present"
  | "navigate"
  | "eval"
  | "snapshot"
  | "hide";

/** Map a raw frame kind onto a coarse filter bucket. */
export function frameCategory(kind: CanvasFrameKind): FrameFilter {
  if (kind === "a2ui_push" || kind === "a2ui_reset") return "a2ui";
  if (kind === "present") return "present";
  if (kind === "navigate") return "navigate";
  if (kind === "eval") return "eval";
  if (kind === "snapshot") return "snapshot";
  return "hide";
}

// ─── kind palette (matches the legacy inspector colour coding) ───────

interface KindStyle {
  pillClass: string;
  dotClass: string;
  ringClass: string;
}

function kindStyle(kind: CanvasFrameKind): KindStyle {
  switch (kind) {
    case "present":
    case "navigate":
      return {
        pillClass: "bg-tp-glass-inner-strong text-tp-ink-2 border-tp-glass-edge",
        dotClass: "bg-tp-ink-3",
        ringClass: "bg-tp-ink-3/40",
      };
    case "a2ui_push":
    case "a2ui_reset":
      return {
        pillClass: "bg-tp-amber-soft text-tp-amber border-tp-amber/30",
        dotClass: "bg-tp-amber",
        ringClass: "bg-tp-amber/40",
      };
    case "eval":
    case "snapshot":
      return {
        pillClass:
          "bg-[color-mix(in_oklch,var(--tp-ember)_14%,transparent)] text-tp-ember border-[color-mix(in_oklch,var(--tp-ember)_32%,transparent)]",
        dotClass: "bg-tp-ember",
        ringClass: "bg-tp-ember/40",
      };
    case "hide":
    default:
      return {
        pillClass: "bg-tp-glass-inner text-tp-ink-4 border-tp-glass-edge",
        dotClass: "bg-tp-ink-4",
        ringClass: "bg-tp-ink-4/40",
      };
  }
}

// ─── size heuristic ───────────────────────────────────────────────────

function approxBytes(payload: Record<string, unknown>): number {
  try {
    return JSON.stringify(payload).length;
  } catch {
    return 0;
  }
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n}B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)}KB`;
  return `${(n / (1024 * 1024)).toFixed(1)}MB`;
}

// ─── direction heuristic ──────────────────────────────────────────────

/** Sent-by-user frames are tagged `*_sent` in their id by the page; everything
 * else comes off the wire (in-bound). */
function frameDirection(ev: CanvasEvent): "in" | "out" {
  return ev.id.endsWith("_sent") ? "out" : "in";
}

// ─── component ────────────────────────────────────────────────────────

export interface MessageInspectorProps {
  events: CanvasEvent[];
  /** Newest id, used to pulse the just-landed row. */
  newestId?: string;
  /** Show the 'session ended' badge. */
  ended?: boolean;
  /** Respects the OS-level reduced-motion preference. */
  reduced: boolean;
}

export function MessageInspector({
  events,
  newestId,
  ended,
  reduced,
}: MessageInspectorProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = React.useState(false);
  const [filter, setFilter] = React.useState<FrameFilter>("all");
  const [selectedId, setSelectedId] = React.useState<string | null>(null);

  // Close the detail on Esc.
  React.useEffect(() => {
    if (selectedId === null) return;
    function onKey(ev: KeyboardEvent) {
      if (ev.key === "Escape") setSelectedId(null);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selectedId]);

  // Screen-reader announcement for collapse/expand.
  const [announce, setAnnounce] = React.useState("");
  React.useEffect(() => {
    setAnnounce(
      expanded
        ? t("canvas.inspectorExpanded")
        : t("canvas.inspectorCollapsed"),
    );
  }, [expanded, t]);

  // ── filtered visible rows ──
  const visible = React.useMemo(() => {
    if (filter === "all") return events;
    return events.filter((e) => frameCategory(e.kind) === filter);
  }, [events, filter]);

  // ── counts for the chip group ──
  const counts = React.useMemo(() => {
    const acc: Partial<Record<FrameFilter, number>> = { all: events.length };
    for (const e of events) {
      const c = frameCategory(e.kind);
      acc[c] = (acc[c] ?? 0) + 1;
    }
    return acc;
  }, [events]);

  const filterOptions: FilterChipOption[] = React.useMemo(
    () => [
      {
        value: "all",
        label: t("canvas.tp.filterAll"),
        count: counts.all ?? 0,
      },
      {
        value: "a2ui",
        label: t("canvas.tp.filterA2ui"),
        count: counts.a2ui ?? 0,
      },
      {
        value: "present",
        label: t("canvas.tp.filterPresent"),
        count: counts.present ?? 0,
      },
      {
        value: "navigate",
        label: t("canvas.tp.filterNavigate"),
        count: counts.navigate ?? 0,
      },
      {
        value: "eval",
        label: t("canvas.tp.filterEval"),
        count: counts.eval ?? 0,
      },
      {
        value: "snapshot",
        label: t("canvas.tp.filterSnapshot"),
        count: counts.snapshot ?? 0,
      },
      {
        value: "hide",
        label: t("canvas.tp.filterHide"),
        count: counts.hide ?? 0,
      },
    ],
    [counts, t],
  );

  const selected = React.useMemo(
    () => events.find((e) => e.id === selectedId) ?? null,
    [events, selectedId],
  );

  return (
    <GlassPanel
      as="section"
      variant="soft"
      className="flex flex-col overflow-hidden"
      data-testid="canvas-inspector"
    >
      {/* ── Header bar with toggle + title + ended badge ── */}
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        aria-expanded={expanded}
        aria-controls="canvas-inspector-log"
        className={cn(
          "flex h-11 w-full shrink-0 items-center justify-between gap-2 px-4",
          "text-[12.5px] text-tp-ink-2 transition-colors",
          "hover:bg-tp-glass-inner-hover hover:text-tp-ink",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-tp-amber/40",
        )}
        data-testid="canvas-inspector-toggle"
      >
        <span className="inline-flex items-center gap-2 font-medium">
          {expanded ? (
            <ChevronDown className="h-3.5 w-3.5" aria-hidden />
          ) : (
            <ChevronUp className="h-3.5 w-3.5" aria-hidden />
          )}
          {t("canvas.inspectorTitle")}
          <span className="ml-2 tabular-nums font-mono text-[11px] text-tp-ink-4">
            {events.length}
          </span>
        </span>
        {ended ? (
          <span
            className={cn(
              "inline-flex items-center gap-1 rounded-full border px-2 py-0.5",
              "font-mono text-[10px] uppercase tracking-wider",
              "border-tp-glass-edge bg-tp-glass-inner text-tp-ink-4",
            )}
            data-testid="canvas-session-ended"
          >
            {t("canvas.sessionEnded")}
          </span>
        ) : null}
      </button>

      <span className="sr-only" role="status" aria-live="polite">
        {announce}
      </span>

      {expanded ? (
        <div
          id="canvas-inspector-log"
          role="log"
          aria-live="polite"
          aria-label={t("canvas.tp.inspectorStreamAria")}
          className="flex flex-col border-t border-tp-glass-edge"
        >
          {/* Filter chips */}
          <div className="flex flex-wrap items-center gap-2 border-b border-tp-glass-edge bg-tp-glass-inner/60 px-4 py-2.5">
            <FilterChipGroup
              label={t("canvas.tp.filterLabel")}
              options={filterOptions}
              value={filter}
              onChange={(v) => setFilter(v as FrameFilter)}
            />
          </div>

          {/* Grid header */}
          <div
            aria-hidden
            className={cn(
              "grid items-center gap-3 border-b border-tp-glass-edge px-4 py-2",
              "grid-cols-[76px_84px_36px_56px_minmax(0,1fr)_20px]",
              "font-mono text-[10px] uppercase tracking-[0.08em] text-tp-ink-4",
            )}
          >
            <span>{t("canvas.tp.inspectorColTime")}</span>
            <span>{t("canvas.tp.inspectorColKind")}</span>
            <span>{t("canvas.tp.inspectorColDir")}</span>
            <span>{t("canvas.tp.inspectorColSize")}</span>
            <span>{t("canvas.tp.inspectorColPreview")}</span>
            <span />
          </div>

          {/* Main content grid — list + side detail at wide viewports */}
          <div
            className={cn(
              "grid min-h-0 gap-0",
              selected
                ? "lg:grid-cols-[minmax(0,1fr)_380px]"
                : "lg:grid-cols-[minmax(0,1fr)]",
            )}
            style={{ maxHeight: "46vh" }}
          >
            {/* Row list */}
            <ul
              className="flex max-h-[46vh] flex-col overflow-y-auto"
              aria-label={t("canvas.tp.inspectorStreamAria")}
            >
              {visible.length === 0 ? (
                <li className="px-4 py-6 text-center text-[12.5px] text-tp-ink-3">
                  {events.length === 0
                    ? t("canvas.inspectorEmpty")
                    : t("canvas.tp.inspectorEmptyFiltered")}
                </li>
              ) : (
                visible.map((ev) => (
                  <InspectorRow
                    key={ev.id}
                    ev={ev}
                    isNewest={ev.id === newestId}
                    isSelected={ev.id === selectedId}
                    onSelect={() =>
                      setSelectedId((prev) => (prev === ev.id ? null : ev.id))
                    }
                    reduced={reduced}
                    wideSelected={selected !== null}
                  />
                ))
              )}
            </ul>

            {/* Side detail — desktop only; inline expand handles narrow. */}
            {selected ? (
              <aside
                className={cn(
                  "hidden border-tp-glass-edge lg:flex lg:flex-col lg:border-l",
                  "bg-tp-glass-inner/40",
                )}
              >
                <div className="flex items-center justify-between gap-2 border-b border-tp-glass-edge px-4 py-2.5">
                  <div className="flex min-w-0 flex-col gap-0.5">
                    <div className="font-mono text-[10px] uppercase tracking-[0.1em] text-tp-ink-4">
                      {t("canvas.tp.detailMeta")}
                    </div>
                    <div className="flex items-center gap-2">
                      <span
                        className={cn(
                          "inline-flex items-center rounded border px-1.5 py-px",
                          "font-mono text-[10px] font-medium",
                          kindStyle(selected.kind).pillClass,
                        )}
                      >
                        {selected.kind}
                      </span>
                      <span className="font-mono text-[11px] text-tp-ink-3">
                        {selected.timestamp}
                      </span>
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={() => setSelectedId(null)}
                    aria-label={t("canvas.tp.detailClose")}
                    className={cn(
                      "inline-flex h-7 w-7 items-center justify-center rounded-md border",
                      "border-tp-glass-edge bg-tp-glass-inner text-tp-ink-3",
                      "hover:bg-tp-glass-inner-hover hover:text-tp-ink",
                      "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-tp-amber/40",
                    )}
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                </div>
                <div className="flex flex-col gap-2 overflow-y-auto p-4">
                  <div className="font-mono text-[10px] uppercase tracking-[0.1em] text-tp-ink-4">
                    {t("canvas.tp.detailPayload")}
                  </div>
                  <JsonView value={selected.payload} />
                </div>
              </aside>
            ) : null}
          </div>
        </div>
      ) : null}
    </GlassPanel>
  );
}

// ─── row ───────────────────────────────────────────────────────────────

interface InspectorRowProps {
  ev: CanvasEvent;
  isNewest: boolean;
  isSelected: boolean;
  reduced: boolean;
  wideSelected: boolean;
  onSelect: () => void;
}

const TRUNCATE_LEN = 120;

function InspectorRow({
  ev,
  isNewest,
  isSelected,
  reduced,
  wideSelected,
  onSelect,
}: InspectorRowProps) {
  const style = kindStyle(ev.kind);
  const payloadStr = React.useMemo(
    () => JSON.stringify(ev.payload),
    [ev.payload],
  );
  const preview =
    payloadStr.length > TRUNCATE_LEN
      ? `${payloadStr.slice(0, TRUNCATE_LEN)}…`
      : payloadStr;

  const showPulse = isNewest && !reduced;
  const [pulseGate, setPulseGate] = React.useState(showPulse);
  React.useEffect(() => {
    if (!showPulse) {
      setPulseGate(false);
      return;
    }
    setPulseGate(true);
    const id = window.setTimeout(() => setPulseGate(false), 500);
    return () => window.clearTimeout(id);
  }, [showPulse, ev.id]);

  const bytes = approxBytes(ev.payload);
  const dir = frameDirection(ev);

  // When a detail drawer is open on wide viewports, skip the inline expand —
  // the drawer wins. On narrow viewports the drawer is hidden, so inline
  // expansion is the only way to see the full payload.
  const inlineExpand = isSelected && !wideSelected;

  return (
    <li
      data-testid={`canvas-event-${ev.id}`}
      data-kind={ev.kind}
      className={cn(
        "flex flex-col border-b border-tp-glass-edge",
        "transition-colors",
      )}
    >
      <button
        type="button"
        onClick={onSelect}
        aria-expanded={isSelected}
        className={cn(
          "relative grid w-full items-center gap-3 px-4 py-2 text-left",
          "grid-cols-[76px_84px_36px_56px_minmax(0,1fr)_20px]",
          "font-mono text-[11.5px] leading-relaxed",
          "hover:bg-tp-glass-inner-hover focus-visible:outline-none",
          "focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-tp-amber/40",
          isSelected && "bg-tp-amber-soft",
          isSelected &&
            "shadow-[inset_2px_0_0_var(--tp-amber)]",
        )}
      >
        {/* just-now left edge */}
        {pulseGate && !isSelected ? (
          <span
            aria-hidden
            className="absolute left-0 top-1.5 bottom-1.5 w-0.5 rounded-sm bg-tp-amber shadow-[0_0_8px_var(--tp-amber-glow)] tp-just-now"
          />
        ) : null}

        <span className="tabular-nums text-tp-ink-4">{ev.timestamp}</span>

        <span className="inline-flex w-fit">
          <span
            className={cn(
              "relative inline-flex items-center gap-1 rounded border px-1.5 py-px",
              "text-[10px] font-medium",
              style.pillClass,
            )}
            data-testid={`canvas-event-kind-${ev.id}`}
          >
            <span
              aria-hidden
              className={cn("inline-block h-1 w-1 rounded-full", style.dotClass)}
            />
            {ev.kind}
          </span>
        </span>

        <span
          className={cn(
            "inline-flex w-fit rounded px-1.5 py-px text-[9.5px] uppercase tracking-wider",
            dir === "out"
              ? "border border-tp-amber/25 bg-tp-amber-soft text-tp-amber"
              : "border border-tp-glass-edge bg-tp-glass-inner text-tp-ink-3",
          )}
        >
          {dir === "out" ? "↑ out" : "↓ in"}
        </span>

        <span className="tabular-nums text-[10.5px] text-tp-ink-4">
          {formatBytes(bytes)}
        </span>

        <span
          className="min-w-0 truncate text-tp-ink-2"
          data-testid={`canvas-event-payload-${ev.id}`}
        >
          {preview}
        </span>

        <span className="text-right text-tp-ink-4">
          {isSelected ? (
            <ChevronDown className="inline-block h-3 w-3" aria-hidden />
          ) : (
            <ChevronUp className="inline-block h-3 w-3 rotate-180" aria-hidden />
          )}
        </span>
      </button>

      {/* Narrow-viewport inline expand. On wide screens the side drawer wins. */}
      {inlineExpand ? (
        <div className="border-t border-tp-glass-edge bg-tp-glass-inner/40 px-4 py-3 lg:hidden">
          <div className="mb-2 font-mono text-[10px] uppercase tracking-[0.1em] text-tp-ink-4">
            payload
          </div>
          <JsonView value={ev.payload} />
        </div>
      ) : null}
    </li>
  );
}

export default MessageInspector;
