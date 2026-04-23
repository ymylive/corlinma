/**
 * Diary page tests (B5-FE2).
 *
 * Covers:
 *   1. Renders 30 mock entries grouped across 10 date sections.
 *   2. Clicking an entry opens the reader modal with a matching
 *      `data-testid="diary-reader-<id>"`.
 *   3. Under `prefers-reduced-motion: reduce` entries are not fade-up
 *      animated — `data-entry-animated="false"` on every card.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import * as React from "react";

const replaceMock = vi.fn();
let searchParams = new URLSearchParams();

vi.mock("next/navigation", () => ({
  useRouter: () => ({ replace: replaceMock, push: vi.fn() }),
  useSearchParams: () => searchParams,
  usePathname: () => "/diary",
}));

import DiaryPage from "./page";
import { MOCK_DIARY, groupByDate } from "@/lib/mocks/diary";

function renderWithClient(ui: React.ReactElement) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

/**
 * jsdom doesn't ship an IntersectionObserver. framer-motion's `useInView`
 * touches it at effect time, so we polyfill before any render. Entries are
 * reported as immediately in-view so the fade-up animations settle in a
 * single pass.
 */
class MockIntersectionObserver {
  readonly root: Element | null = null;
  readonly rootMargin: string = "";
  readonly thresholds: ReadonlyArray<number> = [];
  private readonly _cb: IntersectionObserverCallback;
  constructor(cb: IntersectionObserverCallback) {
    this._cb = cb;
  }
  observe(target: Element): void {
    const entry = {
      isIntersecting: true,
      intersectionRatio: 1,
      target,
      boundingClientRect: target.getBoundingClientRect(),
      intersectionRect: target.getBoundingClientRect(),
      rootBounds: null,
      time: 0,
    } as IntersectionObserverEntry;
    this._cb([entry], this as unknown as IntersectionObserver);
  }
  unobserve(): void {}
  disconnect(): void {}
  takeRecords(): IntersectionObserverEntry[] {
    return [];
  }
}

function installIntersectionObserver() {
  Object.defineProperty(window, "IntersectionObserver", {
    writable: true,
    configurable: true,
    value: MockIntersectionObserver,
  });
  // framer-motion reads the global identifier directly in some paths.
  Object.defineProperty(globalThis, "IntersectionObserver", {
    writable: true,
    configurable: true,
    value: MockIntersectionObserver,
  });
}

/**
 * Installs a `window.matchMedia` shim. When `reducedMotion` is true the
 * `prefers-reduced-motion: reduce` query reports `matches: true`, which
 * lets us drive the reduced-motion branch of the page under test.
 */
function installMatchMedia(reducedMotion = false) {
  const mm = vi.fn().mockImplementation((query: string) => ({
    matches: reducedMotion && query.includes("prefers-reduced-motion: reduce"),
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

describe("DiaryPage", () => {
  beforeEach(() => {
    replaceMock.mockClear();
    searchParams = new URLSearchParams();
    installMatchMedia(false);
    installIntersectionObserver();
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("renders 30 entries grouped across 10 date sections", async () => {
    renderWithClient(<DiaryPage />);

    // Wait for the timeline to mount (the query resolves after a tiny tick).
    await waitFor(() => {
      expect(screen.getByTestId("diary-timeline")).toBeInTheDocument();
    });

    const expectedGroups = groupByDate(MOCK_DIARY);
    expect(expectedGroups).toHaveLength(10);
    expect(MOCK_DIARY).toHaveLength(30);

    // Every date section is present...
    for (const g of expectedGroups) {
      expect(screen.getByTestId(`diary-date-${g.date}`)).toBeInTheDocument();
    }
    // ...and every entry is rendered.
    for (const entry of MOCK_DIARY) {
      expect(
        screen.getByTestId(`diary-entry-${entry.id}`),
      ).toBeInTheDocument();
    }
  });

  it("clicking an entry opens the reader with a matching layoutId / testid", async () => {
    renderWithClient(<DiaryPage />);

    await waitFor(() => {
      expect(screen.getByTestId("diary-timeline")).toBeInTheDocument();
    });

    const target = MOCK_DIARY[0];
    const card = screen.getByTestId(`diary-entry-${target.id}`);
    // Inner clickable role="button" owns the open handler.
    const button = card.querySelector('[role="button"]') as HTMLElement;
    fireEvent.click(button);

    const reader = await screen.findByTestId(`diary-reader-${target.id}`);
    expect(reader).toBeInTheDocument();
  });

  it("reduced-motion: entries carry data-entry-animated=\"false\" (no fade-up)", async () => {
    installMatchMedia(true);
    renderWithClient(<DiaryPage />);

    await waitFor(() => {
      expect(screen.getByTestId("diary-timeline")).toBeInTheDocument();
    });

    // Every entry card reports reduced-motion. `getAllByTestId`-style
    // enumeration via `MOCK_DIARY` is more robust than a CSS selector.
    for (const e of MOCK_DIARY) {
      const card = screen.getByTestId(`diary-entry-${e.id}`);
      expect(card.getAttribute("data-entry-animated")).toBe("false");
      // And the `diary-entry-animated` class is absent in the reduced state.
      expect(card.className).not.toMatch(/diary-entry-animated/);
    }
  });
});
