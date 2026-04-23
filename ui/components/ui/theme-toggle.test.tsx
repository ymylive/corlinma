import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, fireEvent } from "@testing-library/react";
import { ThemeToggle, THEME_STORAGE_KEY } from "./theme-toggle";

// The jsdom environment used by vitest has an incomplete localStorage shim
// (the Node `--localstorage-file` flag is mis-configured in this project).
// Install a clean in-memory Storage mock per test file.
let store: Record<string, string> = {};
Object.defineProperty(window, "localStorage", {
  value: {
    getItem: (k: string) => (k in store ? store[k] : null),
    setItem: (k: string, v: string) => {
      store[k] = String(v);
    },
    removeItem: (k: string) => {
      delete store[k];
    },
    clear: () => {
      store = {};
    },
    key: (i: number) => Object.keys(store)[i] ?? null,
    get length() {
      return Object.keys(store).length;
    },
  },
  writable: true,
  configurable: true,
});

beforeEach(() => {
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.classList.remove("dark");
  store = {};
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("ThemeToggle", () => {
  it("defaults to dark when no theme attribute is set", () => {
    render(<ThemeToggle />);
    const dark = screen.getByLabelText("Night mode");
    expect(dark).toHaveAttribute("aria-pressed", "true");
  });

  it("respects `initial` prop", () => {
    render(<ThemeToggle initial="light" />);
    expect(screen.getByLabelText("Day mode")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("flips to light, writes data-theme, removes .dark, persists", () => {
    render(<ThemeToggle initial="dark" />);
    fireEvent.click(screen.getByLabelText("Day mode"));

    expect(document.documentElement.dataset.theme).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("light");
    expect(screen.getByLabelText("Day mode")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("fires onThemeChange callback", () => {
    const onThemeChange = vi.fn();
    render(<ThemeToggle initial="dark" onThemeChange={onThemeChange} />);
    fireEvent.click(screen.getByLabelText("Day mode"));
    expect(onThemeChange).toHaveBeenCalledWith("light");
  });

  it("reflects external data-theme attribute changes via MutationObserver", async () => {
    render(<ThemeToggle initial="dark" />);
    document.documentElement.dataset.theme = "light";
    // Let the MutationObserver microtask fire.
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.getByLabelText("Day mode")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("uses role=tablist and each option is role=tab", () => {
    render(<ThemeToggle />);
    const group = screen.getByRole("tablist");
    expect(group).toHaveAttribute("aria-label", "Theme");
    const tabs = screen.getAllByRole("tab");
    expect(tabs).toHaveLength(2);
  });
});
