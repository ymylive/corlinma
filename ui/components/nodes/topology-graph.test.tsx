/**
 * TopologyGraph tests (B4-FE2).
 *
 *   1. All 18 mock runners render as SVG groups.
 *   2. Click on a runner toggles selection via `data-selected`.
 *   3. Under `prefers-reduced-motion`, no runner carries the halo/shake
 *      animation classes — topology stays static.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import * as React from "react";

import { TopologyGraph } from "./topology-graph";
import { fetchRunnersMock, type Runner } from "@/lib/mocks/nodes";

function installMatchMedia(reducedMotion: boolean) {
  const mm = vi.fn().mockImplementation((query: string) => ({
    matches:
      query === "(prefers-reduced-motion: reduce)" ? reducedMotion : false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  }));
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    configurable: true,
    value: mm,
  });
}

function Harness({ runners }: { runners: Runner[] }) {
  const [selectedId, setSelectedId] = React.useState<string | null>(null);
  return (
    <TopologyGraph
      runners={runners}
      selectedId={selectedId}
      onSelect={(r) => setSelectedId((prev) => (r && prev !== r.id ? r.id : null))}
    />
  );
}

describe("TopologyGraph", () => {
  let RUNNERS: Runner[] = [];

  beforeEach(async () => {
    installMatchMedia(false);
    RUNNERS = await fetchRunnersMock();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("renders one node per runner (18 total)", () => {
    render(<Harness runners={RUNNERS} />);
    const nodes = screen.getAllByRole("button", { name: /^Runner / });
    expect(nodes).toHaveLength(18);
  });

  it("toggles data-selected when a runner is clicked", () => {
    render(<Harness runners={RUNNERS} />);
    const first = RUNNERS[0]!;
    const node = screen.getByTestId(`runner-node-${first.id}`);
    expect(node).toHaveAttribute("data-selected", "false");

    fireEvent.click(node);
    expect(node).toHaveAttribute("data-selected", "true");

    fireEvent.click(node);
    expect(node).toHaveAttribute("data-selected", "false");
  });

  it("under prefers-reduced-motion, no runner carries the animation classes", () => {
    cleanup();
    installMatchMedia(true);
    render(<Harness runners={RUNNERS} />);
    // Wait one tick for useEffect to pick up the media query — rendered nodes
    // are queried synchronously after render, but the halo/shake <circle>s
    // are only emitted when `reduced === false`, so we assert their absence
    // directly.
    const halos = document.querySelectorAll(".nodes-halo");
    const shakes = document.querySelectorAll(".nodes-shake");
    // When the media-query resolves, the reduced state flips and the motion
    // classes drop out. React's effect runs during the render flush, so the
    // final DOM should contain none of them.
    expect(halos.length + shakes.length).toBe(0);
  });
});
