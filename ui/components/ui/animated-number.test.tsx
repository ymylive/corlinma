import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { AnimatedNumber } from "./animated-number";

function mockMatchMedia(reduceMatches: boolean) {
  const listeners: Array<(e: MediaQueryListEvent) => void> = [];
  window.matchMedia = vi.fn().mockImplementation((query: string) => ({
    matches:
      query === "(prefers-reduced-motion: reduce)" ? reduceMatches : false,
    media: query,
    onchange: null,
    addEventListener: (_: string, cb: (e: MediaQueryListEvent) => void) => {
      listeners.push(cb);
    },
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

describe("AnimatedNumber", () => {
  it("renders a formatted number with aria-live", () => {
    mockMatchMedia(false);
    render(<AnimatedNumber value={1234} format="number" />);
    const el = screen.getByText(/1,?234/);
    expect(el).toBeInTheDocument();
    expect(el).toHaveAttribute("aria-live", "polite");
    expect(el).toHaveAttribute("aria-atomic", "true");
  });

  it("renders currency format", () => {
    mockMatchMedia(false);
    render(<AnimatedNumber value={9.5} format="currency" />);
    // Intl currency output for USD includes "$".
    expect(screen.getByText(/\$/)).toBeInTheDocument();
  });

  it("honors reduced motion by rendering target immediately", () => {
    mockMatchMedia(true);
    render(<AnimatedNumber value={42} format="number" />);
    expect(screen.getByText("42")).toBeInTheDocument();
  });
});
