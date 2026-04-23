"use client";

import * as React from "react";
import { motion, AnimatePresence, type Variants } from "framer-motion";

import { cn } from "@/lib/utils";
import { useMotion } from "@/components/ui/motion-safe";
import type { HookEvent } from "@/lib/hooks/use-mock-hook-stream";

/**
 * Colour class mapping derived from the task spec. Kept as a pure function so
 * tests can assert the binding without rendering. For approval-decided and
 * tool-called, the colour depends on payload fields, so we accept the whole
 * payload record.
 */
export function eventColor(
  kind: string,
  payload?: Record<string, unknown>,
): "accent" | "accent-2" | "accent-3" | "ok" | "warn" | "err" | "muted" {
  if (kind.startsWith("message.")) return "accent";
  if (kind === "session.patch") return "accent-2";
  if (kind.startsWith("agent.")) return "accent-3";
  if (kind === "gateway.startup") return "ok";
  if (kind === "config.changed") return "warn";
  if (kind === "approval.requested") return "warn";
  if (kind === "approval.decided") {
    const decision = payload?.decision;
    return decision === "deny" || decision === "timeout" ? "err" : "ok";
  }
  if (kind === "rate_limit.triggered") return "warn";
  if (kind === "tool.called") {
    return payload?.ok === false ? "err" : "accent-2";
  }
  return "muted";
}

const badgeClass: Record<ReturnType<typeof eventColor>, string> = {
  accent: "bg-accent/70 text-accent-foreground border-accent/40",
  "accent-2": "bg-accent-2/20 text-foreground border-accent-2/40",
  "accent-3": "bg-accent-3/20 text-foreground border-accent-3/40",
  ok: "bg-ok/20 text-foreground border-ok/40",
  warn: "bg-warn/20 text-foreground border-warn/40",
  err: "bg-err/20 text-foreground border-err/40",
  muted: "bg-muted text-muted-foreground border-border",
};

/** Pill tint for approval decision badges. */
const decisionBadgeClass: Record<string, string> = {
  allow: "bg-ok/20 text-foreground border-ok/50",
  deny: "bg-err/20 text-foreground border-err/50",
  timeout: "bg-warn/20 text-foreground border-warn/50",
};

function formatTimestamp(ts: number): string {
  const d = new Date(ts);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  const ms = String(d.getMilliseconds()).padStart(3, "0");
  return `${hh}:${mm}:${ss}.${ms}`;
}

const springPopRow: Variants = {
  hidden: { opacity: 0, scale: 0.92, y: 20 },
  visible: {
    opacity: 1,
    scale: 1,
    y: 0,
    transition: { type: "spring", stiffness: 360, damping: 26, mass: 0.7 },
  },
};
const instantRow: Variants = {
  hidden: { opacity: 0, scale: 1, y: 0 },
  visible: { opacity: 1, scale: 1, y: 0, transition: { duration: 0 } },
};

/** Exposed for tests. Aging tiers: age in seconds → visual scale + saturation. */
export function ageTier(ageSeconds: number): { scale: number; saturate: number } {
  // Shrink 2% per 10s tier, floor 0.92. Saturation drops 8% per tier, floor 0.6.
  const tier = Math.min(4, Math.floor(ageSeconds / 10));
  const scale = Math.max(0.92, 1 - tier * 0.02);
  const saturate = Math.max(0.6, 1 - tier * 0.08);
  return { scale, saturate };
}

export interface EventRowProps {
  event: HookEvent;
  /** Current wall-clock, injected so tests can freeze time. */
  now?: number;
  /** When true, approval.requested rows get an exaggerated pop + border flash. */
  alertBoost?: boolean;
}

export function EventRow({ event, now, alertBoost = false }: EventRowProps) {
  const { reduced } = useMotion();
  const [expanded, setExpanded] = React.useState(false);
  const [tick, setTick] = React.useState(0);

  // Re-render every 10s to step through aging tiers. Skipped under reduced
  // motion — those users get a static card.
  React.useEffect(() => {
    if (reduced) return;
    const id = setInterval(() => setTick((t) => t + 1), 10_000);
    return () => clearInterval(id);
  }, [reduced]);
  void tick;

  const currentNow = now ?? Date.now();
  const ageSeconds = Math.max(0, (currentNow - event.ts) / 1000);
  const { scale, saturate } = reduced
    ? { scale: 1, saturate: 1 }
    : ageTier(ageSeconds);

  const colour = eventColor(event.kind, event.payload);
  const timestamp = formatTimestamp(event.ts);
  const iso = new Date(event.ts).toISOString();

  const isApprovalRequested = event.kind === "approval.requested";
  const isApprovalDecided = event.kind === "approval.decided";
  const isRateLimit = event.kind === "rate_limit.triggered";

  // Alert-boost: motion-safe only. Gives approval-requested rows an exaggerated
  // spring + a brief border flash in --warn for ~600ms.
  const boosted = isApprovalRequested && alertBoost && !reduced;
  const [flashing, setFlashing] = React.useState(boosted);
  React.useEffect(() => {
    if (!boosted) return;
    setFlashing(true);
    const t = setTimeout(() => setFlashing(false), 600);
    return () => clearTimeout(t);
    // We intentionally re-run only when boosted flips true (once per mount).
  }, [boosted]);

  // Reduced-motion + alertBoost: swap the animated flash for a static warn border.
  const staticAlertBorder = isApprovalRequested && alertBoost && reduced;

  const boostedSpring = {
    type: "spring" as const,
    stiffness: 900, // 2.5x default
    damping: 22,
    mass: 0.7,
  };
  const normalSpring = {
    type: "spring" as const,
    stiffness: 360,
    damping: 26,
    mass: 0.7,
  };

  const liveRegion = isApprovalRequested
    ? ((boosted ? "assertive" : "polite") as "polite" | "assertive")
    : undefined;

  return (
    <motion.li
      layout={!reduced}
      variants={reduced ? instantRow : springPopRow}
      initial="hidden"
      animate={{
        opacity: 1,
        scale,
        y: 0,
        transition: reduced
          ? { duration: 0 }
          : boosted
            ? boostedSpring
            : normalSpring,
      }}
      style={{ filter: `saturate(${saturate})` }}
      className={cn(
        "rounded-md border border-border bg-card/60 px-3 py-2 text-sm shadow-1",
        "hover:bg-card/80",
        flashing && "border-warn ring-1 ring-warn/60",
        staticAlertBorder && "border-warn",
        boosted && "row-boosted",
      )}
      data-testid="event-row"
      data-kind={event.kind}
      data-boosted={boosted ? "true" : undefined}
      {...(liveRegion ? { "aria-live": liveRegion } : {})}
    >
      {isApprovalRequested ? (
        <span className="sr-only" data-testid="approval-sr">
          {`Approval request from plugin ${String(event.payload?.plugin ?? "unknown")}`}
        </span>
      ) : null}
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        aria-expanded={expanded}
        className="flex w-full items-center gap-3 text-left"
      >
        <time
          dateTime={iso}
          className="shrink-0 font-mono text-[11px] tabular-nums text-muted-foreground"
        >
          {timestamp}
        </time>
        <span
          className={cn(
            "inline-flex shrink-0 items-center rounded border px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wide",
            badgeClass[colour],
          )}
          data-testid="event-kind-badge"
        >
          {event.kind}
        </span>
        <span className="min-w-0 flex-1 truncate text-foreground">
          {event.summary}
          {isApprovalRequested && event.payload?.tool ? (
            <span className="ml-2 font-mono text-[11px] text-muted-foreground">
              {String(event.payload.plugin ?? "?")}·{String(event.payload.tool)}
            </span>
          ) : null}
          {isRateLimit && typeof event.payload?.retry_after_ms === "number" ? (
            <span
              className="ml-2 font-mono text-[11px] italic text-muted-foreground"
              data-testid="rate-limit-retry"
            >
              retry in {event.payload.retry_after_ms}ms
            </span>
          ) : null}
        </span>
        {isApprovalRequested && typeof event.payload?.id === "string" ? (
          <span
            className="shrink-0 rounded border border-warn/50 bg-warn/10 px-1.5 py-0.5 font-mono text-[10px] text-foreground"
            data-testid="approval-id"
          >
            {String(event.payload.id)}
          </span>
        ) : null}
        {isApprovalDecided && typeof event.payload?.id === "string" ? (
          <span
            className="shrink-0 rounded border bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
            data-testid="approval-id"
          >
            {String(event.payload.id)}
          </span>
        ) : null}
        {isApprovalDecided && typeof event.payload?.decision === "string" ? (
          <span
            className={cn(
              "shrink-0 rounded border px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wide",
              decisionBadgeClass[String(event.payload.decision)] ??
                decisionBadgeClass.timeout,
            )}
            data-testid="decision-badge"
          >
            {String(event.payload.decision)}
          </span>
        ) : null}
        {event.session_key ? (
          <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
            {event.session_key}
          </span>
        ) : null}
      </button>
      <AnimatePresence initial={false}>
        {expanded ? (
          <motion.pre
            key="payload"
            initial={reduced ? { opacity: 0 } : { opacity: 0, height: 0 }}
            animate={
              reduced
                ? { opacity: 1, transition: { duration: 0 } }
                : { opacity: 1, height: "auto" }
            }
            exit={
              reduced
                ? { opacity: 0, transition: { duration: 0 } }
                : { opacity: 0, height: 0 }
            }
            className="mt-2 overflow-hidden rounded bg-muted/40 p-2 font-mono text-[11px] text-muted-foreground"
            data-testid="event-payload"
          >
            {JSON.stringify(event.payload, null, 2)}
          </motion.pre>
        ) : null}
      </AnimatePresence>
    </motion.li>
  );
}

export default EventRow;
