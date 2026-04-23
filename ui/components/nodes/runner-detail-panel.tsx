"use client";

import * as React from "react";
import { AnimatePresence, motion } from "framer-motion";
import { X } from "lucide-react";

import { cn } from "@/lib/utils";
import { LiveDot } from "@/components/ui/live-dot";
import { useMotionVariants } from "@/lib/motion";
import type { Runner, RunnerHealth } from "@/lib/mocks/nodes";

const HEALTH_VARIANT: Record<RunnerHealth, "ok" | "warn" | "muted"> = {
  healthy: "ok",
  degraded: "warn",
  offline: "muted",
};

const HEALTH_LABEL: Record<RunnerHealth, string> = {
  healthy: "Healthy",
  degraded: "Degraded",
  offline: "Offline",
};

function formatDuration(sec: number): string {
  if (sec <= 0) return "—";
  if (sec < 60) return `${sec}s`;
  if (sec < 3_600) return `${Math.floor(sec / 60)}m ${sec % 60}s`;
  const h = Math.floor(sec / 3_600);
  const m = Math.floor((sec % 3_600) / 60);
  return `${h}h ${m}m`;
}

function formatLastPing(ms: number): string {
  if (ms < 1_000) return `${ms}ms`;
  return `${(ms / 1_000).toFixed(1)}s`;
}

export interface RunnerDetailPanelProps {
  runner: Runner | null;
  onClose: () => void;
  className?: string;
}

/**
 * Right-side panel showing full details for the currently-selected runner.
 * Renders nothing when `runner` is null; animation is driven by framer's
 * AnimatePresence + the motion-variants helper (reduced-motion friendly).
 */
export function RunnerDetailPanel({
  runner,
  onClose,
  className,
}: RunnerDetailPanelProps) {
  const { fadeUp } = useMotionVariants();

  return (
    <AnimatePresence>
      {runner ? (
        <motion.aside
          key={runner.id}
          initial="hidden"
          animate="visible"
          exit="hidden"
          variants={fadeUp}
          aria-label={`Runner ${runner.hostname} details`}
          className={cn(
            "flex w-full flex-col gap-4 rounded-lg border border-border bg-card/60 p-4 text-sm shadow-2",
            "lg:w-[320px]",
            className,
          )}
          data-testid="runner-detail-panel"
        >
          <header className="flex items-start justify-between gap-2">
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <LiveDot
                  variant={HEALTH_VARIANT[runner.health]}
                  pulse={runner.health === "healthy"}
                  label={HEALTH_LABEL[runner.health]}
                />
                <span className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                  {HEALTH_LABEL[runner.health]}
                </span>
              </div>
              <h2 className="mt-1 truncate text-base font-semibold">
                {runner.hostname}
              </h2>
              <div className="truncate font-mono text-[11px] text-muted-foreground">
                {runner.id}
              </div>
            </div>
            <button
              type="button"
              onClick={onClose}
              aria-label="Close runner details"
              className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              data-testid="runner-detail-close"
            >
              <X className="h-4 w-4" aria-hidden="true" />
            </button>
          </header>

          <dl className="grid grid-cols-2 gap-x-3 gap-y-2 text-xs">
            <dt className="text-muted-foreground">Ring</dt>
            <dd className="font-mono tabular-nums">
              {runner.ring === 0 ? "inner" : "outer"}
            </dd>

            <dt className="text-muted-foreground">Latency</dt>
            <dd className="font-mono tabular-nums">
              {runner.health === "offline" ? "—" : `${runner.latencyMs}ms`}
            </dd>

            <dt className="text-muted-foreground">Connected for</dt>
            <dd className="font-mono tabular-nums">
              {formatDuration(runner.connectedForSec)}
            </dd>

            <dt className="text-muted-foreground">Last ping</dt>
            <dd className="font-mono tabular-nums">
              {formatLastPing(runner.lastPingMs)} ago
            </dd>

            <dt className="text-muted-foreground">Error rate</dt>
            <dd className="font-mono tabular-nums">
              {(runner.errorRate * 100).toFixed(2)}%
            </dd>

            <dt className="text-muted-foreground">Tools advertised</dt>
            <dd className="font-mono tabular-nums">{runner.toolCount}</dd>
          </dl>

          <section aria-label="Advertised tools" className="min-w-0">
            <div className="mb-1.5 text-[10px] uppercase tracking-wider text-muted-foreground">
              Tools
            </div>
            <ul className="flex flex-wrap gap-1">
              {runner.tools.map((tool) => (
                <li
                  key={tool}
                  className="rounded-full bg-state-hover px-2 py-0.5 font-mono text-[10px] text-muted-foreground"
                >
                  {tool}
                </li>
              ))}
            </ul>
          </section>
        </motion.aside>
      ) : null}
    </AnimatePresence>
  );
}

export default RunnerDetailPanel;
