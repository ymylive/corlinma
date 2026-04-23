import { describe, expect, it } from "vitest";
import { renderHook } from "@testing-library/react";

import {
  fadeUp,
  stagger,
  springPop,
  listItem,
  sharedCard,
  useMotionVariants,
} from "./motion";

describe("motion tokens", () => {
  it("fadeUp has hidden+visible states", () => {
    expect(fadeUp).toHaveProperty("hidden");
    expect(fadeUp).toHaveProperty("visible");
  });

  it("stagger orchestrates children on visible", () => {
    expect(stagger).toHaveProperty("visible");
    const visible = stagger.visible as { transition?: { staggerChildren?: number } };
    expect(visible.transition?.staggerChildren).toBeGreaterThan(0);
  });

  it("springPop scales from <1 to 1", () => {
    expect(springPop).toHaveProperty("hidden");
    expect(springPop).toHaveProperty("visible");
    const hidden = springPop.hidden as { scale?: number };
    expect(hidden.scale).toBeLessThan(1);
  });

  it("listItem has hidden+visible states", () => {
    expect(listItem).toHaveProperty("hidden");
    expect(listItem).toHaveProperty("visible");
  });

  it("sharedCard declares layout + spring transition", () => {
    expect(sharedCard).toMatchObject({
      layout: true,
      transition: { type: "spring" },
    });
  });
});

describe("useMotionVariants", () => {
  it("returns animated variants by default (reduced-motion off)", () => {
    const { result } = renderHook(() => useMotionVariants());
    expect(result.current.fadeUp).toBe(fadeUp);
    expect(result.current.stagger).toBe(stagger);
    expect(result.current.springPop).toBe(springPop);
    expect(result.current.listItem).toBe(listItem);
    expect(result.current.sharedCard).toBe(sharedCard);
  });
});
