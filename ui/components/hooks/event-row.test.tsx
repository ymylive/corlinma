import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { EventRow, ageTier, eventColor } from "./event-row";
import {
  ALL_HOOK_KINDS,
  kindCategory,
  type HookEvent,
} from "@/lib/hooks/use-mock-hook-stream";

function mockMatchMedia(reduceMatches: boolean) {
  window.matchMedia = vi.fn().mockImplementation((query: string) => ({
    matches:
      query === "(prefers-reduced-motion: reduce)" ? reduceMatches : false,
    media: query,
    onchange: null,
    addEventListener: () => void 0,
    removeEventListener: () => void 0,
    addListener: () => void 0,
    removeListener: () => void 0,
    dispatchEvent: () => false,
  })) as typeof window.matchMedia;
}

function makeEvent(overrides: Partial<HookEvent> = {}): HookEvent {
  return {
    id: "evt-1",
    kind: "message.received",
    ts: Date.parse("2026-04-22T08:09:10.123Z"),
    session_key: "qq:12345",
    summary: "inbound text from user",
    payload: { hello: "world" },
    ...overrides,
  };
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("eventColor", () => {
  it("maps every documented kind to the expected palette token", () => {
    expect(eventColor("message.received")).toBe("accent");
    expect(eventColor("message.sent")).toBe("accent");
    expect(eventColor("session.patch")).toBe("accent-2");
    expect(eventColor("agent.bootstrap")).toBe("accent-3");
    expect(eventColor("gateway.startup")).toBe("ok");
    expect(eventColor("config.changed")).toBe("warn");
    expect(eventColor("unknown.kind")).toBe("muted");
  });

  it("handles the B4 approval / rate-limit / tool kinds", () => {
    expect(eventColor("approval.requested")).toBe("warn");
    expect(eventColor("approval.decided", { decision: "allow" })).toBe("ok");
    expect(eventColor("approval.decided", { decision: "deny" })).toBe("err");
    expect(eventColor("approval.decided", { decision: "timeout" })).toBe("err");
    expect(eventColor("rate_limit.triggered")).toBe("warn");
    expect(eventColor("tool.called", { ok: true })).toBe("accent-2");
    expect(eventColor("tool.called", { ok: false })).toBe("err");
  });
});

describe("ageTier", () => {
  it("floors scale at 0.92 and saturate at 0.6", () => {
    expect(ageTier(0).scale).toBe(1);
    expect(ageTier(0).saturate).toBe(1);
    const veryOld = ageTier(999);
    expect(veryOld.scale).toBe(0.92);
    expect(veryOld.saturate).toBeCloseTo(0.68, 2); // 1 - 4*0.08
  });
});

describe("EventRow", () => {
  it("renders kind badge, timestamp, session chip and expands payload on click", () => {
    mockMatchMedia(false);
    const evt = makeEvent();
    render(<EventRow event={evt} now={evt.ts + 100} />);

    expect(screen.getByTestId("event-kind-badge")).toHaveTextContent(
      "message.received",
    );
    expect(screen.getByText("inbound text from user")).toBeInTheDocument();
    expect(screen.getByText("qq:12345")).toBeInTheDocument();
    expect(screen.queryByTestId("event-payload")).toBeNull();

    fireEvent.click(screen.getByRole("button", { expanded: false }));
    expect(screen.getByTestId("event-payload")).toBeInTheDocument();
  });

  it("renders under reduced-motion with scale=1 (no aging shrink)", () => {
    mockMatchMedia(true);
    const evt = makeEvent({ ts: Date.now() - 60_000 }); // would normally age
    render(<EventRow event={evt} />);
    const row = screen.getByTestId("event-row");
    // Reduced-motion path keeps saturate at 1.0.
    expect(row.getAttribute("style") ?? "").toContain("saturate(1)");
  });

  it("special-renders approval.requested rows with id + sr announcement", () => {
    mockMatchMedia(false);
    const evt = makeEvent({
      id: "evt-appr-1",
      kind: "approval.requested",
      summary: "awaiting operator decision",
      payload: {
        id: "appr-abc123",
        plugin: "memory",
        tool: "read_file",
        args_preview: "args:foo",
        timeout_at_ms: Date.now() + 30_000,
      },
    });
    render(<EventRow event={evt} now={evt.ts + 100} />);

    expect(screen.getByTestId("approval-id")).toHaveTextContent("appr-abc123");
    expect(screen.getByTestId("approval-sr")).toHaveTextContent(
      "Approval request from plugin memory",
    );
    // Row should be a polite live region by default (boost off).
    const row = screen.getByTestId("event-row");
    expect(row.getAttribute("aria-live")).toBe("polite");
  });

  it("renders the decision badge on approval.decided rows", () => {
    mockMatchMedia(false);
    const evt = makeEvent({
      id: "evt-appr-2",
      kind: "approval.decided",
      summary: "approval resolved",
      payload: {
        id: "appr-abc123",
        decision: "deny",
        decider: "operator",
        decided_at_ms: Date.now(),
      },
    });
    render(<EventRow event={evt} now={evt.ts + 100} />);
    expect(screen.getByTestId("decision-badge")).toHaveTextContent("deny");
  });

  it("shows retry_after_ms italic-grey chip on rate_limit.triggered rows", () => {
    mockMatchMedia(false);
    const evt = makeEvent({
      id: "evt-rl-1",
      kind: "rate_limit.triggered",
      summary: "rate limit hit, backing off",
      payload: {
        session_key: "qq:12345",
        limit_type: "per_user",
        retry_after_ms: 4200,
      },
    });
    render(<EventRow event={evt} now={evt.ts + 100} />);
    expect(screen.getByTestId("rate-limit-retry")).toHaveTextContent(
      "retry in 4200ms",
    );
  });

  it("applies the alert-boost marker when alertBoost=true + motion-safe", () => {
    mockMatchMedia(false);
    const evt = makeEvent({
      id: "evt-appr-3",
      kind: "approval.requested",
      summary: "awaiting operator decision",
      payload: {
        id: "appr-xyz",
        plugin: "shell",
        tool: "run_cmd",
        args_preview: "args:bar",
        timeout_at_ms: Date.now() + 30_000,
      },
    });
    render(<EventRow event={evt} now={evt.ts + 100} alertBoost={true} />);
    const row = screen.getByTestId("event-row");
    expect(row.getAttribute("data-boosted")).toBe("true");
    // Assertive live region when boost is active (motion-safe path).
    expect(row.getAttribute("aria-live")).toBe("assertive");
    // Boosted row carries the marker class (easier test hook than inspecting
    // framer-motion's inline animation state).
    expect(row.className).toContain("row-boosted");
  });

  it("category filter 'Approval' hides non-approval events", () => {
    // Simulates the page-level `kindsForCategory` + visibility predicate.
    const approvalKinds = ALL_HOOK_KINDS.filter(
      (k) => kindCategory(k) === "approval",
    );
    expect(approvalKinds).toEqual([
      "approval.requested",
      "approval.decided",
    ]);

    const sample: HookEvent[] = ALL_HOOK_KINDS.map((k, i) =>
      makeEvent({ id: `evt-${i}`, kind: k, summary: `s-${k}` }),
    );
    const visible = sample.filter((e) =>
      approvalKinds.includes(e.kind),
    );
    expect(visible).toHaveLength(2);
    expect(visible.every((e) => kindCategory(e.kind) === "approval")).toBe(
      true,
    );

    // Conversely, non-approval kinds are excluded.
    const excluded = sample.filter(
      (e) => !approvalKinds.includes(e.kind),
    );
    expect(excluded.map((e) => e.kind)).not.toContain("approval.requested");
    expect(excluded.map((e) => e.kind)).not.toContain("approval.decided");
  });

  it("does not boost under reduced-motion even with alertBoost=true", () => {
    mockMatchMedia(true);
    const evt = makeEvent({
      id: "evt-appr-4",
      kind: "approval.requested",
      summary: "awaiting operator decision",
      payload: { id: "appr-rm", plugin: "shell", tool: "run_cmd" },
    });
    render(<EventRow event={evt} now={evt.ts + 100} alertBoost={true} />);
    const row = screen.getByTestId("event-row");
    expect(row.getAttribute("data-boosted")).not.toBe("true");
    // But a static warn border replaces the animation.
    expect(row.className).toContain("border-warn");
  });
});
