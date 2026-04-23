import { describe, expect, it } from "vitest";

import {
  ALL_HOOK_KINDS,
  createSampleEvent,
  pendingApprovals,
  type HookEvent,
  type HookEventKind,
} from "./use-mock-hook-stream";

describe("createSampleEvent", () => {
  it("produces all 12 kinds when every kind is explicitly requested", () => {
    // Direct exhaustive assertion — easier & more deterministic than relying
    // on Math.random to cover the full taxonomy.
    const seen = new Set<HookEventKind>();
    for (const kind of ALL_HOOK_KINDS) {
      const evt = createSampleEvent(Date.now(), kind);
      seen.add(evt.kind);
      expect(evt.kind).toBe(kind);
    }
    expect(seen.size).toBe(12);
    expect(ALL_HOOK_KINDS).toHaveLength(12);
  });

  it("emits all 12 kinds across a large random sample", () => {
    const seen = new Set<HookEventKind>();
    // 600 samples — P(missing a kind) ≈ (11/12)^600 ≈ 10^-22. Flake-proof.
    for (let i = 0; i < 600; i += 1) {
      seen.add(createSampleEvent().kind);
    }
    for (const kind of ALL_HOOK_KINDS) {
      expect(seen.has(kind)).toBe(true);
    }
  });

  it("correlates approval.decided ids with previously requested ids", () => {
    // Drain any leftover correlation state from earlier tests.
    pendingApprovals.length = 0;

    const requested: HookEvent[] = [];
    for (let i = 0; i < 5; i += 1) {
      requested.push(createSampleEvent(Date.now(), "approval.requested"));
    }
    const requestedIds = new Set(
      requested.map((e) => e.payload.id as string),
    );
    expect(requestedIds.size).toBe(5);

    // Each decision should pop an id out of the pending pool, matching one of
    // the previously requested ids.
    const decisions: HookEvent[] = [];
    for (let i = 0; i < 5; i += 1) {
      decisions.push(createSampleEvent(Date.now(), "approval.decided"));
    }
    for (const d of decisions) {
      expect(requestedIds.has(d.payload.id as string)).toBe(true);
    }
    // Pool drained after matching.
    expect(pendingApprovals).toHaveLength(0);
  });

  it("populates kind-specific payload fields", () => {
    const rl = createSampleEvent(Date.now(), "rate_limit.triggered");
    expect(typeof rl.payload.retry_after_ms).toBe("number");
    expect(typeof rl.payload.limit_type).toBe("string");

    const tool = createSampleEvent(Date.now(), "tool.called");
    expect(typeof tool.payload.ok).toBe("boolean");
    expect(typeof tool.payload.tool).toBe("string");
    expect(typeof tool.payload.runner_id).toBe("string");
  });
});
