"use client";

import * as React from "react";
import { Plus } from "lucide-react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/utils";
import { GlassPanel } from "@/components/ui/glass-panel";
import { StreamPill, type StreamState } from "@/components/ui/stream-pill";
import { CountdownRing } from "@/components/ui/countdown-ring";
import type { CanvasSession } from "@/lib/mocks/canvas";

/**
 * Hero row for the Canvas page.
 *
 * Warm-orange glass-strong panel + lead pill + title + prose summary + a
 * cluster of controls (new-session button, stream pill, TTL countdown ring).
 * State-driven: the pill + prose paragraph rotate through four shapes —
 * `live`, `fallback`, `ended`, `idle` — so the page reads correctly even in
 * the empty / stub state.
 */

export type CanvasHeroState = "live" | "fallback" | "ended" | "idle";

export interface CanvasHeroProps {
  state: CanvasHeroState;
  session: CanvasSession | null;
  pendingFrames: number;
  remainingMs: number;
  streamState: StreamState;
  streamRate?: string;
  creating: boolean;
  onNewSession: () => void;
}

export function CanvasHero({
  state,
  session,
  pendingFrames,
  remainingMs,
  streamState,
  streamRate,
  creating,
  onNewSession,
}: CanvasHeroProps) {
  const { t } = useTranslation();

  const heroProse =
    state === "ended"
      ? t("canvas.tp.heroProseIdle")
      : state === "fallback"
        ? t("canvas.tp.heroProseFallback")
        : state === "live" && session
          ? t(
              pendingFrames === 1
                ? "canvas.tp.heroProseSession"
                : "canvas.tp.heroProseSessionPlural",
              { pending: pendingFrames },
            )
          : t("canvas.tp.heroProseIdle");

  const leadPillText =
    state === "ended"
      ? t("canvas.tp.leadPillEnded")
      : state === "fallback"
        ? t("canvas.tp.leadPillFallback")
        : session
          ? t("canvas.tp.leadPillLive", { sessionId: session.id })
          : t("canvas.tp.leadPillIdle");

  const dotClass =
    state === "live"
      ? "bg-tp-amber tp-breathe-amber"
      : state === "fallback"
        ? "bg-tp-warn tp-breathe-amber"
        : "bg-tp-ink-4";

  return (
    <GlassPanel
      as="header"
      variant="strong"
      className="relative overflow-hidden p-6 sm:p-7"
    >
      <div
        aria-hidden
        className="pointer-events-none absolute bottom-[-90px] right-[-40px] h-[220px] w-[360px] rounded-full opacity-60 blur-3xl"
        style={{
          background:
            "radial-gradient(closest-side, var(--tp-amber-glow), transparent 70%)",
        }}
      />
      <div
        aria-hidden
        className="pointer-events-none absolute top-[-60px] left-[-40px] h-[180px] w-[260px] rounded-full opacity-40 blur-[50px]"
        style={{
          background:
            "radial-gradient(closest-side, color-mix(in oklch, var(--tp-ember) 35%, transparent), transparent 70%)",
        }}
      />

      <div className="relative flex min-w-0 flex-col gap-4">
        <div
          className={cn(
            "inline-flex w-fit items-center gap-2.5 rounded-full border py-1 pl-2 pr-3",
            "border-tp-glass-edge bg-tp-glass-inner-strong",
            "font-mono text-[11px] text-tp-ink-2",
          )}
        >
          <span className={cn("h-1.5 w-1.5 rounded-full", dotClass)} />
          <span
            data-testid={session ? "canvas-session-id" : undefined}
            className="font-mono text-[11px] text-tp-ink-2"
          >
            {leadPillText}
          </span>
        </div>

        <h1 className="font-sans text-[28px] font-semibold leading-[1.15] tracking-[-0.025em] text-tp-ink sm:text-[32px]">
          {t("canvas.tp.heroTitle")}
        </h1>

        <p className="max-w-[68ch] text-[14.5px] leading-[1.6] text-tp-ink-2">
          <span className="text-tp-ink">{t("canvas.tp.heroProseLive")}</span>{" "}
          <span className="text-tp-ink-3">{heroProse}</span>
        </p>

        <div className="mt-1 flex flex-wrap items-center gap-2.5">
          <button
            type="button"
            onClick={onNewSession}
            disabled={creating}
            aria-label={t("canvas.tp.ctaNewSessionAria")}
            data-testid="canvas-new-session"
            className={cn(
              "inline-flex items-center gap-2 rounded-lg border px-3 py-2",
              "text-[13px] font-medium",
              "border-tp-amber/35 bg-tp-amber-soft text-tp-amber",
              "transition-colors hover:bg-[color-mix(in_oklch,var(--tp-amber)_22%,transparent)]",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-tp-amber/50",
              "disabled:cursor-not-allowed disabled:opacity-60",
            )}
          >
            <Plus className="h-3.5 w-3.5" aria-hidden />
            {creating ? t("common.loading") : t("canvas.tp.ctaNewSession")}
          </button>

          <StreamPill state={streamState} rate={streamRate} />

          {session && state !== "ended" ? (
            <CountdownRing
              remainingMs={remainingMs}
              totalMs={session.ttl_ms}
              label={t("canvas.ttlLabel")}
            />
          ) : null}
        </div>
      </div>
    </GlassPanel>
  );
}

export default CanvasHero;
