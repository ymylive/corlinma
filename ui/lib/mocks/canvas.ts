/**
 * Mock data for B5-FE3 Canvas viewer stub.
 *
 * Shape mirrors what the upcoming Canvas Host HTTP endpoints (B5-BE1) will
 * return. The viewer page (`app/(admin)/canvas/page.tsx`) queries this stub
 * whenever the real endpoint returns 503 (config flag `host_endpoint_enabled`
 * is false) or when the endpoint isn't reachable at all.
 *
 * TODO(B5-BE1): swap to live client once the gateway exposes
 *   POST /canvas/session
 *   POST /canvas/frame
 *   GET  /canvas/session/{id}/events  (SSE)
 */

export type CanvasFrameKind =
  | "present"
  | "hide"
  | "navigate"
  | "eval"
  | "snapshot"
  | "a2ui_push"
  | "a2ui_reset";

export const CANVAS_FRAME_KINDS: readonly CanvasFrameKind[] = [
  "present",
  "hide",
  "navigate",
  "eval",
  "snapshot",
  "a2ui_push",
  "a2ui_reset",
] as const;

export interface CanvasSession {
  id: string;
  /** Epoch ms when the session was created. */
  created_at: number;
  /** Total TTL in ms. */
  ttl_ms: number;
  /** Epoch ms when the session is due to expire. */
  expires_at: number;
}

export interface CanvasEvent {
  /** Client-side id used as a react key. */
  id: string;
  /** HH:mm:ss local time for the row. */
  timestamp: string;
  kind: CanvasFrameKind;
  payload: Record<string, unknown>;
}

/** Sample events used by the rotating fallback mode. */
export const MOCK_EVENTS: ReadonlyArray<
  Pick<CanvasEvent, "kind" | "payload">
> = [
  {
    kind: "a2ui_push",
    payload: {
      component: "HeroCard",
      props: { title: "Hello from a2ui", subtitle: "rust → python → ui" },
    },
  },
  {
    kind: "navigate",
    payload: { url: "https://corlinman.example/canvas/demo" },
  },
  {
    kind: "present",
    payload: { width: 1024, height: 720, theme: "dark" },
  },
  {
    kind: "eval",
    payload: { expr: "window.canvas.version", result: "0.1.0" },
  },
  {
    kind: "snapshot",
    payload: { bytes: 48_912, format: "png" },
  },
  {
    kind: "hide",
    payload: { reason: "user_dismissed" },
  },
] as const;

/** Build an in-memory canvas session with a stable id. */
export function makeMockSession(now: number = Date.now()): CanvasSession {
  const id = `cs_${Math.random().toString(16).slice(2, 10)}`;
  const ttl = 5 * 60_000; // 5 minutes
  return {
    id,
    created_at: now,
    ttl_ms: ttl,
    expires_at: now + ttl,
  };
}

/** Format epoch ms as HH:mm:ss using the current locale (24h). */
export function formatCanvasTime(epochMs: number): string {
  const d = new Date(epochMs);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  return `${hh}:${mm}:${ss}`;
}

/** Turn a raw mock entry into a full CanvasEvent carrying a fresh id + ts. */
export function buildMockEvent(
  index: number,
  now: number = Date.now(),
): CanvasEvent {
  const base = MOCK_EVENTS[index % MOCK_EVENTS.length]!;
  return {
    id: `ev_${now}_${index}`,
    timestamp: formatCanvasTime(now),
    kind: base.kind,
    payload: base.payload,
  };
}
