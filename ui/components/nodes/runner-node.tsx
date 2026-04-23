"use client";

import * as React from "react";
import { motion } from "framer-motion";

import { cn } from "@/lib/utils";
import type { Runner } from "@/lib/mocks/nodes";

/**
 * Visual specification for a single runner circle. Produced by
 * `<TopologyGraph>` once it has resolved ring + slot geometry.
 */
export interface RunnerNodeProps {
  runner: Runner;
  /** Pre-computed x coordinate in SVG user units. */
  cx: number;
  /** Pre-computed y coordinate in SVG user units. */
  cy: number;
  /** Circle radius (inner ring runners are slightly larger). */
  r: number;
  selected: boolean;
  /** User has enabled `prefers-reduced-motion`. */
  reduced: boolean;
  onSelect: (runner: Runner) => void;
}

const HEALTH_STROKE: Record<Runner["health"], string> = {
  healthy: "hsl(var(--ok))",
  degraded: "hsl(var(--warn))",
  offline: "hsl(var(--muted-foreground))",
};

const HEALTH_FILL: Record<Runner["health"], string> = {
  healthy: "hsl(var(--ok) / 0.12)",
  degraded: "hsl(var(--warn) / 0.12)",
  offline: "hsl(var(--muted) / 0.3)",
};

function truncateId(id: string, max = 8): string {
  if (id.length <= max) return id;
  return `${id.slice(0, max)}…`;
}

/**
 * Focusable SVG group representing one runner. Health state drives stroke
 * color and pulse; `selected` lifts the node forward via a framer `layoutId`.
 *
 * Keyboard: Tab moves focus between runners, Enter/Space toggles selection.
 */
export const RunnerNode = React.memo(function RunnerNode({
  runner,
  cx,
  cy,
  r,
  selected,
  reduced,
  onSelect,
}: RunnerNodeProps) {
  const opacity = runner.health === "offline" ? 0.3 : 1;
  const showPulse = runner.health === "healthy" && !reduced;
  const showShake = runner.health === "degraded" && !reduced;

  const onKeyDown = (e: React.KeyboardEvent<SVGGElement>) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelect(runner);
    }
  };

  const ariaLabel = [
    `Runner ${runner.hostname}`,
    `health ${runner.health}`,
    runner.health === "offline"
      ? `offline for ${Math.round(runner.lastPingMs / 1000)}s`
      : `latency ${runner.latencyMs}ms`,
    `${runner.toolCount} tools`,
  ].join(", ");

  return (
    <motion.g
      layout
      layoutId={`runner-${runner.id}`}
      transition={
        reduced
          ? { duration: 0 }
          : { type: "spring", stiffness: 320, damping: 28 }
      }
      tabIndex={0}
      role="button"
      aria-label={ariaLabel}
      aria-pressed={selected}
      data-testid={`runner-node-${runner.id}`}
      data-selected={selected ? "true" : "false"}
      data-health={runner.health}
      className={cn(
        "cursor-pointer outline-none focus-visible:[&>circle]:stroke-[3px]",
        showShake && "nodes-shake",
      )}
      style={{ opacity }}
      onClick={(e) => {
        e.stopPropagation();
        onSelect(runner);
      }}
      onKeyDown={onKeyDown}
    >
      {/* Outer soft halo — pulses on healthy nodes. Keyframes live in
          <TopologyGraph>'s scoped <style> block. */}
      {showPulse ? (
        <circle
          aria-hidden="true"
          cx={cx}
          cy={cy}
          r={r + 6}
          fill="none"
          stroke={HEALTH_STROKE[runner.health]}
          strokeOpacity={0.35}
          strokeWidth={1}
          className="nodes-halo"
        />
      ) : null}
      {/* Body. */}
      <circle
        cx={cx}
        cy={cy}
        r={r}
        fill={HEALTH_FILL[runner.health]}
        stroke={HEALTH_STROKE[runner.health]}
        strokeWidth={selected ? 3 : 2}
      />
      {/* Tool-count badge (top-right). */}
      <g aria-hidden="true">
        <circle
          cx={cx + r * 0.7}
          cy={cy - r * 0.7}
          r={8}
          fill="hsl(var(--background))"
          stroke={HEALTH_STROKE[runner.health]}
          strokeWidth={1}
        />
        <text
          x={cx + r * 0.7}
          y={cy - r * 0.7}
          fontSize="9"
          fontFamily="var(--font-geist-mono, ui-monospace)"
          textAnchor="middle"
          dominantBaseline="central"
          fill="hsl(var(--foreground))"
        >
          {runner.toolCount}
        </text>
      </g>
      {/* Id label below the node. */}
      <text
        x={cx}
        y={cy + r + 14}
        fontSize="10"
        fontFamily="var(--font-geist-mono, ui-monospace)"
        textAnchor="middle"
        fill="hsl(var(--muted-foreground))"
        aria-hidden="true"
      >
        {truncateId(runner.id.replace("rnr_", ""))}
      </text>
      {/* Color-blind safety: a shape/glyph overlay for non-healthy states.
          Degraded → "!" in warn; offline → "∅" in muted. Healthy is baseline
          (green + pulse, no glyph needed). Pair with aria-label text above. */}
      {runner.health === "degraded" ? (
        <text
          aria-hidden="true"
          x={cx - r * 0.7}
          y={cy - r * 0.55}
          fontSize={11}
          fontWeight={700}
          textAnchor="middle"
          fill="hsl(var(--warn))"
          data-testid={`runner-glyph-${runner.id}`}
        >
          !
        </text>
      ) : null}
      {runner.health === "offline" ? (
        <text
          aria-hidden="true"
          x={cx - r * 0.7}
          y={cy - r * 0.55}
          fontSize={11}
          fontWeight={700}
          textAnchor="middle"
          fill="hsl(var(--muted-foreground))"
          data-testid={`runner-glyph-${runner.id}`}
        >
          ∅
        </text>
      ) : null}
    </motion.g>
  );
});

export default RunnerNode;
