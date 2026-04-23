import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { SkillCard, categorize } from "./skill-card";
import type { Skill } from "@/lib/mocks/skills";

function mockMatchMedia(opts: { reduce?: boolean; coarse?: boolean } = {}) {
  const { reduce = false, coarse = false } = opts;
  window.matchMedia = vi.fn().mockImplementation((query: string) => {
    let matches = false;
    if (query === "(prefers-reduced-motion: reduce)") matches = reduce;
    else if (query === "(pointer: coarse)") matches = coarse;
    return {
      matches,
      media: query,
      onchange: null,
      addEventListener: () => void 0,
      removeEventListener: () => void 0,
      addListener: () => void 0,
      removeListener: () => void 0,
      dispatchEvent: () => false,
    };
  }) as typeof window.matchMedia;
}

const sampleSkill: Skill = {
  name: "web_search",
  description: "Query the web.",
  emoji: "🔎",
  allowed_tools: ["web_search.query", "web_search.fetch_page", "web_search.rank", "web_search.cite"],
  requires: ["httpx"],
  install: "set SERPER_API_KEY",
  source_path: "~/skills/web_search",
  body_markdown: "Live web search.",
};

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("categorize", () => {
  it("classifies web_search as search", () => {
    expect(categorize("web_search", ["web_search.query"])).toBe("search");
  });

  it("classifies memory-prefixed skills as memory", () => {
    expect(categorize("memory", ["memory.recall"])).toBe("memory");
  });

  it("classifies file_ops / canvas / browser as dev-tools", () => {
    expect(categorize("file_ops", [])).toBe("dev-tools");
    expect(categorize("canvas", [])).toBe("dev-tools");
    expect(categorize("browser", [])).toBe("dev-tools");
  });

  it("classifies 3rd-party SaaS as integrations", () => {
    expect(categorize("discord", ["discord.send_message"])).toBe("integrations");
    expect(categorize("gh_issues", ["gh_issues.list"])).toBe("integrations");
    expect(categorize("1password", ["1password.read_item"])).toBe("integrations");
  });

  it("falls back to other when nothing matches", () => {
    expect(categorize("mystery_thing", ["unknown.do"])).toBe("other");
  });

  it("uses tool-prefix hints when name is ambiguous", () => {
    expect(categorize("bundle", ["web_search.query"])).toBe("search");
  });
});

describe("SkillCard", () => {
  it("renders name, description, first 3 tools and +N more", () => {
    mockMatchMedia();
    const onOpen = vi.fn();
    render(<SkillCard skill={sampleSkill} onOpen={onOpen} />);

    expect(screen.getByText("web_search")).toBeInTheDocument();
    expect(screen.getByText("Query the web.")).toBeInTheDocument();
    expect(screen.getByText("web_search.query")).toBeInTheDocument();
    expect(screen.getByText("web_search.fetch_page")).toBeInTheDocument();
    expect(screen.getByText("web_search.rank")).toBeInTheDocument();
    // Fourth tool is hidden behind an overflow chip.
    expect(screen.queryByText("web_search.cite")).not.toBeInTheDocument();
    expect(screen.getByText("+1 more")).toBeInTheDocument();
  });

  it("opens on click and on Enter keypress", () => {
    mockMatchMedia();
    const onOpen = vi.fn();
    render(<SkillCard skill={sampleSkill} onOpen={onOpen} />);

    const card = screen.getByRole("button", {
      name: /Open web_search skill details/,
    });
    fireEvent.click(card);
    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(onOpen).toHaveBeenCalledWith(sampleSkill);

    fireEvent.keyDown(card, { key: "Enter" });
    expect(onOpen).toHaveBeenCalledTimes(2);

    fireEvent.keyDown(card, { key: " " });
    expect(onOpen).toHaveBeenCalledTimes(3);
  });

  it("exposes data-category attribute for a11y/styling hooks", () => {
    mockMatchMedia();
    render(<SkillCard skill={sampleSkill} onOpen={() => void 0} />);
    const wrapper = screen.getByTestId("skill-card-web_search");
    expect(wrapper.getAttribute("data-category")).toBe("search");
  });
});
