"use client";

import * as React from "react";

/**
 * Hook event taxonomy mirrors the `corlinman-hooks` crate — every
 * variant uses `#[serde(tag = "kind")]` on the Rust side, so the
 * discriminator is always in `kind`.
 *
 * The stream below is an empty placeholder: it returns no events and
 * stays "disconnected" until `/admin/hooks/stream` (real EventSource)
 * lands. Pure helpers (`kindCategory`, `ALL_HOOK_KINDS`, `pendingApprovals`,
 * `createSampleEvent`) stay so existing importers + their tests still
 * compile.
 *
 * TODO(B5): swap `useMockHookStream` for an EventSource against
 * `/admin/hooks/stream` once the Rust SSE endpoint ships.
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
  id: string;
  kind: HookEventKind;
  ts: number;
  session_key?: string;
  summary: string;
  payload: Record<string, unknown>;
}

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

const SUMMARIES: Record<HookEventKind, string> = {
  "message.received": "inbound text from user",
  "message.sent": "outbound reply dispatched",
  "message.transcribed": "voice → text completed",
  "message.preprocessed": "template + context merged",
  "session.patch": "session state mutated",
  "agent.bootstrap": "agent worker online",
  "gateway.startup": "gateway bound",
  "config.changed": "config.toml reloaded",
  "approval.requested": "awaiting operator decision",
  "approval.decided": "approval resolved",
  "rate_limit.triggered": "rate limit hit, backing off",
  "tool.called": "tool invocation complete",
};

let counter = 0;
function nextId(): string {
  counter += 1;
  return `evt-${counter.toString(36)}`;
}

/**
 * Used to be a correlated pending-approval store driven by random
 * fake events. Empty by default now; existing tests depend on the
 * mutable export so we keep the binding in place.
 */
export const pendingApprovals: string[] = [];

/**
 * Deterministic placeholder. Returns an event of the requested kind
 * with empty payload. Tests that exercised the old random shape now
 * see the canonical empty form — they assert structure, not random
 * field values.
 */
export function createSampleEvent(
  now: number = Date.now(),
  kindOverride?: HookEventKind,
): HookEvent {
  const kind = kindOverride ?? "gateway.startup";
  return {
    id: nextId(),
    kind,
    ts: now,
    session_key: undefined,
    summary: SUMMARIES[kind],
    payload: {
      kind,
      ts: new Date(now).toISOString(),
      session_key: null,
      data: {},
    },
  };
}

export interface UseMockHookStreamResult {
  events: HookEvent[];
  connected: boolean;
  eps: number;
  epsHistory: number[];
}

const HISTORY_SIZE = 60;

/**
 * Empty stream stub. Returns zero events and reports `connected: false`
 * so the page renders an "endpoint not yet wired" empty state. The
 * shape matches the eventual `useHookStream()` once the real SSE
 * endpoint ships.
 */
export function useMockHookStream(): UseMockHookStreamResult {
  const [epsHistory] = React.useState<number[]>(() =>
    new Array(HISTORY_SIZE).fill(0),
  );
  return { events: [], connected: false, eps: 0, epsHistory };
}
