"use client";

import * as React from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, Plus } from "lucide-react";

import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { LiveDot } from "@/components/ui/live-dot";
import { CountdownRing } from "@/components/ui/countdown-ring";
import { useMotion } from "@/components/ui/motion-safe";
import {
  createCanvasSession,
  sendCanvasFrame,
  canvasEventsPath,
} from "@/lib/api/canvas";
import { openEventStream } from "@/lib/sse";
import {
  type CanvasEvent,
  type CanvasFrameKind,
  type CanvasSession,
  buildMockEvent,
  formatCanvasTime,
} from "@/lib/mocks/canvas";

import { ProtocolInspector } from "./ProtocolInspector";
import { SendFrameForm } from "./SendFrameForm";

/**
 * B5-FE3 Canvas viewer stub.
 *
 * Demonstrates the Canvas Host protocol without a real renderer:
 *   - sandboxed iframe surface with a skeleton shimmer overlay
 *   - live SSE protocol-inspector at the bottom
 *   - session controls (new / send frame) that transparently fall back to a
 *     rotating mock stream when the gateway responds 503 (canvas host
 *     disabled in config).
 */
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

  // Track the last-known expiry so the countdown stays correct after mount.
  React.useEffect(() => {
    const i = window.setInterval(() => setNowTick(Date.now()), 1_000);
    return () => window.clearInterval(i);
  }, []);

  const pushEvent = React.useCallback((ev: CanvasEvent) => {
    setEvents((prev) => [ev, ...prev].slice(0, 200));
    setNewestId(ev.id);
  }, []);

  // ---------------------------------------------------------------------
  // Session lifecycle
  // ---------------------------------------------------------------------

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

  // Create a session on first mount so the page has something to show.
  React.useEffect(() => {
    void newSession();
    // `newSession` is stable (useCallback with empty deps)
  }, [newSession]);

  // ---------------------------------------------------------------------
  // SSE / fallback rotation
  // ---------------------------------------------------------------------

  React.useEffect(() => {
    if (!session) return;

    if (fallback) {
      // Rotate a mock event every 2s to keep the UI alive when the canvas
      // host endpoint is disabled.
      let idx = 0;
      pushEvent(buildMockEvent(idx));
      idx += 1;
      const timer = window.setInterval(() => {
        pushEvent(buildMockEvent(idx));
        idx += 1;
      }, 2_000);
      return () => window.clearInterval(timer);
    }

    // Live SSE path.
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

  // ---------------------------------------------------------------------
  // Send frame
  // ---------------------------------------------------------------------

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

      // Whether live or fallback, mirror the frame into the inspector
      // immediately so the user sees their action land. (For a live session
      // the SSE stream will also echo it — duplicates are acceptable in the
      // stub viewer.)
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

  // ---------------------------------------------------------------------
  // Rendering
  // ---------------------------------------------------------------------

  const placeholderHtml = React.useMemo(
    () => buildPlaceholderHtml(session?.id ?? "(no session)"),
    [session?.id],
  );

  const remainingMs = session ? Math.max(0, session.expires_at - nowTick) : 0;
  void nowTick; // used by the countdown label refresh

  const runtimeChip = (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 font-mono text-[10px] uppercase tracking-wider",
        ended
          ? "border-muted-foreground/30 bg-muted-foreground/5 text-muted-foreground"
          : fallback
            ? "border-warn/40 bg-warn/10 text-warn"
            : "border-ok/40 bg-ok/10 text-ok",
      )}
    >
      <LiveDot
        variant={ended ? "muted" : fallback ? "warn" : "ok"}
        pulse={!ended}
      />
      {ended
        ? t("canvas.chipEnded")
        : fallback
          ? t("canvas.chipFallback")
          : t("canvas.chipLive")}
    </span>
  );

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold tracking-tight text-foreground">
            {t("canvas.title")}
          </h1>
          <p className="text-xs text-muted-foreground">
            {t("canvas.subtitle")}
          </p>
        </div>
        <div className="flex items-center gap-3">
          {session ? (
            <span
              className="font-mono text-xs text-muted-foreground"
              data-testid="canvas-session-id"
            >
              {session.id}
            </span>
          ) : null}
          {session ? (
            <CountdownRing
              remainingMs={remainingMs}
              totalMs={session.ttl_ms}
              label={t("canvas.ttlLabel")}
            />
          ) : null}
          {runtimeChip}
          <Button
            size="sm"
            onClick={newSession}
            disabled={creating}
            data-testid="canvas-new-session"
          >
            <Plus className="h-3.5 w-3.5" />
            {creating ? t("common.loading") : t("canvas.newSession")}
          </Button>
        </div>
      </header>

      {fallback ? (
        <div
          role="status"
          data-testid="canvas-fallback-banner"
          className="flex items-start gap-2 rounded-md border border-warn/40 bg-warn/5 px-3 py-2 text-xs text-warn"
        >
          <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden />
          <span>{t("canvas.fallbackBanner")}</span>
        </div>
      ) : null}

      <div className="grid gap-4 lg:grid-cols-[1fr_320px]">
        <div className="flex flex-col overflow-hidden rounded-lg border border-border bg-panel">
          <CanvasSurface
            srcDoc={placeholderHtml}
            reduced={reduced}
            sessionId={session?.id}
          />
        </div>
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
      </div>

      <ProtocolInspector
        events={events}
        newestId={newestId}
        ended={ended}
        labels={{
          title: t("canvas.inspectorTitle"),
          expand: t("canvas.inspectorExpanded"),
          collapse: t("canvas.inspectorCollapsed"),
          empty: t("canvas.inspectorEmpty"),
          sessionEnded: t("canvas.sessionEnded"),
        }}
      />
    </div>
  );
}

/* ---------------------------------------------------------------------- */
/*                        Sandboxed iframe surface                        */
/* ---------------------------------------------------------------------- */

interface CanvasSurfaceProps {
  srcDoc: string;
  reduced: boolean;
  sessionId: string | undefined;
}

const SKELETON_MS = 800;

function CanvasSurface({ srcDoc, reduced, sessionId }: CanvasSurfaceProps) {
  const [showSkeleton, setShowSkeleton] = React.useState(true);

  React.useEffect(() => {
    if (reduced) {
      setShowSkeleton(false);
      return;
    }
    setShowSkeleton(true);
    const t = window.setTimeout(() => setShowSkeleton(false), SKELETON_MS);
    return () => window.clearTimeout(t);
  }, [reduced, sessionId]);

  return (
    <div className="relative" style={{ height: 420 }}>
      <iframe
        data-testid="canvas-iframe"
        title="Canvas surface placeholder"
        sandbox="allow-same-origin"
        srcDoc={srcDoc}
        className="h-full w-full border-0"
      />
      {showSkeleton ? (
        <div
          aria-hidden
          data-testid="canvas-skeleton"
          className={cn(
            "absolute inset-0",
            reduced ? "bg-muted/60" : "shimmer",
          )}
        />
      ) : null}
    </div>
  );
}

/**
 * Build the inert HTML document used as the iframe's `srcDoc`. Kept out of
 * the render path so new renders don't re-serialise the template.
 */
function buildPlaceholderHtml(sessionId: string): string {
  const safeId = String(sessionId).replace(/[<>&"]/g, "");
  return `<!doctype html>
<html>
<head>
<meta charset="utf-8">
<title>Canvas placeholder</title>
<style>
  :root { color-scheme: light dark; }
  html, body { margin: 0; height: 100%; }
  body {
    display: flex; align-items: center; justify-content: center;
    font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif;
    background:
      radial-gradient(1200px 400px at 20% 0%, rgba(99,102,241,0.15), transparent 60%),
      radial-gradient(800px 400px at 80% 100%, rgba(20,184,166,0.15), transparent 60%),
      #0f1115;
    color: #e5e7eb;
  }
  .card {
    text-align: center;
    letter-spacing: 0.02em;
  }
  .title { font-size: 14px; color: #9ca3af; text-transform: uppercase; letter-spacing: 0.16em; }
  .session { margin-top: 12px; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 18px; color: #e5e7eb; }
  @media (prefers-color-scheme: light) {
    body { background:
      radial-gradient(1200px 400px at 20% 0%, rgba(99,102,241,0.18), transparent 60%),
      radial-gradient(800px 400px at 80% 100%, rgba(20,184,166,0.18), transparent 60%),
      #ffffff;
      color: #111827; }
    .title { color: #6b7280; }
    .session { color: #111827; }
  }
</style>
</head>
<body>
<div class="card">
  <div class="title">Canvas placeholder</div>
  <div class="session">Session: ${safeId}</div>
</div>
</body>
</html>`;
}
