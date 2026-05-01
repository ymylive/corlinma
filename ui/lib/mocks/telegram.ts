/**
 * Empty data stubs for `/telegram` page (B3 prototype). Returns a
 * `disconnected` status so the page can render an "endpoint not
 * configured" empty state until the real bridge ships.
 *
 * TODO(B4-BE1): swap to
 * `apiFetch<TelegramStatus>("/admin/channels/telegram/status")` once
 * the gateway exposes the webhook bridge.
 */

export interface TelegramStats {
  messages_today: number;
  messages_week: number;
  latency_p50_ms: number;
  latency_p95_ms: number;
  active_chats: number;
}

export type TelegramChatKind = "dm" | "group" | "channel";

export interface TelegramMessage {
  id: string;
  from: string;
  chat_kind: TelegramChatKind;
  chat_title: string | null;
  text: string;
  ts: string;
  reply_deadline_ms?: number;
  reply_total_ms?: number;
}

export interface TelegramConfig {
  bot_token: string;
  webhook_url: string;
  secret_token: string;
  drop_pending_updates: boolean;
}

export interface TelegramStatus {
  connected: boolean;
  runtime: "connected" | "disconnected" | "unknown";
  config: TelegramConfig;
  stats: TelegramStats;
  recent_messages: TelegramMessage[];
  last_webhook_payload: Record<string, unknown> | null;
  last_dispatch_error: string | null;
}

export async function fetchTelegramMock(): Promise<TelegramStatus> {
  return {
    connected: false,
    runtime: "disconnected",
    config: {
      bot_token: "",
      webhook_url: "",
      secret_token: "",
      drop_pending_updates: false,
    },
    stats: {
      messages_today: 0,
      messages_week: 0,
      latency_p50_ms: 0,
      latency_p95_ms: 0,
      active_chats: 0,
    },
    recent_messages: [],
    last_webhook_payload: null,
    last_dispatch_error: null,
  };
}
