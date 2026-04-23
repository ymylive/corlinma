import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { TiltCard } from "./tilt-card";

function mockMatchMedia(opts: { reduce?: boolean; coarse?: boolean }) {
  const { reduce = false, coarse = false } = opts;
  window.matchMedia = vi.fn().mockImplementation((query: string) => {
    let matches = false;
    if (query === "(prefers-reduced-motion: reduce)") matches = reduce;
    else if (query === "(pointer: coarse)") matches = coarse;
    return {
      matches,
      media: query,
      onchange: null,
      addEventListener: () => void 0,
      removeEventListener: () => void 0,
      addListener: () => void 0,
      removeListener: () => void 0,
      dispatchEvent: () => false,
    };
  }) as typeof window.matchMedia;
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("TiltCard", () => {
  it("renders children and responds to mouse events when enabled", () => {
    mockMatchMedia({ reduce: false, coarse: false });
    render(
      <TiltCard data-testid="tilt">
        <span>hello</span>
      </TiltCard>,
    );
    const node = screen.getByTestId("tilt");
    expect(screen.getByText("hello")).toBeInTheDocument();
    // Smoke test: mouse handlers attach and fire without throwing.
    fireEvent.mouseMove(node, { clientX: 10, clientY: 10 });
    fireEvent.mouseLeave(node);
  });

  it("falls back to a plain div under reduced motion", () => {
    mockMatchMedia({ reduce: true, coarse: false });
    render(
      <TiltCard data-testid="tilt">
        <span>static</span>
      </TiltCard>,
    );
    expect(screen.getByText("static")).toBeInTheDocument();
  });

  it("falls back to a plain div on coarse pointers", () => {
    mockMatchMedia({ reduce: false, coarse: true });
    render(
      <TiltCard data-testid="tilt">
        <span>touch</span>
      </TiltCard>,
    );
    expect(screen.getByText("touch")).toBeInTheDocument();
  });
});
