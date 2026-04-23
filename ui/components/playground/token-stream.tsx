"use client";

import * as React from "react";
import { AnimatePresence, motion } from "framer-motion";

import { useMotion } from "@/components/ui/motion-safe";
import { useMotionVariants } from "@/lib/motion";
import { cn } from "@/lib/utils";
import { diffLineIndexes } from "./diff-highlight";

export interface TokenStreamProps {
  /** Token array rendered left-to-right. Re-renders are append-only. */
  tokens: string[];
  /** When true, fade the "streaming" dot + stop any pending animations. */
  done?: boolean;
  /** Text of the *other* pane; used to pulse divergent lines once both done. */
  peerText?: string;
  /** Both streams finished — triggers the diff pulse pass. */
  diffReady?: boolean;
  className?: string;
  "data-testid"?: string;
}

/**
 * Renders a growing token stream with token-by-token fade-in. Each token is
 * wrapped in a `<motion.span>` that mounts via the shared `fadeUp` variant;
 * under reduced-motion we skip the animation entirely and just print the
 * text verbatim.
 *
 * When `diffReady` is true, the component computes a line diff against
 * `peerText` and pulse-glows the divergent lines for ~800ms. The highlight
 * is deliberately subtle — it's a scanning cue, not a blocker.
 */
export function TokenStream({
  tokens,
  done = false,
  peerText = "",
  diffReady = false,
  className,
  "data-testid": testId,
}: TokenStreamProps) {
  const { reduced } = useMotion();
  const variants = useMotionVariants();

  const fullText = React.useMemo(() => tokens.join(""), [tokens]);
  const lines = React.useMemo(() => fullText.split("\n"), [fullText]);

  const differingLines = React.useMemo(() => {
    if (!diffReady) return new Set<number>();
    return diffLineIndexes(fullText, peerText);
  }, [diffReady, fullText, peerText]);

  // Under reduced motion, render a flat <pre> — no AnimatePresence overhead
  // and no per-token fade.
  if (reduced) {
    return (
      <pre
        role="log"
        aria-live="polite"
        aria-atomic="false"
        data-testid={testId}
        className={cn(
          "m-0 whitespace-pre-wrap break-words font-mono text-xs leading-relaxed text-foreground",
          className,
        )}
      >
        {lines.map((line, i) => (
          <span
            key={i}
            data-line-index={i}
            data-differs={differingLines.has(i) ? "true" : "false"}
            className={cn("block", done === false && "opacity-95")}
          >
            {line}
            {i < lines.length - 1 ? "\n" : ""}
          </span>
        ))}
      </pre>
    );
  }

  return (
    <div
      role="log"
      aria-live="polite"
      aria-atomic="false"
      data-testid={testId}
      className={cn(
        "whitespace-pre-wrap break-words font-mono text-xs leading-relaxed text-foreground",
        className,
      )}
    >
      {lines.map((line, i) => {
        const differs = differingLines.has(i);
        return (
          <span
            key={`line-${i}`}
            data-line-index={i}
            data-differs={differs ? "true" : "false"}
            className={cn(
              "block rounded-sm px-1 -mx-1",
              differs && diffReady && "animate-pulse-glow",
            )}
          >
            {line}
            {i < lines.length - 1 ? "\n" : ""}
          </span>
        );
      })}
      {/* Animated caret-like reveal: mount the *last* token with fadeUp so
          new tokens pop in even though the flat lines array keeps semantic
          structure simple. Using AnimatePresence with a key tied to token
          count gives us a single fade per new token without re-animating
          the whole stream. */}
      <AnimatePresence initial={false}>
        {!done && tokens.length > 0 ? (
          <motion.span
            key={`caret-${tokens.length}`}
            aria-hidden="true"
            variants={variants.fadeUp}
            initial="hidden"
            animate="visible"
            className="inline-block h-3 w-[6px] translate-y-[1px] bg-accent/60"
          />
        ) : null}
      </AnimatePresence>
    </div>
  );
}
