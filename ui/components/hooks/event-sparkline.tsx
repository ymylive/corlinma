"use client";

import * as React from "react";
import { cn } from "@/lib/utils";

export interface EventSparklineProps
  extends React.SVGAttributes<SVGSVGElement> {
  /** Oldest → newest numeric samples. */
  samples: number[];
  width?: number;
  height?: number;
  /** Accessible label for screen readers. */
  label?: string;
}

/**
 * Tiny inline SVG sparkline. No deps. Renders a filled path plus a stroked
 * top line over the supplied numeric samples. Pure data visualisation — no
 * animation, so reduced-motion users get the same output as everyone else.
 *
 * Batch 5 will swap this for a real charting primitive.
 */
export function EventSparkline({
  samples,
  width = 180,
  height = 40,
  label,
  className,
  ...rest
}: EventSparklineProps) {
  const { areaPath, linePath, max } = React.useMemo(() => {
    if (samples.length === 0) {
      return { areaPath: "", linePath: "", max: 0 };
    }
    const maxVal = Math.max(1, ...samples);
    const stepX = samples.length > 1 ? width / (samples.length - 1) : 0;
    // Leave a 1px margin so the stroke doesn't clip on the top/bottom edges.
    const topMargin = 1;
    const bottomMargin = 1;
    const plotHeight = Math.max(1, height - topMargin - bottomMargin);

    const points = samples.map((value, idx) => {
      const x = idx * stepX;
      const ratio = value / maxVal;
      const y = topMargin + (1 - ratio) * plotHeight;
      return [x, y] as const;
    });

    const line = points
      .map(([x, y], idx) => `${idx === 0 ? "M" : "L"}${x.toFixed(2)},${y.toFixed(2)}`)
      .join(" ");

    const area =
      points.length > 0
        ? `${line} L${(points[points.length - 1]![0]).toFixed(2)},${height} L0,${height} Z`
        : "";

    return { areaPath: area, linePath: line, max: maxVal };
  }, [samples, width, height]);

  const current = samples.length > 0 ? samples[samples.length - 1] ?? 0 : 0;

  return (
    <>
      <svg
        viewBox={`0 0 ${width} ${height}`}
        width={width}
        height={height}
        role="img"
        aria-label={label ?? `Sparkline, peak ${max}`}
        className={cn("overflow-visible", className)}
        {...rest}
      >
        {areaPath ? (
          <path d={areaPath} className="fill-primary/15" />
        ) : null}
        {linePath ? (
          <path
            d={linePath}
            className="fill-none stroke-primary"
            strokeWidth={1.5}
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        ) : null}
      </svg>
      {/* Screen-reader text summary — a one-line equivalent of the sparkline
          for AT users, since SVG path geometry isn't enumerable. */}
      <span className="sr-only" data-testid="sparkline-sr">
        events/s over last {samples.length} samples: current {current}, peak{" "}
        {max}
      </span>
    </>
  );
}

export default EventSparkline;
