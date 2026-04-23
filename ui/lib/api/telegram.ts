/**
 * Telegram channel admin API client (B4-FE1).
 *
 * Tries the real gateway at `/admin/channels/telegram/*` and, when the
 * endpoint responds 404 (gateway not yet upgraded by B4-BE1 ops surface),
 * falls back to the B3-FE2 mock from `@/lib/mocks/telegram`. A one-shot
 * `console.info` fires on the first fallback inside a process so dev tools
 * stay readable.
 *
 * The `TelegramMessage` shape intentionally diverges from the mock's
 * (`chat_kind`/`text`/`ts`): it mirrors what the backend
 * (`rust/crates/corlinman-channels/src/telegram/*.rs`) will return once the
 * admin surface ships — `kind`, `routing`, `mention_reason`, optional media
 * descriptor. The status adapter in this module bridges the two so the page
 * can render the same list either way.
 *
 * TODO(B4-BE1 ops): endpoints flagged below.
 */

import {
  CorlinmanApiError,
  apiFetch,
} from "@/lib/api";
import {
  fetchTelegramMock,
  type TelegramConfig as MockTelegramConfig,
  type TelegramStats as MockTelegramStats,
  type TelegramMessage as MockTelegramMessage,
} from "@/lib/mocks/telegram";

/* ------------------------------------------------------------------ */
/*                           Public types                             */
/* ------------------------------------------------------------------ */

export type TelegramConfig = MockTelegramConfig;
export type TelegramStats = MockTelegramStats;

export interface TelegramMedia {
  kind: "photo" | "voice" | "document";
  /** Path under the gateway media cache. UI renders a thumbnail / preview. */
  local_path: string;
  mime?: string;
  size_bytes?: number;
  /** Voice clips: playback duration in seconds (used for the waveform chip). */
  duration_sec?: number;
  /** Documents: original filename (used in the row label). */
  filename?: string;
}

export interface TelegramMessage {
  id: string;
  kind: "private" | "group";
  chat_id: string;
  chat_title?: string;
  from_username?: string;
  content?: string;
  media?: TelegramMedia;
  /** Epoch ms when the message landed. */
  timestamp_ms: number;
  /** Countdown to reply SLA, if a reply is still in flight. */
  reply_deadline_ms?: number;
  reply_total_ms?: number;
  routing: "responded" | "ignored" | "queued";
  mention_reason?: "dm" | "mention" | "reply_to_bot" | "none";
}

export interface TelegramStatusResponse {
  config: TelegramConfig;
  stats: TelegramStats;
  connected: boolean;
  runtime?: "connected" | "disconnected" | "unknown";
  last_error?: string | null;
  last_webhook_payload?: Record<string, unknown> | null;
}

export interface TelegramSendRequest {
  chat_id: string;
  text: string;
}

export interface TelegramSendResponse {
  status: "ok" | "error";
  message_id?: number;
  error?: string;
}

/** Tag attached to fallback responses so consumers (and tests) can tell the
 *  page is running against the mock without inspecting console output. */
export const TELEGRAM_MOCK_SOURCE = "mock" as const;
export const TELEGRAM_LIVE_SOURCE = "live" as const;

/* ------------------------------------------------------------------ */
/*                        Fallback bookkeeping                        */
/* ------------------------------------------------------------------ */

let fallbackLogged = false;

/** Reset the once-per-process fallback log flag — tests use this to reassert
 *  the console.info fired on the first 404. */
export function __resetTelegramFallbackLog(): void {
  fallbackLogged = false;
}

function logFallbackOnce(): void {
  if (fallbackLogged) return;
  fallbackLogged = true;
  // eslint-disable-next-line no-console
  console.info("[telegram] admin endpoint not available; using mock");
}

function is404(err: unknown): boolean {
  return err instanceof CorlinmanApiError && err.status === 404;
}

/* ------------------------------------------------------------------ */
/*                         Mock <-> live bridge                       */
/* ------------------------------------------------------------------ */

/**
 * Translate a B3-FE2 mock message (still the shape shipped in
 * `ui/lib/mocks/telegram.ts`) into the B4-BE1 `TelegramMessage` contract.
 * This keeps the fallback path exercising the same component code.
 */
function adaptMockMessage(m: MockTelegramMessage): TelegramMessage {
  const isGroup = m.chat_kind === "group" || m.chat_kind === "channel";
  return {
    id: m.id,
    kind: isGroup ? "group" : "private",
    chat_id: m.chat_title ?? m.from,
    chat_title: m.chat_title ?? undefined,
    from_username: m.from,
    content: m.text,
    timestamp_ms: Date.now(),
    reply_deadline_ms: m.reply_deadline_ms,
    reply_total_ms: m.reply_total_ms,
    routing: m.reply_deadline_ms ? "queued" : isGroup ? "ignored" : "responded",
    mention_reason: isGroup ? "none" : "dm",
  };
}

/* ------------------------------------------------------------------ */
/*                            Public fetches                          */
/* ------------------------------------------------------------------ */

/**
 * Fetches gateway status + config for the Telegram channel.
 *
 * TODO(B4-BE1 ops): `GET /admin/channels/telegram/status`.
 */
export async function fetchTelegramStatus(): Promise<TelegramStatusResponse> {
  try {
    return await apiFetch<TelegramStatusResponse>(
      "/admin/channels/telegram/status",
    );
  } catch (err) {
    if (!is404(err)) throw err;
    logFallbackOnce();
    const mock = await fetchTelegramMock();
    return {
      config: mock.config,
      stats: mock.stats,
      connected: mock.connected,
      runtime: mock.runtime,
      last_error: mock.last_dispatch_error ?? null,
      last_webhook_payload: mock.last_webhook_payload,
    };
  }
}

/**
 * Fetches the recent-messages list.
 *
 * TODO(B4-BE1 ops): `GET /admin/channels/telegram/messages?limit=<n>`.
 */
export async function fetchTelegramMessages(opts?: {
  limit?: number;
}): Promise<TelegramMessage[]> {
  const limit = opts?.limit ?? 20;
  const qs = new URLSearchParams({ limit: String(limit) }).toString();
  try {
    return await apiFetch<TelegramMessage[]>(
      `/admin/channels/telegram/messages?${qs}`,
    );
  } catch (err) {
    if (!is404(err)) throw err;
    logFallbackOnce();
    const mock = await fetchTelegramMock();
    return mock.recent_messages.slice(0, limit).map(adaptMockMessage);
  }
}

/**
 * Sends a test message via the gateway. Returns `{ status: "not_deployed" }`
 * when the endpoint 404s, so the caller can toast a friendly "backend
 * pending" message rather than an error.
 *
 * TODO(B4-BE1 ops): `POST /admin/channels/telegram/send`.
 */
export async function sendTelegramTestMessage(
  body: TelegramSendRequest,
): Promise<TelegramSendResponse | { status: "not_deployed" }> {
  try {
    return await apiFetch<TelegramSendResponse>(
      "/admin/channels/telegram/send",
      { method: "POST", body },
    );
  } catch (err) {
    if (is404(err)) return { status: "not_deployed" };
    throw err;
  }
}
