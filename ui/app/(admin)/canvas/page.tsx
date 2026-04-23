"use client";

import * as React from "react";
import { AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/utils";
import { StatChip } from "@/components/ui/stat-chip";
import type { StreamState } from "@/components/ui/stream-pill";
import { useMotion } from "@/components/ui/motion-safe";
import {
  canvasEventsPath,
  createCanvasSession,
  sendCanvasFrame,
} from "@/lib/api/canvas";
import { openEventStream } from "@/lib/sse";
import {
  type CanvasEvent,
  type CanvasFrameKind,
  type CanvasSession,
  buildMockEvent,
  formatCanvasTime,
} from "@/lib/mocks/canvas";

import {
  CanvasHero,
  type CanvasHeroState,
} from "@/components/canvas/canvas-hero";
import { CanvasViewer } from "@/components/canvas/canvas-viewer";
import { MessageInspector } from "@/components/canvas/message-inspector";
import {
  buildPlaceholderHtml,
  formatBytes,
} from "@/components/canvas/placeholder-html";
import { SendFrameForm } from "./SendFrameForm";

/**
 * B5-FE3 Canvas viewer — Phase 5d Tidepool cutover.
 *
 * Thin coordinator: owns the session + SSE lifecycle and delegates visuals to
 * four sub-components under `components/canvas/*`:
 *
 *   - `<CanvasHero>` — warm glass-strong hero with lead pill, prose, CTA,
 *     StreamPill, and the TTL countdown ring.
 *   - `<CanvasViewer>` — sandboxed iframe inside a glass-strong panel, with a
 *     centred prose overlay when no frames have landed yet.
 *   - `<SendFrameForm>` (pre-existing, already retokened in Phase 5a) —
 *     small composer to post a frame into the current session.
 *   - `<MessageInspector>` — bottom-docked glass-soft protocol inspector with
 *     filter chips, compact rows, and a side-docked JSON payload at wide
 *     viewports (inline expand at narrow).
 *
 * Fallback path: a rotating mock stream drives the UI when the gateway
 * reports the Canvas Host is disabled — kept identical to the pre-cutover
 * behaviour so the stub stays demoable in dev.
 */

const EVENT_RING_MAX = 200;

/** Baked sparks — same geometry language as other Tidepool pages. */
const FRAMES_SPARK =
  "M0 28 L30 26 L60 20 L90 24 L120 16 L150 22 L180 14 L210 18 L240 10 L270 14 L300 8 L300 36 L0 36 Z";
const BYTES_SPARK =
  "M0 22 L30 24 L60 20 L90 22 L120 18 L150 20 L180 14 L210 18 L240 12 L270 16 L300 10 L300 36 L0 36 Z";

export default function CanvasPage() {
  const { t } = useTranslation();
  const { reduced } = useMotion();

  const [session, setSession] = React.useState<CanvasSession | null>(null);
  const [fallback, setFallback] = React.useState(false);
  const [events, setEvents] = React.useState<CanvasEvent[]>([]);
  const [newestId, setNewestId] = React.useState<string | undefined>();
  const [sending, setSending] = React.useState(false);
  const [ended, setEnded] = React.useState(false);
  const [creating, setCreating] = React.useState(false);
  const [nowTick, setNowTick] = React.useState<number>(Date.now());

  // Keep the countdown ring fresh after mount.
  React.useEffect(() => {
    const i = window.setInterval(() => setNowTick(Date.now()), 1_000);
    return () => window.clearInterval(i);
  }, []);

  const pushEvent = React.useCallback((ev: CanvasEvent) => {
    setEvents((prev) => [ev, ...prev].slice(0, EVENT_RING_MAX));
    setNewestId(ev.id);
  }, []);

  // ── Session lifecycle ───────────────────────────────────────

  const newSession = React.useCallback(async () => {
    setCreating(true);
    setEnded(false);
    setEvents([]);
    try {
      const res = await createCanvasSession();
      setSession(res.session);
      setFallback(res.kind === "fallback");
    } catch {
      // Any non-503 failure still falls back so the page stays demoable.
      setFallback(true);
      const local = buildMockEvent(0);
      setEvents([local]);
    } finally {
      setCreating(false);
    }
  }, []);

  // Bootstrap a session on first mount.
  React.useEffect(() => {
    void newSession();
  }, [newSession]);

  // ── SSE / fallback rotation ─────────────────────────────────

  React.useEffect(() => {
    if (!session) return;

    if (fallback) {
      let idx = 0;
      pushEvent(buildMockEvent(idx));
      idx += 1;
      const timer = window.setInterval(() => {
        pushEvent(buildMockEvent(idx));
        idx += 1;
      }, 2_000);
      return () => window.clearInterval(timer);
    }

    const close = openEventStream<{
      kind: CanvasFrameKind;
      payload: Record<string, unknown>;
    }>(canvasEventsPath(session.id), {
      events: ["canvas", "end"],
      onMessage: ({ event, data }) => {
        if (event === "end") {
          setEnded(true);
          return;
        }
        if (event === "canvas") {
          pushEvent({
            id: `ev_${Date.now()}_${Math.random().toString(16).slice(2, 6)}`,
            timestamp: formatCanvasTime(Date.now()),
            kind: data.kind,
            payload: data.payload,
          });
        }
      },
    });
    return close;
  }, [session, fallback, pushEvent]);

  // ── Send frame ──────────────────────────────────────────────

  const handleSend = async (
    kind: CanvasFrameKind,
    payload: Record<string, unknown>,
  ) => {
    if (!session) return;
    setSending(true);
    try {
      const res = await sendCanvasFrame({
        session_id: session.id,
        kind,
        payload,
      });
      if (res.kind === "fallback") setFallback(true);

      // Mirror the frame into the inspector immediately — the `_sent` suffix
      // is what `<MessageInspector>` reads to flag the direction as outbound.
      pushEvent({
        id: `ev_${Date.now()}_sent`,
        timestamp: formatCanvasTime(Date.now()),
        kind,
        payload,
      });
    } finally {
      setSending(false);
    }
  };

  // ── Derived state ───────────────────────────────────────────

  const sessionCount = session ? 1 : 0;
  const pendingFrames = events.length;

  /** Approximate frames-per-minute from the in-memory ring. Real metrics will
   * ship in B5 — this stays honest for the stub by capping the window. */
  const framesPerMin = React.useMemo(() => {
    if (events.length === 0) return 0;
    const windowCount = Math.min(events.length, 30);
    return windowCount * 2;
  }, [events]);

  const bytesPerMin = React.useMemo(() => {
    if (events.length === 0) return 0;
    let total = 0;
    for (const e of events.slice(0, 30)) {
      try {
        total += JSON.stringify(e.payload).length;
      } catch {
        /* skip unserialisable payloads */
      }
    }
    return total * 2;
  }, [events]);

  const heroState: CanvasHeroState = ended
    ? "ended"
    : fallback
      ? "fallback"
      : session
        ? "live"
        : "idle";

  const streamState: StreamState =
    heroState === "ended" || heroState === "idle"
      ? "paused"
      : heroState === "fallback"
        ? "throttled"
        : "live";

  const streamRate = React.useMemo(() => {
    if (heroState === "ended") return undefined;
    if (events.length === 0) return "0 ev/min";
    return `${framesPerMin} ev/min`;
  }, [heroState, events.length, framesPerMin]);

  const statSessionsFoot = ended
    ? t("canvas.tp.statSessionsFootEnded")
    : session
      ? t("canvas.tp.statSessionsFootActive")
      : t("canvas.tp.statSessionsFootIdle");

  const placeholderHtml = React.useMemo(
    () => buildPlaceholderHtml(session?.id ?? "(no session)"),
    [session?.id],
  );

  const remainingMs = session ? Math.max(0, session.expires_at - nowTick) : 0;
  // `nowTick` drives the effect above + the remaining-ms recalculation.
  void nowTick;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4">
      <CanvasHero
        state={heroState}
        session={session}
        pendingFrames={pendingFrames}
        remainingMs={remainingMs}
        streamState={streamState}
        streamRate={streamRate}
        creating={creating}
        onNewSession={newSession}
      />

      {fallback ? (
        <div
          role="status"
          data-testid="canvas-fallback-banner"
          className={cn(
            "flex items-start gap-2 rounded-xl border px-3 py-2 text-[12.5px]",
            "border-tp-warn/30 bg-tp-warn-soft text-tp-warn",
          )}
        >
          <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden />
          <span>{t("canvas.fallbackBanner")}</span>
        </div>
      ) : null}

      <section className="grid grid-cols-1 gap-3.5 md:grid-cols-3">
        <StatChip
          variant={session && !ended ? "primary" : "default"}
          live={!!session && !ended && !fallback}
          label={t("canvas.tp.statSessions")}
          value={sessionCount}
          foot={statSessionsFoot}
          sparkPath={FRAMES_SPARK}
          sparkTone="amber"
        />
        <StatChip
          label={t("canvas.tp.statFrames")}
          value={framesPerMin}
          foot={t("canvas.tp.statFramesFoot")}
          sparkPath={FRAMES_SPARK}
          sparkTone="ember"
        />
        <StatChip
          label={t("canvas.tp.statBytes")}
          value={formatBytes(bytesPerMin)}
          foot={t("canvas.tp.statBytesFoot")}
          sparkPath={BYTES_SPARK}
          sparkTone="peach"
        />
      </section>

      <section className="grid gap-3.5 lg:grid-cols-[minmax(0,1fr)_320px]">
        <CanvasViewer
          srcDoc={placeholderHtml}
          sessionId={session?.id ?? null}
          reduced={reduced}
          showOverlay={events.length === 0 || !session}
          hintText={
            fallback ? t("canvas.tp.viewerFallbackHint") : undefined
          }
        />
        <SendFrameForm
          disabled={!session || ended}
          sending={sending}
          onSubmit={handleSend}
          labels={{
            title: t("canvas.sendFrame"),
            kind: t("canvas.frameKind"),
            payload: t("canvas.framePayload"),
            payloadPlaceholder: t("canvas.framePayloadPlaceholder"),
            send: t("canvas.send"),
            sending: t("canvas.sending"),
            invalidJson: t("common.invalidJson"),
          }}
        />
      </section>

      <MessageInspector
        events={events}
        newestId={newestId}
        ended={ended}
        reduced={reduced}
      />
    </div>
  );
}
