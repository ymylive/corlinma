/**
 * <Drawer> primitive tests (B4-FE4).
 *
 * The drawer is built on `@radix-ui/react-dialog`, so most of its
 * accessibility behaviour (focus-trap, Tab cycling, `role=dialog`) comes for
 * free from radix. These tests cover the seams that *this* wrapper owns:
 * title/description wiring, Esc + overlay-click dismissal, the `dismissable`
 * escape hatch, the `width` preset → class map, and the reduced-motion snap.
 */

import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

import { Drawer } from "./drawer";

type MediaMap = Record<string, boolean>;

function mockMatchMedia(map: MediaMap) {
  const impl = vi.fn((query: string) => {
    const matches = map[query] ?? false;
    const mql: MediaQueryList = {
      matches,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
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
  cleanup();
  vi.restoreAllMocks();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  delete (window as any).matchMedia;
});

describe("<Drawer>", () => {
  it("renders title and description", () => {
    mockMatchMedia({});
    render(
      <Drawer
        open
        onOpenChange={() => {}}
        title="Drawer title"
        description="Drawer description"
      >
        <p>body</p>
      </Drawer>,
    );
    expect(screen.getByText("Drawer title")).toBeInTheDocument();
    expect(screen.getByText("Drawer description")).toBeInTheDocument();
    // Radix wires `role=dialog` + `aria-labelledby`/`aria-describedby`.
    // (Radix does not set `aria-modal` — it blocks the background via
    // `aria-hidden` on siblings instead, which still satisfies the contract.)
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveAttribute("aria-labelledby");
    expect(dialog).toHaveAttribute("aria-describedby");
  });

  it("closes on Escape", () => {
    mockMatchMedia({});
    const onOpenChange = vi.fn();
    render(
      <Drawer open onOpenChange={onOpenChange} title="T">
        <p>body</p>
      </Drawer>,
    );
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("closes on overlay click", async () => {
    mockMatchMedia({});
    const onOpenChange = vi.fn();
    render(
      <Drawer open onOpenChange={onOpenChange} title="T">
        <p>body</p>
      </Drawer>,
    );
    // Radix's DismissableLayer registers its `pointerdown` document
    // listener on a setTimeout(0); drain the task queue before firing.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });
    const overlay = document.querySelector(
      ".fixed.inset-0",
    ) as HTMLElement | null;
    expect(overlay).not.toBeNull();
    fireEvent.pointerDown(overlay!);
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
  });

  it("ignores Escape when dismissable=false", () => {
    mockMatchMedia({});
    const onOpenChange = vi.fn();
    render(
      <Drawer open onOpenChange={onOpenChange} title="T" dismissable={false}>
        <p>body</p>
      </Drawer>,
    );
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onOpenChange).not.toHaveBeenCalled();
  });

  it("applies the expected max-width class for each width preset", () => {
    mockMatchMedia({});
    const { rerender } = render(
      <Drawer open onOpenChange={() => {}} title="T" width="sm">
        <p>body</p>
      </Drawer>,
    );
    expect(screen.getByRole("dialog").className).toMatch(/max-w-\[360px\]/);

    rerender(
      <Drawer open onOpenChange={() => {}} title="T" width="lg">
        <p>body</p>
      </Drawer>,
    );
    expect(screen.getByRole("dialog").className).toMatch(/max-w-\[560px\]/);

    rerender(
      <Drawer open onOpenChange={() => {}} title="T" width="xl">
        <p>body</p>
      </Drawer>,
    );
    expect(screen.getByRole("dialog").className).toMatch(/max-w-\[720px\]/);
  });

  it("does not apply slide motion classes under prefers-reduced-motion", () => {
    mockMatchMedia({ "(prefers-reduced-motion: reduce)": true });
    render(
      <Drawer open onOpenChange={() => {}} title="T">
        <p>body</p>
      </Drawer>,
    );
    const dialog = screen.getByRole("dialog");
    // framer-motion emits a `transform` inline style for slides. Under
    // reduced motion we set `initial={{ x: 0 }}` and `transition={{ duration:0 }}`,
    // so no `translateX` that isn't 0 should appear and no tailwindcss-animate
    // `animate-in` / `slide-in-*` class should be emitted by the drawer.
    expect(dialog.className).not.toMatch(/animate-in|slide-in|slide-out/);
    const style = dialog.getAttribute("style") ?? "";
    // Either no transform, or a transform that is the identity on X.
    // framer-motion writes `transform: translateX(0px) ...`.
    expect(style).not.toMatch(/translateX\((?!0)/);
  });
});
