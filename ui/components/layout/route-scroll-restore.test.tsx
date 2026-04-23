/**
 * RouteScrollRestore tests.
 *
 * Contract we rely on for Batches 2-5:
 *   - On pathname change, scroll to top with `behavior: "instant"` so
 *     shared-layout morphs don't start from the previous page's scroll
 *     position.
 *   - When the URL carries a hash, don't stomp on the anchor scroll the
 *     browser just did.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render } from "@testing-library/react";

let currentPath = "/first";
vi.mock("next/navigation", () => ({
  usePathname: () => currentPath,
}));

import { RouteScrollRestore } from "./route-scroll-restore";

describe("RouteScrollRestore", () => {
  let scrollToMock: ReturnType<typeof vi.fn>;
  let originalHash: string;

  beforeEach(() => {
    currentPath = "/first";
    scrollToMock = vi.fn();
    window.scrollTo = scrollToMock as unknown as typeof window.scrollTo;
    originalHash = window.location.hash;
    // Ensure clean hash to start.
    if (window.location.hash) {
      window.location.hash = "";
    }
  });

  afterEach(() => {
    if (window.location.hash !== originalHash) {
      window.location.hash = originalHash;
    }
    vi.restoreAllMocks();
  });

  it("scrolls to top on initial mount", () => {
    render(<RouteScrollRestore />);
    expect(scrollToMock).toHaveBeenCalledWith({
      top: 0,
      behavior: "instant",
    });
  });

  it("scrolls to top again when the pathname changes", () => {
    const { rerender } = render(<RouteScrollRestore />);
    expect(scrollToMock).toHaveBeenCalledTimes(1);

    currentPath = "/second";
    rerender(<RouteScrollRestore />);

    expect(scrollToMock).toHaveBeenCalledTimes(2);
    expect(scrollToMock).toHaveBeenLastCalledWith({
      top: 0,
      behavior: "instant",
    });
  });

  it("does nothing when window.location.hash is set", () => {
    window.location.hash = "#section-2";
    render(<RouteScrollRestore />);
    expect(scrollToMock).not.toHaveBeenCalled();
  });
});
