"use client";

import * as React from "react";
import { AnimatePresence, motion } from "framer-motion";
import { ChevronDown, ChevronUp } from "lucide-react";

import { cn } from "@/lib/utils";
import { useMotion } from "@/components/ui/motion-safe";
import type { CanvasEvent, CanvasFrameKind } from "@/lib/mocks/canvas";

/**
 * Color coding per the B5 spec:
 *   present | navigate          → --accent      (indigo)
 *   a2ui_push | a2ui_reset      → --accent-2    (teal)
 *   hide                        → --muted       (neutral grey)
 *   eval | snapshot             → --accent-3    (amber)
 *
 * `dotClass` is applied to a bare span — `<LiveDot>` only exposes ok/warn/err
 * variants, so we render the kind-colored dot inline with the same pulse
 * primitive (animate-ping) it uses.
 */
function kindStyle(kind: CanvasFrameKind): {
  dotClass: string;
  ringClass: string;
  labelClass: string;
} {
  switch (kind) {
    case "present":
    case "navigate":
      return {
        dotClass: "bg-[hsl(var(--accent-foreground))]",
        ringClass: "bg-[hsl(var(--accent-foreground))]/40",
        labelClass: "text-[hsl(var(--accent-foreground))]",
      };
    case "a2ui_push":
    case "a2ui_reset":
      return {
        dotClass: "bg-accent-2",
        ringClass: "bg-accent-2/40",
        labelClass: "text-accent-2",
      };
    case "eval":
    case "snapshot":
      return {
        dotClass: "bg-accent-3",
        ringClass: "bg-accent-3/40",
        labelClass: "text-accent-3",
      };
    case "hide":
    default:
      return {
        dotClass: "bg-muted-foreground/60",
        ringClass: "bg-muted-foreground/30",
        labelClass: "text-muted-foreground",
      };
  }
}

export interface ProtocolInspectorProps {
  events: CanvasEvent[];
  /** Most-recent event id; receives a 500ms pulse halo. */
  newestId?: string;
  /** Optional session-ended chip at the top of the panel. */
  ended?: boolean;
  endedLabel?: string;
  /** Controlled expand state. Uncontrolled when omitted. */
  expanded?: boolean;
  onExpandedChange?: (expanded: boolean) => void;
  /** Labels (localised by the parent). */
  labels: {
    title: string;
    expand: string;
    collapse: string;
    empty: string;
    sessionEnded: string;
  };
}

const EXPANDED_MAX_VH = "50vh";

/**
 * Bottom-anchored protocol-message inspector.
 *
 * - Collapsed: 40px tall rail with a chevron toggle.
 * - Expanded: up to 50vh, scrollable event log (role="log", aria-live=polite).
 * - Under reduced-motion we skip the height spring and the pulse halo; the
 *   panel snaps open/closed.
 */
export function ProtocolInspector({
  events,
  newestId,
  ended,
  endedLabel,
  expanded: controlledExpanded,
  onExpandedChange,
  labels,
}: ProtocolInspectorProps) {
  const { reduced } = useMotion();
  const [internalExpanded, setInternalExpanded] = React.useState(false);
  const expanded = controlledExpanded ?? internalExpanded;
  const setExpanded = (next: boolean) => {
    if (onExpandedChange) onExpandedChange(next);
    else setInternalExpanded(next);
  };

  const [expandedIds, setExpandedIds] = React.useState<Set<string>>(
    () => new Set(),
  );
  const toggleRow = (id: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  // Announce the collapsed/expanded state changes to screen readers.
  const [announce, setAnnounce] = React.useState("");
  React.useEffect(() => {
    setAnnounce(expanded ? labels.expand : labels.collapse);
  }, [expanded, labels.expand, labels.collapse]);

  return (
    <div
      className={cn(
        "border-t border-border bg-panel",
        "flex flex-col overflow-hidden",
      )}
      data-testid="canvas-inspector"
    >
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        aria-expanded={expanded}
        aria-controls="canvas-inspector-log"
        className="flex h-10 w-full shrink-0 items-center justify-between gap-2 px-4 text-xs text-muted-foreground transition-colors hover:bg-accent/40 hover:text-foreground"
        data-testid="canvas-inspector-toggle"
      >
        <span className="inline-flex items-center gap-2 font-medium">
          {expanded ? (
            <ChevronDown className="h-3.5 w-3.5" aria-hidden />
          ) : (
            <ChevronUp className="h-3.5 w-3.5" aria-hidden />
          )}
          {labels.title}
          <span className="ml-2 tabular-nums text-muted-foreground/70">
            {events.length}
          </span>
        </span>
        {ended ? (
          <span
            className="inline-flex items-center gap-1 rounded-full border border-muted-foreground/30 bg-muted-foreground/5 px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider text-muted-foreground"
            data-testid="canvas-session-ended"
          >
            {endedLabel ?? labels.sessionEnded}
          </span>
        ) : null}
      </button>

      <span className="sr-only" role="status" aria-live="polite">
        {announce}
      </span>

      <AnimatePresence initial={false}>
        {expanded ? (
          <motion.div
            key="log"
            id="canvas-inspector-log"
            role="log"
            aria-live="polite"
            aria-label={labels.title}
            initial={reduced ? false : { height: 0, opacity: 0 }}
            animate={
              reduced
                ? { height: "auto", opacity: 1 }
                : { height: "auto", opacity: 1 }
            }
            exit={reduced ? { height: 0, opacity: 0 } : { height: 0, opacity: 0 }}
            transition={
              reduced
                ? { duration: 0 }
                : { type: "spring", stiffness: 240, damping: 28, mass: 0.9 }
            }
            className="overflow-hidden"
            style={{ maxHeight: EXPANDED_MAX_VH }}
          >
            <ul
              className="flex flex-col divide-y divide-border/60 overflow-y-auto"
              style={{ maxHeight: EXPANDED_MAX_VH }}
            >
              {events.length === 0 ? (
                <li className="px-4 py-6 text-center text-xs text-muted-foreground">
                  {labels.empty}
                </li>
              ) : (
                events.map((ev) => (
                  <EventRow
                    key={ev.id}
                    ev={ev}
                    isNewest={ev.id === newestId}
                    isExpanded={expandedIds.has(ev.id)}
                    onToggle={() => toggleRow(ev.id)}
                    reduced={reduced}
                  />
                ))
              )}
            </ul>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}

interface EventRowProps {
  ev: CanvasEvent;
  isNewest: boolean;
  isExpanded: boolean;
  onToggle: () => void;
  reduced: boolean;
}

const TRUNCATE_LEN = 120;

function EventRow({
  ev,
  isNewest,
  isExpanded,
  onToggle,
  reduced,
}: EventRowProps) {
  const style = kindStyle(ev.kind);
  const payloadStr = React.useMemo(
    () => JSON.stringify(ev.payload),
    [ev.payload],
  );
  const truncated = payloadStr.length > TRUNCATE_LEN;
  const showPulse = isNewest && !reduced;

  // Pulse only persists for the first 500ms after the event lands — the
  // parent rotates `newestId` forward quickly enough that this resolves
  // naturally, but we also time-bound it here so late renders don't keep
  // pulsing forever.
  const [pulseGate, setPulseGate] = React.useState(showPulse);
  React.useEffect(() => {
    if (!showPulse) {
      setPulseGate(false);
      return;
    }
    setPulseGate(true);
    const t = setTimeout(() => setPulseGate(false), 500);
    return () => clearTimeout(t);
  }, [showPulse, ev.id]);

  return (
    <li
      data-testid={`canvas-event-${ev.id}`}
      data-kind={ev.kind}
      className="flex items-start gap-3 px-4 py-2 font-mono text-xs leading-relaxed"
    >
      <span className="relative mt-1.5 inline-flex h-2 w-2 shrink-0">
        {pulseGate ? (
          <span
            aria-hidden
            className={cn(
              "absolute inset-0 animate-ping rounded-full",
              style.ringClass,
            )}
          />
        ) : null}
        <span
          aria-hidden
          className={cn("relative inline-flex h-2 w-2 rounded-full", style.dotClass)}
        />
      </span>
      <span className="shrink-0 tabular-nums text-muted-foreground">
        {ev.timestamp}
      </span>
      <span
        className={cn("shrink-0 font-semibold", style.labelClass)}
        data-testid={`canvas-event-kind-${ev.id}`}
      >
        {ev.kind}
      </span>
      <button
        type="button"
        onClick={onToggle}
        className={cn(
          "min-w-0 flex-1 truncate text-left text-muted-foreground hover:text-foreground",
          isExpanded && "whitespace-pre-wrap break-all",
        )}
        aria-expanded={isExpanded}
        aria-label={`toggle payload for ${ev.kind}`}
        data-testid={`canvas-event-payload-${ev.id}`}
      >
        {isExpanded || !truncated
          ? payloadStr
          : `${payloadStr.slice(0, TRUNCATE_LEN)}…`}
      </button>
    </li>
  );
}

export default ProtocolInspector;
