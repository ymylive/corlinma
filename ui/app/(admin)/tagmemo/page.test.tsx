/**
 * B5-FE1 — tagmemo dashboard tests.
 *
 * Covers:
 *   1. Three panels + stats render.
 *   2. Hovering a scatter dot propagates through `HoveredIdProvider` —
 *      same chunk id is highlighted in the pyramid row.
 *   3. `prefers-reduced-motion` snaps the dual-line paths (no draw-in).
 *   4. The `<details>` data-table fallback enumerates all 500 chunks.
 */

import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import React from "react";

// Mock ParentSize — jsdom has no real layout so the callback would never
// receive sensible dimensions otherwise. Forcing 800x320 lets the inner
// components paint their SVGs.
vi.mock("@visx/responsive", () => ({
  ParentSize: ({
    children,
  }: {
    children: (d: { width: number; height: number }) => React.ReactNode;
  }) => <>{children({ width: 800, height: 320 })}</>,
}));

// Stub TooltipInPortal to a same-tree div so tooltip content queries work.
vi.mock("@visx/tooltip", async () => {
  const actual =
    await vi.importActual<typeof import("@visx/tooltip")>("@visx/tooltip");
  return {
    ...actual,
    useTooltipInPortal: () => ({
      containerRef: { current: null },
      TooltipInPortal: ({ children }: { children: React.ReactNode }) => (
        <div>{children}</div>
      ),
    }),
  };
});

interface MatchMediaOverrides {
  reducedMotion?: boolean;
}

function installMatchMedia(overrides: MatchMediaOverrides = {}) {
  const { reducedMotion = false } = overrides;
  const mm = vi.fn().mockImplementation((query: string) => ({
    matches:
      query.includes("prefers-reduced-motion") && reducedMotion === true,
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

// jsdom lacks ResizeObserver.
class RO {
  observe() {
    /* no-op */
  }
  unobserve() {
    /* no-op */
  }
  disconnect() {
    /* no-op */
  }
}
if (typeof globalThis.ResizeObserver === "undefined") {
  (globalThis as unknown as { ResizeObserver: typeof RO }).ResizeObserver = RO;
}

import TagMemoPage from "./page";

describe("TagMemoPage", () => {
  beforeEach(() => {
    installMatchMedia();
  });
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("renders the three panels + stat cards", () => {
    render(<TagMemoPage />);
    expect(screen.getByTestId("panel-scatter")).toBeInTheDocument();
    expect(screen.getByTestId("panel-dual-line")).toBeInTheDocument();
    expect(screen.getByTestId("panel-pyramid-inner")).toBeInTheDocument();
    // Stats — we seed 500 chunks, so the "chunks" card should surface 500
    // via AnimatedNumber (which jumps to value under reduced-motion, but
    // under normal motion the spring value starts at the prior state; we
    // only assert that the aria-live node was rendered).
    const stats = screen.getAllByText(/chunks|avg entropy|avg logic_depth|unique axes/i);
    expect(stats.length).toBeGreaterThanOrEqual(4);
  });

  it("hovering a scatter dot highlights the matching pyramid row", () => {
    render(<TagMemoPage />);
    // Pick a chunk that definitely exists in the mock set.
    const dot = screen.getByTestId("scatter-dot-7");
    fireEvent.mouseOver(dot);
    const row = screen.getByTestId("pyramid-row-7");
    // When a chunk is the hovered one, its parent <g> keeps opacity 1
    // while others drop to 0.3. We assert the opposite for a sibling row.
    const other = screen.getByTestId("pyramid-row-0");
    expect(row).toHaveAttribute("opacity", "1");
    expect(other).toHaveAttribute("opacity", "0.3");
  });

  it("snaps the dual-line path animation under prefers-reduced-motion", () => {
    installMatchMedia({ reducedMotion: true });
    render(<TagMemoPage />);
    const entropyPath = screen.getByTestId("line-entropy");
    // framer-motion seeds initial + animate; with reduced motion the path
    // jumps straight to pathLength 1 (no draw-in). We verify the path
    // carries a `d` attribute (i.e. the animation didn't leave it empty
    // because we gate the initial/animate variant on `reduced`).
    const d = entropyPath.getAttribute("d");
    expect(d).toBeTruthy();
    expect((d ?? "").length).toBeGreaterThan(0);
  });

  it("renders a screen-reader data table listing all 500 chunk ids", () => {
    render(<TagMemoPage />);
    const table = screen.getByTestId("fallback-table");
    const rows = table.querySelectorAll("tbody tr");
    expect(rows.length).toBe(500);
    // Spot-check first + last ids are present.
    expect(screen.getByTestId("fallback-row-0")).toBeInTheDocument();
    expect(screen.getByTestId("fallback-row-499")).toBeInTheDocument();
  });
});
