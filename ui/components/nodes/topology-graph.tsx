"use client";

import * as React from "react";
import { useMotion } from "@/components/ui/motion-safe";
import { cn } from "@/lib/utils";
import type { Runner } from "@/lib/mocks/nodes";
import { RunnerNode } from "./runner-node";

/**
 * Radial topology SVG: a central "Gateway" rounded rect with runners orbiting
 * on two concentric ellipses. Geometry is computed once per render from the
 * runner list; visuals (health glow, dataflow dash) live in a scoped <style>
 * block so we don't have to touch globals.css or tailwind.config.ts.
 *
 *   Ring 0 (inner)  — 6 slots  · rx/ry = 180/150
 *   Ring 1 (outer)  — 12 slots · rx/ry = 320/280
 *
 *   Slot k on a ring with N slots lands at:
 *     angle = -π/2 + 2π · k / N
 *     x     = cx + rx · cos(angle)
 *     y     = cy + ry · sin(angle)
 *   (-π/2 starts the first slot at the top of the orbit.)
 */

const VIEWBOX = 800;
const CENTER = VIEWBOX / 2;
const GATEWAY_HALF_W = 72;
const GATEWAY_HALF_H = 40;

interface RingSpec {
  slots: number;
  rx: number;
  ry: number;
  nodeRadius: number;
}

const RINGS: Record<0 | 1, RingSpec> = {
  0: { slots: 6, rx: 180, ry: 150, nodeRadius: 24 },
  1: { slots: 12, rx: 320, ry: 280, nodeRadius: 18 },
};

interface PositionedRunner {
  runner: Runner;
  cx: number;
  cy: number;
  r: number;
}

function position(runner: Runner): PositionedRunner {
  const ring = RINGS[runner.ring];
  const angle = -Math.PI / 2 + (2 * Math.PI * runner.slot) / ring.slots;
  const cx = CENTER + ring.rx * Math.cos(angle);
  const cy = CENTER + ring.ry * Math.sin(angle);
  return { runner, cx, cy, r: ring.nodeRadius };
}

function strokeForHealth(health: Runner["health"]): string {
  if (health === "healthy") return "hsl(var(--ok))";
  if (health === "degraded") return "hsl(var(--warn))";
  return "hsl(var(--muted-foreground))";
}

export interface TopologyGraphProps {
  runners: Runner[];
  selectedId: string | null;
  onSelect: (runner: Runner | null) => void;
  className?: string;
}

export function TopologyGraph({
  runners,
  selectedId,
  onSelect,
  className,
}: TopologyGraphProps) {
  const { reduced } = useMotion();

  const positioned = React.useMemo(
    () => runners.map(position),
    [runners],
  );

  // Dataflow keyframes + pulse + shake. Scoped to this component so we don't
  // pollute the global stylesheet. Reduced-motion disables every animation.
  const styleBlock = React.useMemo(() => {
    if (reduced) {
      return `
        .nodes-halo { animation: none; }
        .nodes-shake { animation: none; }
        .nodes-dash { animation: none; }
      `;
    }
    return `
      @keyframes nodes-dash-kf {
        from { stroke-dashoffset: 24; }
        to   { stroke-dashoffset: 0; }
      }
      @keyframes nodes-halo-kf {
        0%, 100% { opacity: 0.2; transform-origin: center; }
        50%      { opacity: 0.6; }
      }
      @keyframes nodes-shake-kf {
        0%, 88%, 100% { transform: translateX(0); }
        90%           { transform: translateX(-2px); }
        92%           { transform: translateX(2px); }
        94%           { transform: translateX(-2px); }
        96%           { transform: translateX(2px); }
      }
      .nodes-dash { animation: nodes-dash-kf 1.2s linear infinite; }
      .nodes-halo { animation: nodes-halo-kf 2s ease-in-out infinite; }
      .nodes-shake { animation: nodes-shake-kf 6s ease-in-out infinite; }
    `;
  }, [reduced]);

  return (
    <div
      className={cn(
        "relative w-full overflow-hidden rounded-lg border border-border bg-card/40",
        className,
      )}
      data-testid="topology-graph"
    >
      <style>{styleBlock}</style>
      <svg
        viewBox={`0 0 ${VIEWBOX} ${VIEWBOX}`}
        role="img"
        aria-label="Runner topology"
        className="block h-auto w-full"
        onClick={() => onSelect(null)}
      >
        {/* Orbital guides — decorative, muted. */}
        <ellipse
          cx={CENTER}
          cy={CENTER}
          rx={RINGS[0].rx}
          ry={RINGS[0].ry}
          fill="none"
          stroke="hsl(var(--border))"
          strokeDasharray="2 6"
          strokeWidth={1}
          aria-hidden="true"
        />
        <ellipse
          cx={CENTER}
          cy={CENTER}
          rx={RINGS[1].rx}
          ry={RINGS[1].ry}
          fill="none"
          stroke="hsl(var(--border))"
          strokeDasharray="2 6"
          strokeWidth={1}
          aria-hidden="true"
        />

        {/* Connection lines — drawn first so runners render on top. */}
        {positioned.map(({ runner, cx, cy }) => {
          const stroke = strokeForHealth(runner.health);
          const opacity = runner.health === "offline" ? 0.25 : 0.55;
          return (
            <path
              key={`link-${runner.id}`}
              d={`M ${CENTER} ${CENTER} L ${cx} ${cy}`}
              stroke={stroke}
              strokeOpacity={opacity}
              strokeWidth={1.5}
              strokeDasharray="6 6"
              fill="none"
              className={runner.health === "offline" ? undefined : "nodes-dash"}
              aria-hidden="true"
              data-testid={`link-${runner.id}`}
            />
          );
        })}

        {/* Gateway center block. */}
        <g aria-label="Gateway">
          <rect
            x={CENTER - GATEWAY_HALF_W}
            y={CENTER - GATEWAY_HALF_H}
            width={GATEWAY_HALF_W * 2}
            height={GATEWAY_HALF_H * 2}
            rx={12}
            fill="hsl(var(--accent))"
            stroke="hsl(var(--primary))"
            strokeWidth={2}
          />
          {/* Server-stack glyph (pure SVG copy of Lucide `Server`). */}
          <g
            aria-hidden="true"
            transform={`translate(${CENTER - 10} ${CENTER - 22})`}
            stroke="hsl(var(--primary))"
            strokeWidth={1.6}
            strokeLinecap="round"
            strokeLinejoin="round"
            fill="none"
          >
            <rect x={0} y={0} width={20} height={8} rx={2} />
            <rect x={0} y={12} width={20} height={8} rx={2} />
            <line x1={6} y1={4} x2={6.01} y2={4} />
            <line x1={6} y1={16} x2={6.01} y2={16} />
          </g>
          <text
            x={CENTER}
            y={CENTER + 20}
            fontSize="12"
            fontWeight={600}
            textAnchor="middle"
            fill="hsl(var(--foreground))"
          >
            Gateway
          </text>
        </g>

        {/* Runners — drawn last so they z-stack above the lines. */}
        {positioned.map(({ runner, cx, cy, r }) => (
          <RunnerNode
            key={runner.id}
            runner={runner}
            cx={cx}
            cy={cy}
            r={r}
            selected={selectedId === runner.id}
            reduced={reduced}
            onSelect={(next) => onSelect(next)}
          />
        ))}
      </svg>
    </div>
  );
}

export default TopologyGraph;
