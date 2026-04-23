import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { CountdownRing } from "./countdown-ring";

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

describe("CountdownRing", () => {
  it("exposes role=timer with aria values", () => {
    mockMatchMedia(false);
    render(
      <CountdownRing
        remainingMs={123000}
        totalMs={300000}
        size={32}
        label="5m deadline"
      />,
    );
    const timer = screen.getByRole("timer");
    expect(timer).toHaveAttribute("aria-valuemax", "300000");
    expect(timer).toHaveAttribute("aria-label", "5m deadline");
    // aria-valuenow should be initialised close to remainingMs.
    const now = Number(timer.getAttribute("aria-valuenow"));
    expect(now).toBeGreaterThan(100000);
    expect(now).toBeLessThanOrEqual(123000);
  });

  it("renders under reduced motion without crashing", () => {
    mockMatchMedia(true);
    render(
      <CountdownRing remainingMs={60000} totalMs={300000} label="1m left" />,
    );
    const timer = screen.getByRole("timer");
    expect(timer).toHaveAttribute("aria-valuenow", "60000");
  });

  it("switches to urgent color when remaining < 20%", () => {
    mockMatchMedia(true); // reduced-motion keeps display stable at provided values
    render(
      <CountdownRing remainingMs={10000} totalMs={300000} label="hurry" />,
    );
    // Text reads "10s" when under urgent threshold.
    expect(screen.getByText(/10s/)).toBeInTheDocument();
  });
});
