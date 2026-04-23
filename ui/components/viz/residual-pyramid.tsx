/**
 * Residual pyramid — stacked horizontal bars, one row per chunk (B5-FE1).
 *
 * Each row has `pyramid_levels.length` segments; widths are proportional to
 * `explained_energy`, colours are keyed to the axis label. Rows reveal
 * top-down in an 80ms stagger — disabled under `prefers-reduced-motion`
 * because stagger of 500 rows is a nontrivial amount of animation.
 */
"use client";

import * as React from "react";
import { motion, useReducedMotion } from "framer-motion";
import {
  useTooltip,
  useTooltipInPortal,
  defaultStyles,
} from "@visx/tooltip";

import { cn } from "@/lib/utils";
import type {
  PyramidLevel,
  TagMemoChunk,
} from "@/lib/mocks/tagmemo";
import { useHoveredId } from "./use-hovered-id";

interface ResidualPyramidProps {
  chunks: TagMemoChunk[];
  /** Parent width — we cap at 900 regardless. */
  parentWidth: number;
  className?: string;
}

const ROW_HEIGHT = 12;
const ROW_GAP = 1;
const LABEL_COL = 52; // space for the chunk id on the left
const MAX_WIDTH = 900;

interface TipData {
  chunk: TagMemoChunk;
}

export function ResidualPyramid({
  chunks,
  parentWidth,
  className,
}: ResidualPyramidProps) {
  const reduced = useReducedMotion();
  const { hoveredId, setHoveredId } = useHoveredId();

  const width = Math.min(parentWidth, MAX_WIDTH);
  const rowWidth = Math.max(200, width - LABEL_COL - 12);
  const height = chunks.length * (ROW_HEIGHT + ROW_GAP);

  const {
    tooltipOpen,
    tooltipData,
    tooltipLeft,
    tooltipTop,
    showTooltip,
    hideTooltip,
  } = useTooltip<TipData>();
  const { containerRef, TooltipInPortal } = useTooltipInPortal({
    detectBounds: true,
    scroll: true,
  });

  // Stagger caps out — animating 500 rows with layout delay is fine here
  // because we're only animating opacity/x, not layout. Under reduced motion
  // we just render them immediately.
  const staggerMs = reduced ? 0 : 80;

  return (
    <div
      ref={containerRef}
      className={cn(
        "relative max-h-[420px] w-full overflow-auto rounded-md border border-border bg-panel",
        className,
      )}
      data-testid="residual-pyramid-root"
    >
      <svg
        width={width}
        height={height}
        role="img"
        aria-label="Residual pyramid — per-chunk axis decomposition"
      >
        {chunks.map((c, rowIdx) => {
          const y = rowIdx * (ROW_HEIGHT + ROW_GAP);
          const highlighted = hoveredId === c.chunk_id;
          const dim =
            hoveredId !== null && hoveredId !== c.chunk_id ? 0.3 : 1;

          // Compute segment widths from explained_energy.
          const total = c.pyramid_levels.reduce(
            (acc, l) => acc + l.explained_energy,
            0,
          );
          let xCursor = 0;

          const row = (
            <g
              key={c.chunk_id}
              transform={`translate(0, ${y})`}
              opacity={dim}
              onMouseOver={(ev) => {
                setHoveredId(c.chunk_id);
                const rect = (
                  ev.currentTarget.ownerSVGElement as SVGSVGElement
                ).getBoundingClientRect();
                showTooltip({
                  tooltipData: { chunk: c },
                  tooltipLeft: ev.clientX - rect.left,
                  tooltipTop: ev.clientY - rect.top,
                });
              }}
              onMouseOut={() => {
                setHoveredId(null);
                hideTooltip();
              }}
              data-testid={`pyramid-row-${c.chunk_id}`}
              style={{ cursor: "pointer" }}
            >
              <text
                x={LABEL_COL - 6}
                y={ROW_HEIGHT - 2}
                textAnchor="end"
                fontSize={9}
                fill="hsl(var(--muted-foreground))"
                className="font-mono"
              >
                #{c.chunk_id}
              </text>
              {highlighted ? (
                <rect
                  x={LABEL_COL - 2}
                  y={-1}
                  width={rowWidth + 4}
                  height={ROW_HEIGHT + 2}
                  fill="none"
                  stroke="hsl(var(--foreground))"
                  strokeWidth={0.8}
                />
              ) : null}
              {c.pyramid_levels.map((lvl, segIdx) => {
                const frac =
                  total > 0 ? lvl.explained_energy / total : 0;
                const w = rowWidth * frac;
                const x = LABEL_COL + xCursor;
                xCursor += w;
                const fill = colourForAxis(lvl.axis_label);
                const showLabel = w > 48;
                return (
                  <g key={segIdx}>
                    <rect
                      x={x}
                      y={0}
                      width={Math.max(0, w - 0.5)}
                      height={ROW_HEIGHT}
                      fill={fill}
                      rx={1}
                    />
                    {showLabel ? (
                      <text
                        x={x + 4}
                        y={ROW_HEIGHT - 3}
                        fontSize={8}
                        fill="hsl(var(--background))"
                        className="font-mono"
                      >
                        {lvl.axis_label}
                      </text>
                    ) : null}
                  </g>
                );
              })}
            </g>
          );

          if (reduced) return row;
          return (
            <motion.g
              key={c.chunk_id}
              initial={{ opacity: 0, x: -8 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{
                duration: 0.2,
                delay: (rowIdx * staggerMs) / 1000,
                ease: "easeOut",
              }}
            >
              {row}
            </motion.g>
          );
        })}
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
            <div className="font-semibold">
              chunk #{tooltipData.chunk.chunk_id}
            </div>
            <ul className="mt-1 space-y-0.5">
              {tooltipData.chunk.pyramid_levels.map((l, i) => (
                <li key={i} className="flex items-center gap-1">
                  <span
                    className="inline-block h-2 w-2 rounded-sm"
                    style={{
                      backgroundColor: colourForAxis(l.axis_label),
                    }}
                  />
                  <span>{l.axis_label}</span>
                  <span className="ml-auto">
                    {(l.explained_energy * 100).toFixed(1)}%
                  </span>
                </li>
              ))}
            </ul>
          </div>
        </TooltipInPortal>
      ) : null}
    </div>
  );
}

// -------- helpers --------

// 8 distinct hues, one per axis label. Picked for distinguishability in
// light + dark modes; the row outline on hover is what carries the
// highlight signal, so colour alone isn't load-bearing for a11y.
const AXIS_COLOURS: Record<string, string> = {
  identity: "hsl(244 75% 60%)",
  intent: "hsl(174 60% 45%)",
  emotion: "hsl(0 72% 55%)",
  tempo: "hsl(38 92% 55%)",
  certainty: "hsl(142 71% 45%)",
  locality: "hsl(210 80% 55%)",
  abstraction: "hsl(288 60% 60%)",
  tense: "hsl(20 80% 55%)",
};

function colourForAxis(label: string): string {
  return AXIS_COLOURS[label] ?? "hsl(240 5% 60%)";
}

/**
 * Helper exported for tests / rows consumed outside SVG.
 */
export function pyramidRowSummary(levels: PyramidLevel[]): string {
  return levels
    .map((l) => `${l.axis_label}:${(l.explained_energy * 100).toFixed(0)}%`)
    .join(" · ");
}
