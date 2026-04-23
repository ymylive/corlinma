"use client";

import * as React from "react";
import { cn } from "@/lib/utils";

/**
 * Fixed-width bar-chart sparkline, 6 bars by default.
 *
 * Used in the System Health per-service rows — each bar is a recent
 * availability sample (%). Last bar can opt into `bad` tint via the
 * per-bar descriptor; otherwise all bars use `--tp-ok` at 45% opacity.
 *
 * Intentionally dumb — no tooltip, no interaction. Callers build their own
 * accessible data table fallback (already present via the existing
 * `<details>` sr-only pattern used in TagMemo).
 *
 * Accessibility: the whole SVG carries `aria-label` with the rounded
 * average. If a label is not passed, the viz is marked `aria-hidden`.
 */

export type SparkTone = "ok" | "warn" | "err" | "muted";

export interface SparkBar {
  /** 0–100 height (percent). */
  height: number;
  tone?: SparkTone;
}

export interface MiniSparklineProps
  extends React.HTMLAttributes<HTMLDivElement> {
  bars: SparkBar[];
  /** Total SVG height in px. Default 14. */
  height?: number;
  /** Screen-reader label. Omit to mark decorative. */
  label?: string;
}

const toneToVar: Record<SparkTone, string> = {
  ok: "var(--tp-ok)",
  warn: "var(--tp-warn)",
  err: "var(--tp-err)",
  muted: "var(--tp-ink-4)",
};

export const MiniSparkline = React.forwardRef<
  HTMLDivElement,
  MiniSparklineProps
>(function MiniSparkline(
  { bars, height = 14, label, className, ...rest },
  ref,
) {
  return (
    <div
      ref={ref}
      className={cn("flex items-end gap-[1.5px]", className)}
      style={{ height }}
      aria-hidden={label ? undefined : true}
      aria-label={label}
      role={label ? "img" : undefined}
      {...rest}
    >
      {bars.map((b, i) => (
        <span
          key={i}
          className="w-[3px] flex-1 rounded-[1px]"
          style={{
            height: `${clamp(b.height)}%`,
            background: `color-mix(in oklch, ${toneToVar[b.tone ?? "ok"]} 45%, transparent)`,
          }}
        />
      ))}
    </div>
  );
});

function clamp(n: number): number {
  if (!Number.isFinite(n)) return 0;
  if (n < 0) return 0;
  if (n > 100) return 100;
  return n;
}

export default MiniSparkline;
