/**
 * Mock data for B3-FE2 Telegram Channel page.
 *
 * Shape approximates the upcoming `GET /admin/channels/telegram/status` payload
 * (B4-BE1). The page queries this stub via `fetchTelegramMock()` until the
 * real webhook bridge lands.
 *
 * TODO(B4-BE1): swap to `apiFetch<TelegramStatus>("/admin/channels/telegram/status")`.
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
  from: string; // "@alice"
  chat_kind: TelegramChatKind;
  chat_title: string | null; // "dev-chat" | null for DMs
  text: string;
  ts: string; // "14:32"
  /** If present, a reply is in-flight and expected before this deadline. */
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

const MOCK_RECENT: TelegramMessage[] = [
  {
    id: "m-1",
    from: "@alice",
    chat_kind: "group",
    chat_title: "dev-chat",
    text: "help me with the k8s manifest — the sidecar keeps OOMing",
    ts: "14:32",
    reply_deadline_ms: 12_000,
    reply_total_ms: 15_000,
  },
  {
    id: "m-2",
    from: "@bob",
    chat_kind: "dm",
    chat_title: null,
    text: "can you summarise the last stand-up?",
    ts: "14:28",
    reply_deadline_ms: 4_500,
    reply_total_ms: 15_000,
  },
  {
    id: "m-3",
    from: "@carol",
    chat_kind: "group",
    chat_title: "design-review",
    text: "thoughts on the new palette?",
    ts: "14:21",
    reply_deadline_ms: 9_000,
    reply_total_ms: 15_000,
  },
  {
    id: "m-4",
    from: "@dave",
    chat_kind: "group",
    chat_title: "dev-chat",
    text: "merged — thanks",
    ts: "14:14",
  },
  {
    id: "m-5",
    from: "@eve",
    chat_kind: "dm",
    chat_title: null,
    text: "👍",
    ts: "14:02",
  },
  {
    id: "m-6",
    from: "@frank",
    chat_kind: "group",
    chat_title: "ops",
    text: "gateway p95 spiked to 320ms for ~2 minutes around 13:55",
    ts: "13:58",
  },
  {
    id: "m-7",
    from: "@grace",
    chat_kind: "group",
    chat_title: "dev-chat",
    text: "rebasing onto main; will push after tests",
    ts: "13:51",
  },
  {
    id: "m-8",
    from: "@heidi",
    chat_kind: "dm",
    chat_title: null,
    text: "re: your draft — two nits inline, otherwise LGTM",
    ts: "13:47",
  },
  {
    id: "m-9",
    from: "@ivan",
    chat_kind: "group",
    chat_title: "random",
    text: "anyone tried the new Claude release yet?",
    ts: "13:40",
  },
  {
    id: "m-10",
    from: "@judy",
    chat_kind: "group",
    chat_title: "ops",
    text: "alert resolved — disk pressure, rotated logs",
    ts: "13:35",
  },
];

const MOCK_STATUS: TelegramStatus = {
  connected: true,
  runtime: "connected",
  config: {
    bot_token: "7834561230:AAEhBP9aFxZqLk3n2mQrStUvWx0YzAbCdEf",
    webhook_url: "https://corlinman.example.com/tg/webhook",
    secret_token: "s3cret-webhook-token-abcdef123456",
    drop_pending_updates: true,
  },
  stats: {
    messages_today: 248,
    messages_week: 1867,
    latency_p50_ms: 142,
    latency_p95_ms: 389,
    active_chats: 12,
  },
  recent_messages: MOCK_RECENT,
  last_webhook_payload: {
    update_id: 987_654_321,
    message: {
      message_id: 42,
      from: { id: 11111, username: "alice", first_name: "Alice" },
      chat: { id: -100123, title: "dev-chat", type: "supergroup" },
      date: 1_713_790_320,
      text: "help me with the k8s manifest — the sidecar keeps OOMing",
    },
  },
  last_dispatch_error: null,
};

/**
 * Stub fetcher — swap to `apiFetch<TelegramStatus>("/admin/channels/telegram/status")`
 * once B4-BE1 ships. Simulates a tiny roundtrip so skeleton states are visible.
 *
 * TODO(B4-BE1): replace with the real endpoint.
 */
export async function fetchTelegramMock(): Promise<TelegramStatus> {
  await new Promise((r) => setTimeout(r, 80));
  return MOCK_STATUS;
}
