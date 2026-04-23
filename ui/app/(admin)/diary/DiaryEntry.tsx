"use client";

import * as React from "react";
import { motion, useInView } from "framer-motion";

import { cn } from "@/lib/utils";
import { useMotion } from "@/components/ui/motion-safe";
import { timeLabel, type DiaryEntry } from "@/lib/mocks/diary";

export interface DiaryEntryCardProps {
  entry: DiaryEntry;
  /** 0-based position within its date group. Drives the stagger delay. */
  indexInGroup: number;
  onOpen: (entry: DiaryEntry) => void;
}

/**
 * Single diary card. Fades up on viewport enter (IntersectionObserver via
 * framer-motion's `useInView`) and carries a stable `layoutId` so the
 * reader modal can morph into place via shared layout animation.
 *
 * Reduced-motion: no fade-up class is applied, no `layoutId` transition
 * delay — the card appears instantly. The `data-entry-animated` attribute
 * gives tests a stable hook for asserting motion state.
 */
export function DiaryEntryCard({
  entry,
  indexInGroup,
  onOpen,
}: DiaryEntryCardProps) {
  const { reduced } = useMotion();
  const ref = React.useRef<HTMLDivElement>(null);
  // `once: true` so pre-rendering entries on mount doesn't re-fire as the
  // user scrolls back up. 100px bottom margin gives a small lead so the
  // reveal starts *before* the card is fully on-screen.
  const inView = useInView(ref, {
    once: true,
    margin: "0px 0px -100px 0px",
  });

  const shouldAnimate = !reduced;
  // 40ms stagger per entry within a group.
  const delay = shouldAnimate ? indexInGroup * 0.04 : 0;

  const previewLine = firstNonTitleLine(entry.body_markdown);

  function onClick() {
    onOpen(entry);
  }
  function onKeyDown(e: React.KeyboardEvent<HTMLDivElement>) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onOpen(entry);
    }
  }

  return (
    <motion.div
      ref={ref}
      layoutId={shouldAnimate ? `entry-${entry.id}` : undefined}
      role="listitem"
      data-testid={`diary-entry-${entry.id}`}
      data-entry-animated={shouldAnimate ? "true" : "false"}
      initial={shouldAnimate ? { opacity: 0, y: 20 } : false}
      animate={
        shouldAnimate
          ? inView
            ? { opacity: 1, y: 0 }
            : { opacity: 0, y: 20 }
          : { opacity: 1, y: 0 }
      }
      transition={{
        duration: shouldAnimate ? 0.28 : 0,
        ease: [0.22, 1, 0.36, 1],
        delay,
      }}
      className={cn(
        "relative ml-6 rounded-lg border border-border bg-card/40 px-4 py-3",
        shouldAnimate && "diary-entry-animated",
      )}
    >
      {/* Timeline dot — sits in the gutter, anchored to the left rail. */}
      <span
        aria-hidden
        className="absolute -left-[22px] top-4 h-2.5 w-2.5 rounded-full border-2 border-background bg-primary"
      />
      <div
        role="button"
        tabIndex={0}
        aria-label={`${entry.title} — open full entry`}
        onClick={onClick}
        onKeyDown={onKeyDown}
        className="cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
      >
        <div className="flex items-baseline gap-2">
          <span className="font-mono text-[11px] tabular-nums text-muted-foreground">
            {timeLabel(entry.created_at_ms)}
          </span>
          <h3 className="text-sm font-medium text-foreground">{entry.title}</h3>
        </div>
        {previewLine ? (
          <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">
            {previewLine}
          </p>
        ) : null}
        {entry.tags.length > 0 ? (
          <div className="mt-2 flex flex-wrap gap-1">
            {entry.tags.map((t) => (
              <span
                key={t}
                className="rounded bg-accent/50 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
              >
                #{t}
              </span>
            ))}
          </div>
        ) : null}
      </div>
    </motion.div>
  );
}

/**
 * Plucks the first non-heading, non-empty line from a markdown body for use
 * as a preview. Keeps the timeline scannable without rendering markdown.
 */
function firstNonTitleLine(md: string): string {
  for (const raw of md.split("\n")) {
    const line = raw.trim();
    if (!line) continue;
    if (line.startsWith("#")) continue;
    if (line.startsWith("-") || line.startsWith("*")) continue;
    return line;
  }
  return "";
}
