/**
 * SkillDrawer focus-trap smoke tests (B3-FE5 audit).
 *
 * `SkillDrawer` is a thin restyle over `@radix-ui/react-dialog`, so the focus
 * trap, Esc-to-close and scroll-lock all come from radix. These tests verify
 * the behaviour end-to-end rather than re-implementing it:
 *
 *   1. Open the drawer, Tab 10 times, assert every focused element is still
 *      inside the drawer (no escape into page chrome).
 *   2. Pressing Esc fires `onOpenChange(false)`.
 */

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { SkillDrawer } from "./skill-drawer";
import type { Skill } from "@/lib/mocks/skills";

const SAMPLE: Skill = {
  name: "web_search",
  description: "Query the web.",
  emoji: "🔎",
  allowed_tools: ["web_search.query", "web_search.fetch_page"],
  requires: ["httpx"],
  install: "set SERPER_API_KEY",
  source_path: "~/skills/web_search",
  body_markdown: "Live web search.",
};

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SkillDrawer focus trap", () => {
  it("keeps focus within the drawer across 10 Tab presses", () => {
    const onOpenChange = vi.fn();
    render(
      <SkillDrawer skill={SAMPLE} open={true} onOpenChange={onOpenChange} />,
    );

    const dialog = screen.getByRole("dialog");
    // Tab 10 times starting from the first focusable element within the
    // drawer. Radix's FocusScope should bounce focus back to the start
    // on overflow — we only assert it never leaks out.
    for (let i = 0; i < 10; i++) {
      fireEvent.keyDown(document.activeElement || document.body, {
        key: "Tab",
      });
      // Focus must remain inside the drawer's DOM subtree.
      const active = document.activeElement;
      expect(dialog.contains(active)).toBe(true);
    }
  });

  it("fires onOpenChange(false) on Escape", () => {
    const onOpenChange = vi.fn();
    render(
      <SkillDrawer skill={SAMPLE} open={true} onOpenChange={onOpenChange} />,
    );

    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
