/**
 * EPA 3-axis scatter (B5-FE1).
 *
 * Renders one circle per chunk on a 2D projection of the first two EPA axes.
 * The third axis is colour-encoded (via `logic_depth`, which correlates
 * with depth-of-projection) and size is proportional to the chunk's dominant
 * energy. Hovering a circle updates the shared `hoveredId` so linked panels
 * react too.
 */
"use client";

import * as React from "react";
import { motion, useReducedMotion } from "framer-motion";
import { scaleLinear } from "@visx/scale";
import { ParentSize } from "@visx/responsive";
import { AxisBottom, AxisLeft } from "@visx/axis";
import {
  useTooltip,
  useTooltipInPortal,
  defaultStyles,
} from "@visx/tooltip";

import { cn } from "@/lib/utils";
import type { TagMemoChunk } from "@/lib/mocks/tagmemo";
import { useHoveredId } from "./use-hovered-id";

interface EpaScatterProps {
  chunks: TagMemoChunk[];
  className?: string;
}

interface InnerProps extends EpaScatterProps {
  width: number;
  height: number;
}

const MARGIN = { top: 12, right: 16, bottom: 36, left: 40 };
const MIN_R = 3;
const MAX_R = 10;

export function EpaScatter({ chunks, className }: EpaScatterProps) {
  return (
    <div className={cn("relative h-[320px] w-full", className)}>
      <ParentSize>
        {({ width, height }) =>
          width > 0 && height > 0 ? (
            <ScatterInner
              chunks={chunks}
              width={width}
              height={height}
            />
          ) : null
        }
      </ParentSize>
    </div>
  );
}

function ScatterInner({ chunks, width, height }: InnerProps) {
  const { hoveredId, setHoveredId } = useHoveredId();
  const reduced = useReducedMotion();

  const xs = chunks.map((c) => c.projections[0] ?? 0);
  const ys = chunks.map((c) => c.projections[1] ?? 0);
  const ds = chunks.map((c) => c.logic_depth);

  const xDomain = domainOf(xs);
  const yDomain = domainOf(ys);
  const dDomain = domainOf(ds, 0, 1);

  const innerW = Math.max(0, width - MARGIN.left - MARGIN.right);
  const innerH = Math.max(0, height - MARGIN.top - MARGIN.bottom);

  const xScale = React.useMemo(
    () => scaleLinear({ domain: xDomain, range: [0, innerW], nice: true }),
    [xDomain, innerW],
  );
  const yScale = React.useMemo(
    () => scaleLinear({ domain: yDomain, range: [innerH, 0], nice: true }),
    [yDomain, innerH],
  );
  const colorScale = React.useMemo(
    () => scaleLinear({ domain: dDomain, range: [0, 1] }),
    [dDomain],
  );

  const {
    tooltipOpen,
    tooltipData,
    tooltipLeft,
    tooltipTop,
    showTooltip,
    hideTooltip,
  } = useTooltip<TagMemoChunk>();
  const { containerRef, TooltipInPortal } = useTooltipInPortal({
    detectBounds: true,
    scroll: true,
  });

  const axisLabelX =
    chunks[0]?.dominant_axes[0]?.label ?? "axis_0";
  const axisLabelY =
    chunks[0]?.dominant_axes[1]?.label ?? "axis_1";
  const axisLabelD =
    chunks[0]?.dominant_axes[2]?.label ?? "axis_2";

  return (
    <div ref={containerRef} className="relative h-full w-full">
      <svg
        width={width}
        height={height}
        role="img"
        aria-label="EPA 3-axis scatter"
      >
        <g transform={`translate(${MARGIN.left},${MARGIN.top})`}>
          <AxisBottom
            top={innerH}
            scale={xScale}
            numTicks={5}
            stroke="hsl(var(--border))"
            tickStroke="hsl(var(--border))"
            tickLabelProps={{
              fill: "hsl(var(--muted-foreground))",
              fontSize: 10,
              textAnchor: "middle",
            }}
            label={axisLabelX}
            labelProps={{
              fill: "hsl(var(--muted-foreground))",
              fontSize: 10,
              textAnchor: "middle",
            }}
          />
          <AxisLeft
            scale={yScale}
            numTicks={5}
            stroke="hsl(var(--border))"
            tickStroke="hsl(var(--border))"
            tickLabelProps={{
              fill: "hsl(var(--muted-foreground))",
              fontSize: 10,
              textAnchor: "end",
              dx: -4,
              dy: 3,
            }}
            label={axisLabelY}
            labelProps={{
              fill: "hsl(var(--muted-foreground))",
              fontSize: 10,
              textAnchor: "middle",
            }}
          />

          {chunks.map((c) => {
            const cx = xScale(c.projections[0] ?? 0);
            const cy = yScale(c.projections[1] ?? 0);
            const energy = c.dominant_axes[0]?.energy ?? 0.25;
            const baseR = MIN_R + energy * (MAX_R - MIN_R);
            const highlighted = hoveredId === c.chunk_id;
            const dim =
              hoveredId !== null && hoveredId !== c.chunk_id
                ? 0.25
                : 1;
            const t = colorScale(c.logic_depth) ?? 0;
            const fill = colourForDepth(t);
            return (
              <motion.circle
                key={c.chunk_id}
                layout={!reduced}
                cx={cx}
                cy={cy}
                r={highlighted ? baseR * 1.6 : baseR}
                fill={fill}
                stroke={highlighted ? "hsl(var(--foreground))" : "none"}
                strokeWidth={highlighted ? 1.2 : 0}
                opacity={dim}
                data-testid={`scatter-dot-${c.chunk_id}`}
                onMouseOver={(ev) => {
                  setHoveredId(c.chunk_id);
                  showTooltip({
                    tooltipData: c,
                    tooltipLeft:
                      (ev.nativeEvent as MouseEvent).offsetX ?? cx,
                    tooltipTop:
                      (ev.nativeEvent as MouseEvent).offsetY ?? cy,
                  });
                }}
                onMouseOut={() => {
                  setHoveredId(null);
                  hideTooltip();
                }}
                onFocus={() => setHoveredId(c.chunk_id)}
                onBlur={() => setHoveredId(null)}
                style={{ cursor: "pointer" }}
              />
            );
          })}
        </g>
        {/* Legend — colour key for the third axis. */}
        <g transform={`translate(${width - 140},12)`}>
          <text fontSize={10} fill="hsl(var(--muted-foreground))">
            {axisLabelD} (depth)
          </text>
          {[0, 0.25, 0.5, 0.75, 1].map((t, i) => (
            <rect
              key={t}
              x={i * 20}
              y={6}
              width={20}
              height={6}
              fill={colourForDepth(t)}
            />
          ))}
        </g>
      </svg>
      {tooltipOpen && tooltipData ? (
        <TooltipInPortal
          top={tooltipTop}
          left={tooltipLeft}
          style={{
            ...defaultStyles,
            background: "hsl(var(--popover))",
            color: "hsl(var(--popover-foreground))",
            border: "1px solid hsl(var(--border))",
            fontSize: 11,
            padding: "6px 8px",
          }}
        >
          <div className="font-mono text-[11px] leading-4">
            <div className="font-semibold">chunk #{tooltipData.chunk_id}</div>
            <div>
              x: {tooltipData.projections[0]?.toFixed(2)} · y:{" "}
              {tooltipData.projections[1]?.toFixed(2)}
            </div>
            <div>entropy: {tooltipData.entropy.toFixed(3)}</div>
            <div>logic_depth: {tooltipData.logic_depth.toFixed(3)}</div>
          </div>
        </TooltipInPortal>
      ) : null}
    </div>
  );
}

// ------------- helpers -------------

function domainOf(
  vs: number[],
  fallbackLo = 0,
  fallbackHi = 1,
): [number, number] {
  if (vs.length === 0) return [fallbackLo, fallbackHi];
  let lo = Infinity;
  let hi = -Infinity;
  for (const v of vs) {
    if (v < lo) lo = v;
    if (v > hi) hi = v;
  }
  if (!Number.isFinite(lo) || !Number.isFinite(hi)) {
    return [fallbackLo, fallbackHi];
  }
  if (lo === hi) return [lo - 1, hi + 1];
  return [lo, hi];
}

/**
 * Two-stop gradient from muted → primary accent. Indices 0..1 expected.
 * Colour-blind safety: we also vary size by energy, so depth information
 * isn't only encoded in hue.
 */
function colourForDepth(t: number): string {
  const clamped = Math.max(0, Math.min(1, t));
  // HSL ramp from accent-2 (teal) to primary (indigo) — distinguishable in
  // greyscale too because the lightness differs.
  const hue = 174 + (244 - 174) * clamped;
  const sat = 60;
  const light = 70 - clamped * 25;
  return `hsl(${hue.toFixed(1)}, ${sat}%, ${light.toFixed(1)}%)`;
}
