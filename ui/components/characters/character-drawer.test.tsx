/**
 * CharacterDrawer focus-trap smoke tests (B3-FE5 audit).
 *
 * Same contract as `skill-drawer.test.tsx`: the drawer wraps
 * `@radix-ui/react-dialog`, so we verify radix's focus trap + Esc-to-close
 * actually take effect in this restyled surface.
 */

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { CharacterDrawer } from "./character-drawer";
import type { AgentCard } from "@/lib/mocks/characters";

const SAMPLE: AgentCard = {
  name: "Mentor",
  emoji: "🧑‍🏫",
  description: "A senior developer who reviews your code.",
  system_prompt: "You are {{agent.mentor}}.",
  variables: { tone: "encouraging" },
  tools_allowed: ["read_file", "search_code"],
  skill_refs: [],
  source_path: "Agent/Mentor.md",
};

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("CharacterDrawer focus trap", () => {
  it("keeps focus inside the drawer across 10 Tab presses", () => {
    const onOpenChange = vi.fn();
    render(
      <CharacterDrawer open={true} onOpenChange={onOpenChange} card={SAMPLE} />,
    );

    const dialog = screen.getByRole("dialog");
    for (let i = 0; i < 10; i++) {
      fireEvent.keyDown(document.activeElement || document.body, {
        key: "Tab",
      });
      expect(dialog.contains(document.activeElement)).toBe(true);
    }
  });

  it("closes on Escape", () => {
    const onOpenChange = vi.fn();
    render(
      <CharacterDrawer open={true} onOpenChange={onOpenChange} card={SAMPLE} />,
    );

    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
