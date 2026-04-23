import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { ChannelShell } from "./channel-shell";

function installMatchMedia(reduced: boolean) {
  const mm = vi.fn().mockImplementation((query: string) => ({
    matches:
      query === "(prefers-reduced-motion: reduce)" ? reduced : false,
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

describe("ChannelShell", () => {
  beforeEach(() => {
    installMatchMedia(false);
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("renders title, subtitle, children and the LiveDot (connected)", () => {
    render(
      <ChannelShell
        channelId="qq"
        title="QQ"
        subtitle="sub"
        connected
      >
        <p data-testid="body">hello</p>
      </ChannelShell>,
    );

    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("QQ");
    expect(screen.getByText("sub")).toBeInTheDocument();
    expect(screen.getByTestId("body")).toHaveTextContent("hello");
    expect(screen.getByTestId("channel-shell-live-dot")).toHaveAttribute(
      "data-variant",
      "ok",
    );
    // Default "Live" label rendered twice (sr-only + visible indicator).
    expect(screen.getAllByText("Live").length).toBeGreaterThan(0);
  });

  it("switches LiveDot variant + label when disconnected", () => {
    render(
      <ChannelShell channelId="qq" title="QQ" connected={false}>
        <div />
      </ChannelShell>,
    );
    expect(screen.getByTestId("channel-shell-live-dot")).toHaveAttribute(
      "data-variant",
      "err",
    );
    expect(screen.getAllByText("Offline").length).toBeGreaterThan(0);
  });

  it("honours an explicit connectionLabel override", () => {
    render(
      <ChannelShell
        channelId="telegram"
        title="Telegram"
        connected
        connectionLabel="Polling"
      >
        <div />
      </ChannelShell>,
    );
    expect(screen.getAllByText("Polling").length).toBeGreaterThan(0);
  });

  it("renders an actions slot in the top bar", () => {
    render(
      <ChannelShell
        channelId="qq"
        title="QQ"
        connected
        actions={<button data-testid="act">Reconnect</button>}
      >
        <div />
      </ChannelShell>,
    );
    expect(screen.getByTestId("act")).toHaveTextContent("Reconnect");
  });

  it("renders tabs and marks the active one with an underline", () => {
    render(
      <ChannelShell
        channelId="qq"
        title="QQ"
        connected
        tabs={[
          { id: "overview", label: "Overview" },
          { id: "keywords", label: "Keywords" },
        ]}
        activeTabId="keywords"
      >
        <div />
      </ChannelShell>,
    );
    const tabs = screen.getAllByRole("tab");
    expect(tabs).toHaveLength(2);
    const active = tabs.find((t) => t.getAttribute("aria-selected") === "true");
    expect(active).toBeDefined();
    expect(active).toHaveTextContent("Keywords");
    // Underline marker rendered for the active tab.
    expect(screen.getByTestId("channel-shell-tab-underline")).toBeInTheDocument();
  });

  it("invokes onTabChange when a non-link tab is clicked", () => {
    const onTabChange = vi.fn();
    render(
      <ChannelShell
        channelId="qq"
        title="QQ"
        connected
        tabs={[
          { id: "a", label: "A" },
          { id: "b", label: "B" },
        ]}
        activeTabId="a"
        onTabChange={onTabChange}
      >
        <div />
      </ChannelShell>,
    );
    fireEvent.click(screen.getByRole("tab", { name: "B" }));
    expect(onTabChange).toHaveBeenCalledWith("b");
  });

  it("under reduced-motion the underline is a plain span (no motion.span)", () => {
    installMatchMedia(true);
    render(
      <ChannelShell
        channelId="qq"
        title="QQ"
        connected
        tabs={[{ id: "a", label: "A" }]}
        activeTabId="a"
      >
        <div />
      </ChannelShell>,
    );
    const underline = screen.getByTestId("channel-shell-tab-underline");
    expect(underline.tagName.toLowerCase()).toBe("span");
  });
});
