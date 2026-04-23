/**
 * Canvas Host HTTP client (B5-FE3).
 *
 * Wraps the upcoming B5-BE1 endpoints:
 *   POST /canvas/session        → { session }
 *   POST /canvas/frame          → { ok: true }
 *   GET  /canvas/session/{id}/events (SSE)
 *
 * When the gateway is compiled without the Canvas Host (config
 * `[canvas].host_endpoint_enabled = false`) the POST endpoints respond
 * `503 Service Unavailable`. The page then switches into fallback mode and
 * renders a mock session plus rotating mock events via the helpers in
 * `@/lib/mocks/canvas` — the UI stays demoable without the backend.
 *
 * We deliberately keep the 503-detection here so consumers receive a tagged
 * fallback response rather than having to catch-and-translate errors.
 */

import { CorlinmanApiError, apiFetch } from "@/lib/api";
import {
  type CanvasFrameKind,
  type CanvasSession,
  makeMockSession,
} from "@/lib/mocks/canvas";

export type CanvasSessionResult =
  | { kind: "live"; session: CanvasSession }
  | { kind: "fallback"; session: CanvasSession };

export type CanvasFrameResult =
  | { kind: "live"; ok: true }
  | { kind: "fallback" };

export interface CanvasFrameRequest {
  session_id: string;
  kind: CanvasFrameKind;
  payload: Record<string, unknown>;
}

function is503(err: unknown): boolean {
  return err instanceof CorlinmanApiError && err.status === 503;
}

/**
 * Create a new canvas session. Returns a tagged result so the UI can flip
 * into fallback mode on 503 without having to interpret exceptions itself.
 */
export async function createCanvasSession(): Promise<CanvasSessionResult> {
  try {
    const session = await apiFetch<CanvasSession>("/canvas/session", {
      method: "POST",
    });
    return { kind: "live", session };
  } catch (err) {
    if (!is503(err)) throw err;
    return { kind: "fallback", session: makeMockSession() };
  }
}

/**
 * Post a frame to the canvas host. Returns `{ kind: "fallback" }` on 503 so
 * the page can mirror the frame locally without showing an error toast.
 */
export async function sendCanvasFrame(
  body: CanvasFrameRequest,
): Promise<CanvasFrameResult> {
  try {
    await apiFetch<{ ok: true }>("/canvas/frame", {
      method: "POST",
      body,
    });
    return { kind: "live", ok: true };
  } catch (err) {
    if (!is503(err)) throw err;
    return { kind: "fallback" };
  }
}

/** SSE path for a session's event stream. */
export function canvasEventsPath(sessionId: string): string {
  return `/canvas/session/${encodeURIComponent(sessionId)}/events`;
}
