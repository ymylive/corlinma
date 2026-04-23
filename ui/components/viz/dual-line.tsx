/**
 * Entropy / logic_depth dual-line chart (B5-FE1).
 *
 * Two lines drawn with `@visx/shape LinePath`: entropy (red-ish `--err`) and
 * logic_depth (green `--ok`). Both share a 0..1 y-axis. On mount each path
 * animates its `pathLength` from 0 → 1 over 1200 ms; `prefers-reduced-motion`
 * snaps to 1 instantly. Hover draws a vertical guideline and shows a
 * tooltip with both values.
 */
"use client";

import * as React from "react";
import { motion, useReducedMotion } from "framer-motion";
import { scaleLinear } from "@visx/scale";
import { LinePath } from "@visx/shape";
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

interface DualLineProps {
  chunks: TagMemoChunk[];
  className?: string;
}

interface InnerProps extends DualLineProps {
  width: number;
  height: number;
}

interface TipData {
  chunk: TagMemoChunk;
}

const MARGIN = { top: 12, right: 16, bottom: 32, left: 40 };

export function DualLine({ chunks, className }: DualLineProps) {
  return (
    <div className={cn("relative h-[320px] w-full", className)}>
      <ParentSize>
        {({ width, height }) =>
          width > 0 && height > 0 ? (
            <DualLineInner
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

function DualLineInner({ chunks, width, height }: InnerProps) {
  const reduced = useReducedMotion();
  const { hoveredId, setHoveredId } = useHoveredId();

  const innerW = Math.max(0, width - MARGIN.left - MARGIN.right);
  const innerH = Math.max(0, height - MARGIN.top - MARGIN.bottom);

  const xScale = React.useMemo(
    () =>
      scaleLinear({
        domain: [0, Math.max(1, chunks.length - 1)],
        range: [0, innerW],
      }),
    [chunks.length, innerW],
  );
  const yScale = React.useMemo(
    () => scaleLinear({ domain: [0, 1], range: [innerH, 0] }),
    [innerH],
  );

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

  const lineAnim = reduced
    ? { initial: { pathLength: 1 }, animate: { pathLength: 1 } }
    : {
        initial: { pathLength: 0 },
        animate: { pathLength: 1 },
        transition: { duration: 1.2, ease: "easeOut" as const },
      };

  const handleMouseMove = (ev: React.MouseEvent<SVGRectElement>) => {
    if (chunks.length === 0) return;
    const rect = (ev.currentTarget as SVGRectElement).getBoundingClientRect();
    const mouseX = ev.clientX - rect.left;
    const raw = xScale.invert(mouseX);
    const idx = Math.max(0, Math.min(chunks.length - 1, Math.round(raw)));
    const chunk = chunks[idx];
    if (!chunk) return;
    setHoveredId(chunk.chunk_id);
    showTooltip({
      tooltipData: { chunk },
      tooltipLeft: MARGIN.left + xScale(idx),
      tooltipTop: MARGIN.top + yScale(chunk.entropy),
    });
  };

  const handleMouseLeave = () => {
    setHoveredId(null);
    hideTooltip();
  };

  const hoveredIdx = React.useMemo(() => {
    if (hoveredId === null) return null;
    return chunks.findIndex((c) => c.chunk_id === hoveredId);
  }, [hoveredId, chunks]);

  return (
    <div ref={containerRef} className="relative h-full w-full">
      <svg
        width={width}
        height={height}
        role="img"
        aria-label="Entropy and logic depth by chunk"
      >
        <g transform={`translate(${MARGIN.left},${MARGIN.top})`}>
          <AxisBottom
            top={innerH}
            scale={xScale}
            numTicks={6}
            stroke="hsl(var(--border))"
            tickStroke="hsl(var(--border))"
            tickLabelProps={{
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
          />

          <LinePath
            data={chunks}
            x={(_, i) => xScale(i)}
            y={(d) => yScale(d.entropy)}
          >
            {({ path }) => {
              const d = path(chunks) ?? "";
              return (
                <motion.path
                  d={d}
                  fill="none"
                  stroke="hsl(var(--err))"
                  strokeWidth={1.5}
                  strokeLinecap="round"
                  {...lineAnim}
                  data-testid="line-entropy"
                />
              );
            }}
          </LinePath>
          <LinePath
            data={chunks}
            x={(_, i) => xScale(i)}
            y={(d) => yScale(d.logic_depth)}
          >
            {({ path }) => {
              const d = path(chunks) ?? "";
              return (
                <motion.path
                  d={d}
                  fill="none"
                  stroke="hsl(var(--ok))"
                  strokeWidth={1.5}
                  strokeDasharray="4 3"
                  strokeLinecap="round"
                  {...lineAnim}
                  data-testid="line-logic-depth"
                />
              );
            }}
          </LinePath>

          {hoveredIdx !== null && hoveredIdx >= 0 ? (
            <line
              x1={xScale(hoveredIdx)}
              x2={xScale(hoveredIdx)}
              y1={0}
              y2={innerH}
              stroke="hsl(var(--foreground))"
              strokeDasharray="3 3"
              strokeOpacity={0.5}
            />
          ) : null}

          {/* Transparent hit-layer for mouse tracking. */}
          <rect
            x={0}
            y={0}
            width={innerW}
            height={innerH}
            fill="transparent"
            onMouseMove={handleMouseMove}
            onMouseLeave={handleMouseLeave}
          />
        </g>

        {/* Legend — pair colour with a dash pattern so the chart is readable
            under deuteranopia (both lines are distinguishable by stroke). */}
        <g transform={`translate(${MARGIN.left + 4}, ${MARGIN.top + 4})`}>
          <line x1={0} x2={18} y1={4} y2={4} stroke="hsl(var(--err))" strokeWidth={1.5} />
          <text x={22} y={7} fontSize={10} fill="hsl(var(--muted-foreground))">
            entropy
          </text>
          <line
            x1={80}
            x2={98}
            y1={4}
            y2={4}
            stroke="hsl(var(--ok))"
            strokeWidth={1.5}
            strokeDasharray="4 3"
          />
          <text x={102} y={7} fontSize={10} fill="hsl(var(--muted-foreground))">
            logic_depth
          </text>
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
            <div className="font-semibold">
              chunk #{tooltipData.chunk.chunk_id}
            </div>
            <div>entropy: {tooltipData.chunk.entropy.toFixed(3)}</div>
            <div>
              logic_depth: {tooltipData.chunk.logic_depth.toFixed(3)}
            </div>
          </div>
        </TooltipInPortal>
      ) : null}
      {/* Screen-reader fallback. The SVG above is aria-labelled but a table
          gives AT users the raw series. */}
      <details className="sr-only">
        <summary>Entropy / logic_depth table (accessibility fallback)</summary>
        <table>
          <thead>
            <tr>
              <th>chunk_id</th>
              <th>entropy</th>
              <th>logic_depth</th>
            </tr>
          </thead>
          <tbody>
            {chunks.map((c) => (
              <tr key={c.chunk_id}>
                <td>{c.chunk_id}</td>
                <td>{c.entropy.toFixed(3)}</td>
                <td>{c.logic_depth.toFixed(3)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </div>
  );
}
