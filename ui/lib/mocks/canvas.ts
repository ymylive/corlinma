/**
 * Empty data stubs for the `/canvas` page (B5 prototype). All MOCK_*
 * constants are empty arrays / sentinels so dev-time fake events
 * never paint to a real user. Pure formatters (`formatCanvasTime`)
 * stay since they're not data, just rendering helpers.
 *
 * TODO(B5-BE1): the page can drop these imports once `/canvas/session`
 * + `/canvas/frame` + `/canvas/session/:id/events` ship on the gateway.
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
  created_at: number;
  ttl_ms: number;
  expires_at: number;
}

export interface CanvasEvent {
  id: string;
  timestamp: string;
  kind: CanvasFrameKind;
  payload: Record<string, unknown>;
}

export const MOCK_EVENTS: ReadonlyArray<
  Pick<CanvasEvent, "kind" | "payload">
> = [];

export function makeMockSession(now: number = Date.now()): CanvasSession {
  return { id: "", created_at: now, ttl_ms: 0, expires_at: now };
}

export function formatCanvasTime(epochMs: number): string {
  const d = new Date(epochMs);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  return `${hh}:${mm}:${ss}`;
}

export function buildMockEvent(
  index: number,
  now: number = Date.now(),
): CanvasEvent {
  return {
    id: `ev_${now}_${index}`,
    timestamp: formatCanvasTime(now),
    kind: "present",
    payload: {},
  };
}
