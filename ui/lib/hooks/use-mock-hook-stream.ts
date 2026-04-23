"use client";

import * as React from "react";

/**
 * Hook event taxonomy mirrors Batch 1 `corlinman-hooks` crate — every variant
 * uses `#[serde(tag = "kind")]` on the Rust side, so the discriminator is
 * always in `kind`.
 *
 * B4-FE3 extended the taxonomy with approval/rate-limit/tool events now that
 * B4-BE3/BE6 publish them on the unified `HookBus`.
 *
 * TODO(B5): replace with real `/admin/hooks/stream` EventSource once the
 * Rust-side SSE endpoint is live.
 */
export type HookEventKind =
  | "message.received"
  | "message.sent"
  | "message.transcribed"
  | "message.preprocessed"
  | "session.patch"
  | "agent.bootstrap"
  | "gateway.startup"
  | "config.changed"
  | "approval.requested"
  | "approval.decided"
  | "rate_limit.triggered"
  | "tool.called";

export interface HookEvent {
  /** Stable per-event UUID-ish id for React keys. */
  id: string;
  kind: HookEventKind;
  /** Millisecond timestamp. */
  ts: number;
  /** Opaque session id; not present for gateway/config events. */
  session_key?: string;
  /** One-line human-readable summary. */
  summary: string;
  /** Raw decoded event payload — rendered in the expanded view. */
  payload: Record<string, unknown>;
}

/** Category buckets used by the Hooks Monitor filter row. */
export type HookCategory =
  | "all"
  | "message"
  | "session"
  | "agent"
  | "lifecycle"
  | "approval"
  | "rate_limit"
  | "tool"
  | "config";

/** Map a kind → its category bucket (pure, used by page + tests). */
export function kindCategory(kind: HookEventKind): HookCategory {
  if (kind.startsWith("message.")) return "message";
  if (kind === "session.patch") return "session";
  if (kind.startsWith("agent.")) return "agent";
  if (kind === "gateway.startup") return "lifecycle";
  if (kind === "approval.requested" || kind === "approval.decided")
    return "approval";
  if (kind === "rate_limit.triggered") return "rate_limit";
  if (kind === "tool.called") return "tool";
  if (kind === "config.changed") return "config";
  return "all";
}

/** All 12 supported event kinds, in canonical display order. */
export const ALL_HOOK_KINDS: HookEventKind[] = [
  "message.received",
  "message.sent",
  "message.transcribed",
  "message.preprocessed",
  "session.patch",
  "agent.bootstrap",
  "gateway.startup",
  "config.changed",
  "approval.requested",
  "approval.decided",
  "rate_limit.triggered",
  "tool.called",
];

const SESSION_POOL = [
  "qq:12345",
  "qq:67890",
  "tg:alice",
  "tg:bob",
  "cli:local",
];

const PLUGIN_POOL = ["memory", "search", "weather", "calendar", "shell"];
const TOOL_POOL = ["read_file", "run_cmd", "web_fetch", "sql_query", "notify"];
const RUNNER_POOL = ["runner-a", "runner-b", "runner-c"];
const LIMIT_TYPES = ["per_user", "per_session", "global"];
const ERROR_CODES = ["timeout", "upstream_5xx", "invalid_args", "denied"];

const SUMMARIES: Record<HookEventKind, string> = {
  "message.received": "inbound text from user",
  "message.sent": "outbound reply dispatched",
  "message.transcribed": "voice → text completed",
  "message.preprocessed": "template + context merged",
  "session.patch": "session state mutated",
  "agent.bootstrap": "agent worker online",
  "gateway.startup": "gateway bound to :8080",
  "config.changed": "config.toml reloaded",
  "approval.requested": "awaiting operator decision",
  "approval.decided": "approval resolved",
  "rate_limit.triggered": "rate limit hit, backing off",
  "tool.called": "tool invocation complete",
};

let counter = 0;
function nextId(): string {
  counter += 1;
  return `evt-${counter.toString(36)}-${Date.now().toString(36)}`;
}

function pick<T>(pool: readonly T[]): T {
  return pool[Math.floor(Math.random() * pool.length)]!;
}

/**
 * Small correlated store of pending approval ids so `approval.decided` events
 * can reference an id that actually exists in the recent stream. Exported for
 * tests.
 */
export const pendingApprovals: string[] = [];
const MAX_PENDING = 10;

function mintApprovalId(): string {
  const id = `appr-${Math.random().toString(36).slice(2, 8)}`;
  pendingApprovals.push(id);
  if (pendingApprovals.length > MAX_PENDING) pendingApprovals.shift();
  return id;
}

function consumePendingApprovalId(): string | null {
  if (pendingApprovals.length === 0) return null;
  // Pop a random pending id; models real-world out-of-order resolution.
  const idx = Math.floor(Math.random() * pendingApprovals.length);
  const [id] = pendingApprovals.splice(idx, 1);
  return id ?? null;
}

/** Exported for tests — produces a deterministic-ish sample event. */
export function createSampleEvent(
  now: number = Date.now(),
  kindOverride?: HookEventKind,
): HookEvent {
  const kind = kindOverride ?? ALL_HOOK_KINDS[Math.floor(Math.random() * ALL_HOOK_KINDS.length)]!;
  const needsSession =
    kind.startsWith("message.") ||
    kind === "session.patch" ||
    kind === "approval.requested" ||
    kind === "approval.decided" ||
    kind === "rate_limit.triggered";
  const session = needsSession
    ? pick(SESSION_POOL)
    : undefined;

  let data: Record<string, unknown> = {
    sample: true,
    nonce: Math.random().toString(36).slice(2, 10),
  };

  if (kind === "approval.requested") {
    data = {
      id: mintApprovalId(),
      session_key: session,
      plugin: pick(PLUGIN_POOL),
      tool: pick(TOOL_POOL),
      args_preview: `args:${Math.random().toString(36).slice(2, 6)}`,
      timeout_at_ms: now + 30_000,
    };
  } else if (kind === "approval.decided") {
    const existingId = consumePendingApprovalId();
    const id = existingId ?? `appr-${Math.random().toString(36).slice(2, 8)}`;
    const decisions = ["allow", "deny", "timeout"] as const;
    const decision = decisions[Math.floor(Math.random() * decisions.length)]!;
    data = {
      id,
      decision,
      decider: decision === "timeout" ? undefined : pick(["operator", "auto-rule"]),
      decided_at_ms: now,
    };
  } else if (kind === "rate_limit.triggered") {
    data = {
      session_key: session,
      limit_type: pick(LIMIT_TYPES),
      retry_after_ms: 1000 + Math.floor(Math.random() * 15_000),
    };
  } else if (kind === "tool.called") {
    const ok = Math.random() > 0.2;
    data = {
      tool: pick(TOOL_POOL),
      runner_id: pick(RUNNER_POOL),
      duration_ms: Math.floor(Math.random() * 2500),
      ok,
      ...(ok ? {} : { error_code: pick(ERROR_CODES) }),
    };
  }

  return {
    id: nextId(),
    kind,
    ts: now,
    session_key: session,
    summary: SUMMARIES[kind],
    payload: {
      kind,
      ts: new Date(now).toISOString(),
      session_key: session ?? null,
      data,
      // Promote common payload fields to top-level so renderers + tests can
      // read `payload.decision` / `payload.ok` / `payload.retry_after_ms`
      // without reaching into `data`.
      ...data,
    },
  };
}

export interface UseMockHookStreamResult {
  events: HookEvent[];
  connected: boolean;
  /** Rolling events-per-second, sampled over the last 5 seconds. */
  eps: number;
  /** Last 60 one-second EPS samples (oldest → newest). */
  epsHistory: number[];
}

const MAX_EVENTS = 200;
const EPS_WINDOW_SECONDS = 5;
const HISTORY_SIZE = 60;

/**
 * Mock event stream. Fabricates a realistic cadence of hook events with an
 * occasional flaky reconnect so the `<LiveDot>` gets to exercise both states.
 */
export function useMockHookStream(): UseMockHookStreamResult {
  const [events, setEvents] = React.useState<HookEvent[]>([]);
  const [connected, setConnected] = React.useState(true);
  const [eps, setEps] = React.useState(0);
  const [epsHistory, setEpsHistory] = React.useState<number[]>(() =>
    new Array(HISTORY_SIZE).fill(0),
  );
  // Running log of event timestamps used to derive the rolling EPS figure.
  const tsRef = React.useRef<number[]>([]);

  // Event emission loop.
  React.useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    function schedule() {
      const delay = 400 + Math.random() * 400;
      timer = setTimeout(() => {
        if (cancelled) return;
        if (Math.random() < 0.02) {
          // Flaky disconnect — recovers after ~1.2s.
          setConnected(false);
          setTimeout(() => {
            if (!cancelled) setConnected(true);
          }, 1200);
        } else {
          const evt = createSampleEvent();
          tsRef.current.push(evt.ts);
          setEvents((prev) => [evt, ...prev.slice(0, MAX_EVENTS - 1)]);
        }
        schedule();
      }, delay);
    }
    schedule();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, []);

  // EPS aggregator — recomputes once per second, keeping 60s of history.
  React.useEffect(() => {
    const interval = setInterval(() => {
      const now = Date.now();
      const cutoff = now - EPS_WINDOW_SECONDS * 1000;
      tsRef.current = tsRef.current.filter((t) => t >= cutoff);
      const current = tsRef.current.length / EPS_WINDOW_SECONDS;
      setEps(current);
      setEpsHistory((prev) => {
        const next = prev.slice(1);
        next.push(current);
        return next;
      });
    }, 1000);
    return () => clearInterval(interval);
  }, []);

  return { events, connected, eps, epsHistory };
}
