import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { LiveDot } from "./live-dot";

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

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("LiveDot", () => {
  it("renders with default ok variant and sr-only label", () => {
    mockMatchMedia(false);
    render(<LiveDot label="Live" data-testid="dot" />);
    const el = screen.getByTestId("dot");
    expect(el).toHaveAttribute("data-variant", "ok");
    expect(screen.getByText("Live")).toHaveClass("sr-only");
  });

  it("accepts explicit variants", () => {
    mockMatchMedia(false);
    render(<LiveDot variant="err" data-testid="dot" />);
    expect(screen.getByTestId("dot")).toHaveAttribute("data-variant", "err");
  });

  it("renders without crashing under reduced motion (no pulse element)", () => {
    mockMatchMedia(true);
    const { container } = render(
      <LiveDot variant="warn" pulse data-testid="dot" />,
    );
    expect(screen.getByTestId("dot")).toHaveAttribute("data-variant", "warn");
    // No animate-ping span when reduced-motion is on.
    expect(container.querySelector(".animate-ping")).toBeNull();
  });
});
