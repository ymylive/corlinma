import { afterEach, describe, expect, it, vi } from "vitest";
import { render, renderHook, screen } from "@testing-library/react";
import {
  LiveLabels,
  LiveRegion,
  MotionSafe,
  VisuallyHidden,
  useMotion,
} from "./motion-safe";

type MediaMap = Record<string, boolean>;

function mockMatchMedia(map: MediaMap) {
  const listeners = new Set<() => void>();
  const impl = vi.fn((query: string) => {
    const matches = map[query] ?? false;
    const mql: MediaQueryList = {
      matches,
      media: query,
      onchange: null,
      addEventListener: (_type: string, cb: EventListener) => {
        listeners.add(cb as () => void);
      },
      removeEventListener: (_type: string, cb: EventListener) => {
        listeners.delete(cb as () => void);
      },
      addListener: (cb: () => void) => listeners.add(cb),
      removeListener: (cb: () => void) => listeners.delete(cb),
      dispatchEvent: () => false,
    };
    return mql;
  });
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: impl,
  });
  return impl;
}

afterEach(() => {
  // Best-effort cleanup; jsdom resets the global between tests but not the
  // property definition we wrote above.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  delete (window as any).matchMedia;
});

describe("useMotion", () => {
  it("reports reduced=true when the user prefers reduced motion", () => {
    mockMatchMedia({ "(prefers-reduced-motion: reduce)": true });
    const { result } = renderHook(() => useMotion());
    expect(result.current.reduced).toBe(true);
    expect(result.current.motionSafe).toBe(false);
  });

  it("reports reduced=false when the media query does not match", () => {
    mockMatchMedia({ "(prefers-reduced-motion: reduce)": false });
    const { result } = renderHook(() => useMotion());
    expect(result.current.reduced).toBe(false);
    expect(result.current.motionSafe).toBe(true);
  });

  it("reports touch=true on a coarse pointer", () => {
    mockMatchMedia({ "(pointer: coarse)": true });
    const { result } = renderHook(() => useMotion());
    expect(result.current.touch).toBe(true);
  });
});

describe("<MotionSafe>", () => {
  it("renders children when motion is safe", () => {
    mockMatchMedia({ "(prefers-reduced-motion: reduce)": false });
    render(
      <MotionSafe fallback={<span>static</span>}>
        <span>animated</span>
      </MotionSafe>,
    );
    expect(screen.getByText("animated")).toBeInTheDocument();
    expect(screen.queryByText("static")).not.toBeInTheDocument();
  });

  it("renders fallback when the user prefers reduced motion", () => {
    mockMatchMedia({ "(prefers-reduced-motion: reduce)": true });
    render(
      <MotionSafe fallback={<span>static</span>}>
        <span>animated</span>
      </MotionSafe>,
    );
    expect(screen.getByText("static")).toBeInTheDocument();
    expect(screen.queryByText("animated")).not.toBeInTheDocument();
  });

  it("passes children through when no fallback is supplied even under reduced motion", () => {
    mockMatchMedia({ "(prefers-reduced-motion: reduce)": true });
    render(
      <MotionSafe>
        <span>child</span>
      </MotionSafe>,
    );
    expect(screen.getByText("child")).toBeInTheDocument();
  });
});

describe("<LiveRegion>", () => {
  it("renders polite live region with role=status by default", () => {
    mockMatchMedia({});
    render(<LiveRegion>{LiveLabels.updating}</LiveRegion>);
    const el = screen.getByRole("status");
    expect(el).toHaveAttribute("aria-live", "polite");
    expect(el).toHaveTextContent("Updating");
    expect(el).toHaveClass("sr-only");
    expect(el).not.toHaveAttribute("aria-label");
  });

  it("renders assertive when politeness=assertive and applies aria-label", () => {
    mockMatchMedia({});
    render(
      <LiveRegion politeness="assertive" label="Dashboard updates">
        Live data
      </LiveRegion>,
    );
    const el = screen.getByRole("status");
    expect(el).toHaveAttribute("aria-live", "assertive");
    expect(el).toHaveAttribute("aria-label", "Dashboard updates");
  });
});

describe("<VisuallyHidden>", () => {
  it("applies the sr-only Tailwind utility", () => {
    mockMatchMedia({});
    render(<VisuallyHidden>hidden copy</VisuallyHidden>);
    const el = screen.getByText("hidden copy");
    expect(el).toHaveClass("sr-only");
  });
});
