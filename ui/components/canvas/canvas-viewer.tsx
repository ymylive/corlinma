"use client";

import * as React from "react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/utils";
import { GlassPanel } from "@/components/ui/glass-panel";

/**
 * Canvas viewer surface — a `GlassPanel` that hosts the sandboxed iframe used
 * by the Canvas Host protocol.
 *
 * In dev / stub mode the iframe body is a warm placeholder document (the host
 * hasn't rendered anything yet); we overlay a centred prose card on top so the
 * empty state reads on-theme instead of leaking the raw placeholder. The
 * overlay fades out when real content lands — left to the caller by toggling
 * `showOverlay`.
 *
 * The inner `<iframe>` keeps `data-testid="canvas-iframe"` and its
 * `sandbox="allow-same-origin"` attribute — both consumed by the canvas page
 * test suite, so they must not move.
 */

const SKELETON_MS = 800;

export interface CanvasViewerProps {
  /** Pre-rendered placeholder document for the iframe. */
  srcDoc: string;
  /** Session id bound to the current surface. `null` ⇒ no session yet. */
  sessionId: string | null;
  /** User-level reduced-motion preference. */
  reduced: boolean;
  /** When true, render the warm empty-state overlay on top of the iframe. */
  showOverlay: boolean;
  /** Additional hint shown below the empty-state headline. */
  hintText?: string;
}

export function CanvasViewer({
  srcDoc,
  sessionId,
  reduced,
  showOverlay,
  hintText,
}: CanvasViewerProps) {
  const { t } = useTranslation();
  const [showSkeleton, setShowSkeleton] = React.useState(!reduced);

  // Reset the skeleton shimmer every time the session rotates.
  React.useEffect(() => {
    if (reduced) {
      setShowSkeleton(false);
      return;
    }
    setShowSkeleton(true);
    const timer = window.setTimeout(() => setShowSkeleton(false), SKELETON_MS);
    return () => window.clearTimeout(timer);
  }, [reduced, sessionId]);

  return (
    <GlassPanel
      as="section"
      variant="strong"
      className="relative flex flex-col overflow-hidden"
      aria-label={t("canvas.tp.viewerTitle")}
    >
      {/* 16:9 aspect ratio — falls back to explicit height on older browsers. */}
      <div className="relative w-full" style={{ aspectRatio: "16 / 9", minHeight: 340 }}>
        <iframe
          data-testid="canvas-iframe"
          title="Canvas surface placeholder"
          sandbox="allow-same-origin"
          srcDoc={srcDoc}
          className="absolute inset-0 h-full w-full border-0 bg-tp-glass-inner"
        />

        {/* Skeleton shimmer — only on first mount / session rotation. */}
        {showSkeleton ? (
          <div
            aria-hidden
            data-testid="canvas-skeleton"
            className={cn(
              "absolute inset-0 pointer-events-none",
              reduced ? "bg-tp-glass-inner/60" : "shimmer",
            )}
          />
        ) : null}

        {/* Warm prose overlay — covers the iframe while no real content has
         * been rendered. Uses the same glass/ink tokens as the rest of the
         * page so it reads as a viewer-level empty state, not a modal. */}
        {showOverlay ? (
          <div
            aria-hidden={!showOverlay}
            className={cn(
              "absolute inset-0 flex flex-col items-center justify-center gap-3 px-6 text-center",
              "bg-[radial-gradient(closest-side,color-mix(in_oklch,var(--tp-glass)_85%,transparent),color-mix(in_oklch,var(--tp-glass)_95%,transparent))]",
              "backdrop-blur-[6px]",
            )}
          >
            <div className="font-mono text-[10.5px] uppercase tracking-[0.14em] text-tp-ink-4">
              {t("canvas.tp.viewerTitle")}
            </div>
            <div className="max-w-[32ch] font-sans text-[17px] font-medium leading-[1.35] tracking-[-0.01em] text-tp-ink">
              {t("canvas.tp.viewerEmpty")}
            </div>
            <div className="max-w-[42ch] text-[12.5px] leading-[1.55] text-tp-ink-3">
              {hintText ?? t("canvas.tp.viewerEmptyHint")}
            </div>
            {sessionId ? (
              <div className="mt-1 inline-flex items-center gap-2 rounded-full border border-tp-glass-edge bg-tp-glass-inner px-2.5 py-0.5 font-mono text-[10.5px] text-tp-ink-3">
                <span className="text-tp-ink-4">
                  {t("canvas.tp.viewerSessionLabel")}
                </span>
                <span className="text-tp-ink-2">{sessionId}</span>
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
    </GlassPanel>
  );
}

export default CanvasViewer;
