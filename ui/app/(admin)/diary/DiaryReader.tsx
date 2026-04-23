"use client";

import * as React from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { AnimatePresence, motion } from "framer-motion";
import { X } from "lucide-react";

import { cn } from "@/lib/utils";
import { useMotion } from "@/components/ui/motion-safe";
import { timeLabel, type DiaryEntry } from "@/lib/mocks/diary";

export interface DiaryReaderProps {
  entry: DiaryEntry | null;
  onClose: () => void;
}

/**
 * Full-body reader for a diary entry.
 *
 * Implementation:
 *   - Radix `Dialog` provides the focus trap, Escape-to-close + scroll lock.
 *   - framer-motion `layoutId={`entry-${id}`}` links the card in the list
 *     to the modal panel so the morph is a true shared-layout animation.
 *   - Under `prefers-reduced-motion: reduce` the layoutId is dropped and
 *     `<AnimatePresence mode="wait">` performs an instant crossfade.
 *
 * Markdown rendering is intentionally naive — a `<pre>` preserving the raw
 * text keeps the mock readable without pulling `marked` into the bundle.
 */
export function DiaryReader({ entry, onClose }: DiaryReaderProps) {
  const { reduced } = useMotion();
  const open = entry !== null;
  const shouldMorph = !reduced;

  return (
    <DialogPrimitive.Root
      open={open}
      onOpenChange={(v) => {
        if (!v) onClose();
      }}
    >
      <AnimatePresence mode="wait">
        {entry ? (
          <DialogPrimitive.Portal forceMount>
            <DialogPrimitive.Overlay asChild forceMount>
              <motion.div
                key="overlay"
                className="fixed inset-0 z-50 bg-black/70"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                transition={{ duration: reduced ? 0 : 0.18 }}
              />
            </DialogPrimitive.Overlay>
            <DialogPrimitive.Content asChild forceMount>
              <motion.div
                key={`reader-${entry.id}`}
                layoutId={shouldMorph ? `entry-${entry.id}` : undefined}
                data-testid={`diary-reader-${entry.id}`}
                initial={shouldMorph ? false : { opacity: 0 }}
                animate={shouldMorph ? undefined : { opacity: 1 }}
                exit={shouldMorph ? undefined : { opacity: 0 }}
                transition={{
                  type: "spring",
                  stiffness: 380,
                  damping: 34,
                  duration: reduced ? 0 : undefined,
                }}
                className={cn(
                  "fixed left-1/2 top-1/2 z-50 w-[min(92vw,640px)]",
                  "max-h-[85vh] -translate-x-1/2 -translate-y-1/2 overflow-auto",
                  "rounded-xl border border-tp-glass-edge bg-tp-glass-3 p-6 backdrop-blur-glass-strong",
                  "shadow-[inset_0_1px_0_var(--tp-glass-hl),0_30px_60px_-30px_rgba(0,0,0,0.5)]",
                )}
              >
                <div className="mb-4 flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="font-mono text-[11px] tabular-nums text-tp-ink-3">
                      {new Date(entry.created_at_ms).toISOString().slice(0, 10)}
                      {" · "}
                      {timeLabel(entry.created_at_ms)}
                      {entry.session_key ? (
                        <>
                          {" · "}
                          <span className="text-tp-ink-4">
                            session={entry.session_key}
                          </span>
                        </>
                      ) : null}
                    </div>
                    <DialogPrimitive.Title asChild>
                      <h2 className="mt-1 truncate font-serif text-[22px] font-normal leading-tight tracking-[-0.015em] text-tp-ink">
                        {entry.title}
                      </h2>
                    </DialogPrimitive.Title>
                    <DialogPrimitive.Description className="sr-only">
                      Full diary entry for {entry.title}.
                    </DialogPrimitive.Description>
                  </div>
                  <DialogPrimitive.Close
                    className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-tp-ink-3 transition-colors hover:bg-tp-glass-inner-hover hover:text-tp-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-tp-amber/40"
                    aria-label="Close reader"
                    data-testid="diary-reader-close"
                  >
                    <X className="h-4 w-4" />
                  </DialogPrimitive.Close>
                </div>

                {entry.tags.length > 0 ? (
                  <div className="mb-3 flex flex-wrap gap-1">
                    {entry.tags.map((t) => (
                      <span
                        key={t}
                        className="rounded bg-tp-glass-inner-strong px-1.5 py-0.5 font-mono text-[10px] text-tp-ink-3"
                      >
                        #{t}
                      </span>
                    ))}
                  </div>
                ) : null}

                <pre className="whitespace-pre-wrap break-words font-sans text-[14px] leading-relaxed text-tp-ink-2">
                  {entry.body_markdown}
                </pre>
              </motion.div>
            </DialogPrimitive.Content>
          </DialogPrimitive.Portal>
        ) : null}
      </AnimatePresence>
    </DialogPrimitive.Root>
  );
}
