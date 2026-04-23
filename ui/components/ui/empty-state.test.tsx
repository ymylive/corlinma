import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { EmptyState } from "./empty-state";

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

describe("EmptyState", () => {
  it("renders title, description, icon and action", () => {
    mockMatchMedia(false);
    render(
      <EmptyState
        icon={<svg data-testid="icon" />}
        title="No plugins loaded"
        description="Add a plugin in config.toml."
        action={<button type="button">Learn more</button>}
      />,
    );
    expect(screen.getByText("No plugins loaded")).toBeInTheDocument();
    expect(screen.getByText("Add a plugin in config.toml.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Learn more" })).toBeInTheDocument();
    expect(screen.getByTestId("icon")).toBeInTheDocument();
    expect(screen.getByRole("status")).toBeInTheDocument();
  });

  it("renders under reduced motion without crashing", () => {
    mockMatchMedia(true);
    render(<EmptyState title="Nothing here" />);
    expect(screen.getByText("Nothing here")).toBeInTheDocument();
  });
});
