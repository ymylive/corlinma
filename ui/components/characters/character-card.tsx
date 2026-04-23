"use client";

import * as React from "react";
import { AnimatePresence, motion } from "framer-motion";

import { cn } from "@/lib/utils";
import { useMotion } from "@/components/ui/motion-safe";
import type { AgentCard } from "@/lib/mocks/characters";

/**
 * Card-deck metaphor for the Characters page (B2-FE4).
 *
 * Interaction model:
 *   - Default face ("back") shows emoji + name + 1-line description.
 *   - Click / Enter / Space flips the card (rotateY 0 → 180).
 *   - The flipped face ("front") shows description + top tools + an Edit button.
 *   - Clicking Edit invokes `onEdit`; the parent opens a drawer and, when the
 *     drawer closes, toggles `flipped` back to false.
 *
 * Flip technique:
 *   3D CSS flip — parent wrapper rotates, each face has `backfaceVisibility:
 *   hidden`, the front face is pre-rotated 180° so it reads rightside-up when
 *   the wrapper reaches 180°. Under `prefers-reduced-motion` we swap the two
 *   faces via `<AnimatePresence mode="wait">` instead (instant cut, no
 *   rotation) — matches the motion-safety pattern used elsewhere in the UI.
 */
export interface CharacterCardProps {
  card: AgentCard;
  flipped: boolean;
  /** Slight random rotate variance in degrees (-1..+1). Passed from parent so
   * the deterministic per-name hash stays stable across rerenders. */
  rotateDeg: number;
  onFlip: () => void;
  onEdit: () => void;
}

export function CharacterCard({
  card,
  flipped,
  rotateDeg,
  onFlip,
  onEdit,
}: CharacterCardProps) {
  const { reduced } = useMotion();
  const displayEmoji = card.emoji || firstGlyph(card.name);
  const label = flipped
    ? `${card.name} — details, press enter to flip back`
    : `${card.name} — ${card.description}, press enter to flip`;

  function onKeyDown(e: React.KeyboardEvent<HTMLDivElement>) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onFlip();
    }
  }

  return (
    <div
      className="relative h-[320px] w-full [perspective:1200px]"
      style={{ transform: `rotate(${rotateDeg}deg)` }}
      data-testid={`character-card-${card.name}`}
    >
      {reduced ? (
        // Reduced motion: no 3D rotation. Swap faces with a plain crossfade.
        <AnimatePresence mode="wait" initial={false}>
          {flipped ? (
            <motion.div
              key="front"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0 }}
              className="absolute inset-0"
            >
              <FrontFace
                card={card}
                flipped
                label={label}
                onFlip={onFlip}
                onEdit={onEdit}
                onKeyDown={onKeyDown}
              />
            </motion.div>
          ) : (
            <motion.div
              key="back"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0 }}
              className="absolute inset-0"
            >
              <BackFace
                card={card}
                displayEmoji={displayEmoji}
                label={label}
                onFlip={onFlip}
                onKeyDown={onKeyDown}
              />
            </motion.div>
          )}
        </AnimatePresence>
      ) : (
        <motion.div
          className="relative h-full w-full"
          animate={{ rotateY: flipped ? 180 : 0 }}
          transition={{ type: "spring", stiffness: 200, damping: 22 }}
          style={{ transformStyle: "preserve-3d" }}
        >
          <BackFace
            card={card}
            displayEmoji={displayEmoji}
            label={label}
            onFlip={onFlip}
            onKeyDown={onKeyDown}
            // back sits at 0° by default
            style={{ backfaceVisibility: "hidden", WebkitBackfaceVisibility: "hidden" }}
          />
          <FrontFace
            card={card}
            flipped={flipped}
            label={label}
            onFlip={onFlip}
            onEdit={onEdit}
            onKeyDown={onKeyDown}
            // Pre-rotated so it reads rightside-up when the wrapper is at 180°.
            style={{
              transform: "rotateY(180deg)",
              backfaceVisibility: "hidden",
              WebkitBackfaceVisibility: "hidden",
            }}
          />
        </motion.div>
      )}
    </div>
  );
}

// --- faces -----------------------------------------------------------------

interface FaceProps {
  label: string;
  onFlip: () => void;
  onKeyDown: (e: React.KeyboardEvent<HTMLDivElement>) => void;
  style?: React.CSSProperties;
}

interface BackFaceProps extends FaceProps {
  card: AgentCard;
  displayEmoji: string;
}

function BackFace({ card, displayEmoji, label, onFlip, onKeyDown, style }: BackFaceProps) {
  return (
    <motion.div
      role="button"
      tabIndex={0}
      aria-pressed={false}
      aria-label={label}
      onClick={onFlip}
      onKeyDown={onKeyDown}
      whileHover={{ y: -2, boxShadow: "var(--shadow-3)" }}
      transition={{ type: "spring", stiffness: 300, damping: 24 }}
      className={cn(
        "absolute inset-0 flex cursor-pointer select-none flex-col gap-3 rounded-2xl border border-border bg-gradient-to-br from-accent-2/30 via-panel to-accent-3/20 p-5 text-card-foreground shadow-2",
        "outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
      )}
      style={style}
      data-testid={`character-card-back-${card.name}`}
    >
      <div className="flex items-start justify-between">
        <span
          aria-hidden="true"
          className="text-4xl leading-none drop-shadow-sm"
        >
          {displayEmoji}
        </span>
        <span className="rounded-full bg-background/50 px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
          card
        </span>
      </div>
      <div className="mt-auto space-y-1.5">
        <h3 className="text-lg font-semibold leading-tight tracking-tight">
          {card.name}
        </h3>
        <p className="line-clamp-2 text-xs leading-relaxed text-muted-foreground">
          {card.description}
        </p>
      </div>
    </motion.div>
  );
}

interface FrontFaceProps extends FaceProps {
  card: AgentCard;
  flipped: boolean;
  onEdit: () => void;
}

function FrontFace({ card, flipped, label, onFlip, onEdit, onKeyDown, style }: FrontFaceProps) {
  const topTools = card.tools_allowed.slice(0, 3);
  return (
    <div
      role="button"
      tabIndex={0}
      aria-pressed={flipped}
      aria-label={label}
      onClick={onFlip}
      onKeyDown={onKeyDown}
      className={cn(
        "absolute inset-0 flex cursor-pointer select-none flex-col gap-3 rounded-2xl border border-border bg-panel p-5 text-card-foreground shadow-3",
        "outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
      )}
      style={style}
      data-testid={`character-card-front-${card.name}`}
    >
      <header className="space-y-1">
        <h3 className="text-base font-semibold leading-tight">{card.name}</h3>
        <p className="text-xs leading-relaxed text-muted-foreground">
          {card.description}
        </p>
      </header>
      <div className="flex-1 space-y-2">
        <div className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
          tools
        </div>
        <ul className="flex flex-wrap gap-1.5">
          {topTools.length === 0 ? (
            <li className="text-xs italic text-muted-foreground">none</li>
          ) : (
            topTools.map((tool) => (
              <li
                key={tool}
                className="rounded-md border border-border bg-background/60 px-2 py-0.5 font-mono text-[10px] text-foreground"
              >
                {tool}
              </li>
            ))
          )}
        </ul>
      </div>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onEdit();
        }}
        className="self-end rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground shadow-1 transition-colors hover:bg-primary/90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
        data-testid={`character-card-edit-${card.name}`}
      >
        Edit
      </button>
    </div>
  );
}

// --- helpers ---------------------------------------------------------------

/**
 * First visible glyph of a string — honours surrogate pairs so an opening
 * emoji lands intact. Used as the fallback when an AgentCard has no explicit
 * emoji.
 */
function firstGlyph(s: string): string {
  if (!s) return "✨";
  const iter = s[Symbol.iterator]();
  const { value } = iter.next();
  return value ?? "✨";
}

/**
 * Deterministic hash of a string → a number in [-1, 1]. Used to give each
 * card a stable tilt across rerenders (a naive `Math.random()` would jitter
 * every time React reconciles).
 */
export function tiltForName(name: string): number {
  let h = 0;
  for (let i = 0; i < name.length; i++) {
    h = (h * 31 + name.charCodeAt(i)) | 0;
  }
  // Map to [-1, 1] with two decimals.
  const norm = ((h % 2000) + 2000) % 2000; // [0, 2000)
  return Math.round(norm - 1000) / 1000;
}
