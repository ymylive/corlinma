import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { CharacterCard, tiltForName } from "./character-card";
import type { AgentCard } from "@/lib/mocks/characters";

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

const SAMPLE: AgentCard = {
  name: "Mentor",
  emoji: "🧑‍🏫",
  description: "A senior developer who reviews your code.",
  system_prompt: "You are {{agent.mentor}}.",
  variables: { tone: "encouraging" },
  tools_allowed: ["read_file", "search_code", "run_tests"],
  skill_refs: [],
  source_path: "Agent/Mentor.md",
};

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("CharacterCard", () => {
  it("renders the agent name and description on the back face", () => {
    mockMatchMedia(false);
    render(
      <CharacterCard
        card={SAMPLE}
        flipped={false}
        rotateDeg={0}
        onFlip={() => {}}
        onEdit={() => {}}
      />,
    );
    // Both faces mount for the 3D flip; the back face carries the primary copy.
    const back = screen.getByTestId("character-card-back-Mentor");
    expect(back).toHaveTextContent("Mentor");
    expect(back).toHaveTextContent("A senior developer who reviews your code.");
    expect(back).toHaveAttribute("aria-pressed", "false");
  });

  it("toggles aria-pressed when flipped", () => {
    mockMatchMedia(false);
    const onFlip = vi.fn();
    const { rerender } = render(
      <CharacterCard
        card={SAMPLE}
        flipped={false}
        rotateDeg={0}
        onFlip={onFlip}
        onEdit={() => {}}
      />,
    );
    const back = screen.getByTestId("character-card-back-Mentor");
    expect(back).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(back);
    expect(onFlip).toHaveBeenCalledTimes(1);

    rerender(
      <CharacterCard
        card={SAMPLE}
        flipped
        rotateDeg={0}
        onFlip={onFlip}
        onEdit={() => {}}
      />,
    );
    const front = screen.getByTestId("character-card-front-Mentor");
    expect(front).toHaveAttribute("aria-pressed", "true");
  });

  it("renders the top 3 tools on the front face", () => {
    mockMatchMedia(false);
    render(
      <CharacterCard
        card={SAMPLE}
        flipped
        rotateDeg={0}
        onFlip={() => {}}
        onEdit={() => {}}
      />,
    );
    const front = screen.getByTestId("character-card-front-Mentor");
    expect(front).toHaveTextContent("read_file");
    expect(front).toHaveTextContent("search_code");
    expect(front).toHaveTextContent("run_tests");
  });

  it("fires onEdit without re-flipping the card", () => {
    mockMatchMedia(false);
    const onFlip = vi.fn();
    const onEdit = vi.fn();
    render(
      <CharacterCard
        card={SAMPLE}
        flipped
        rotateDeg={0}
        onFlip={onFlip}
        onEdit={onEdit}
      />,
    );
    fireEvent.click(screen.getByTestId("character-card-edit-Mentor"));
    expect(onEdit).toHaveBeenCalledTimes(1);
    expect(onFlip).not.toHaveBeenCalled();
  });

  it("falls back to an instant swap under reduced motion", () => {
    mockMatchMedia(true);
    render(
      <CharacterCard
        card={SAMPLE}
        flipped={false}
        rotateDeg={0}
        onFlip={() => {}}
        onEdit={() => {}}
      />,
    );
    // Only the back face mounts when reduced motion swaps instead of flips.
    expect(screen.getByTestId("character-card-back-Mentor")).toBeInTheDocument();
    expect(
      screen.queryByTestId("character-card-front-Mentor"),
    ).not.toBeInTheDocument();
  });
});

describe("tiltForName", () => {
  it("returns a stable value in [-1, 1] for the same name", () => {
    const a = tiltForName("Mentor");
    const b = tiltForName("Mentor");
    expect(a).toBe(b);
    expect(a).toBeGreaterThanOrEqual(-1);
    expect(a).toBeLessThanOrEqual(1);
  });

  it("returns different values for different names", () => {
    const all = ["Mentor", "Researcher", "Critic", "DataSci"].map(tiltForName);
    const unique = new Set(all);
    expect(unique.size).toBeGreaterThan(1);
  });
});
