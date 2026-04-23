"use client";

import * as React from "react";
import { motion } from "framer-motion";

import { useMotion } from "@/components/ui/motion-safe";
import { cn } from "@/lib/utils";

export interface SplitPaneProps {
  left: React.ReactNode;
  right: React.ReactNode;
  /** Initial split position as percent 0-100. Defaults to 50. */
  defaultSplit?: number;
  /** Minimum % each pane may shrink to. Defaults to 20. */
  minPercent?: number;
  /** Accessible label for the divider. */
  ariaLabel?: string;
  className?: string;
}

const SNAP_CENTRE = 50;
const SNAP_WINDOW = 5; // snap if within ±5%

function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, v));
}

/**
 * Horizontal split pane with a draggable divider.
 *
 * - Click-drag updates the split percent via pointer events.
 * - Release within ±5% of centre snaps back to 50% (spring under motion,
 *   instant under reduced-motion).
 * - Keyboard: ← / → nudge ±5%; Home / End go to `minPercent` / `100 - min`.
 * - ARIA: `role="separator"`, `aria-orientation="vertical"`,
 *   `aria-valuenow/min/max`, `aria-label`.
 */
export function SplitPane({
  left,
  right,
  defaultSplit = 50,
  minPercent = 20,
  ariaLabel = "Resize panes",
  className,
}: SplitPaneProps) {
  const containerRef = React.useRef<HTMLDivElement>(null);
  const [split, setSplit] = React.useState(() =>
    clamp(defaultSplit, minPercent, 100 - minPercent),
  );
  const [dragging, setDragging] = React.useState(false);
  const { reduced } = useMotion();

  const minAllowed = minPercent;
  const maxAllowed = 100 - minPercent;

  const updateFromClientX = React.useCallback(
    (clientX: number) => {
      const el = containerRef.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      const pct = ((clientX - rect.left) / rect.width) * 100;
      setSplit(clamp(pct, minAllowed, maxAllowed));
    },
    [minAllowed, maxAllowed],
  );

  const onPointerDown = React.useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      e.preventDefault();
      (e.target as HTMLDivElement).setPointerCapture?.(e.pointerId);
      setDragging(true);
    },
    [],
  );

  const onPointerMove = React.useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (!dragging) return;
      updateFromClientX(e.clientX);
    },
    [dragging, updateFromClientX],
  );

  const onPointerUp = React.useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (!dragging) return;
      (e.target as HTMLDivElement).releasePointerCapture?.(e.pointerId);
      setDragging(false);
      setSplit((curr) => {
        if (Math.abs(curr - SNAP_CENTRE) <= SNAP_WINDOW) return SNAP_CENTRE;
        return curr;
      });
    },
    [dragging],
  );

  const onKeyDown = React.useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        setSplit((c) => clamp(c - 5, minAllowed, maxAllowed));
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        setSplit((c) => clamp(c + 5, minAllowed, maxAllowed));
      } else if (e.key === "Home") {
        e.preventDefault();
        setSplit(minAllowed);
      } else if (e.key === "End") {
        e.preventDefault();
        setSplit(maxAllowed);
      } else if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        setSplit(SNAP_CENTRE);
      }
    },
    [minAllowed, maxAllowed],
  );

  // Spring-animated width; under reduced motion the motion.div transition is
  // bypassed (duration: 0).
  const spring = dragging
    ? { duration: 0 }
    : reduced
      ? { duration: 0 }
      : { type: "spring" as const, stiffness: 420, damping: 32, mass: 0.7 };

  return (
    <div
      ref={containerRef}
      className={cn("flex min-h-0 w-full items-stretch", className)}
    >
      <motion.div
        className="min-w-0 overflow-hidden"
        style={{ width: `${split}%` }}
        animate={{ width: `${split}%` }}
        transition={spring}
        data-testid="split-pane-left"
      >
        {left}
      </motion.div>

      <div
        role="separator"
        tabIndex={0}
        aria-orientation="vertical"
        aria-label={ariaLabel}
        aria-valuenow={Math.round(split)}
        aria-valuemin={0}
        aria-valuemax={100}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onKeyDown={onKeyDown}
        data-testid="split-pane-divider"
        className={cn(
          "group relative flex w-2 shrink-0 cursor-col-resize items-center justify-center bg-transparent outline-none",
          "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
        )}
      >
        <span
          aria-hidden="true"
          className={cn(
            "h-full w-px bg-border transition-colors group-hover:bg-accent group-focus-visible:bg-accent",
            dragging && "bg-accent",
          )}
        />
        <span
          aria-hidden="true"
          className={cn(
            "absolute left-1/2 top-1/2 h-8 w-[3px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-border/60 transition-colors group-hover:bg-accent",
            dragging && "bg-accent",
          )}
        />
      </div>

      <motion.div
        className="min-w-0 overflow-hidden"
        style={{ width: `${100 - split}%` }}
        animate={{ width: `${100 - split}%` }}
        transition={spring}
        data-testid="split-pane-right"
      >
        {right}
      </motion.div>
    </div>
  );
}
