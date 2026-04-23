import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { act } from "react";
import * as React from "react";

// Mock the SSE helper so tests don't spawn real EventSource connections.
vi.mock("@/lib/sse", () => ({
  openEventStream: vi.fn(() => () => {}),
}));

// Mock the canvas API client so each test can drive live / fallback paths.
vi.mock("@/lib/api/canvas", async () => {
  const actual = await vi.importActual<typeof import("@/lib/api/canvas")>(
    "@/lib/api/canvas",
  );
  return {
    ...actual,
    createCanvasSession: vi.fn(),
    sendCanvasFrame: vi.fn(),
  };
});

import { createCanvasSession, sendCanvasFrame } from "@/lib/api/canvas";
import CanvasPage from "./page";
import { makeMockSession } from "@/lib/mocks/canvas";

const mockedCreate = vi.mocked(createCanvasSession);
const mockedSend = vi.mocked(sendCanvasFrame);

function installMatchMedia() {
  const mm = vi.fn().mockImplementation((query: string) => ({
    matches: false,
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

beforeEach(() => {
  installMatchMedia();
  vi.useFakeTimers({ shouldAdvanceTime: true });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.useRealTimers();
});

describe("CanvasPage", () => {
  it("renders the iframe surface and protocol inspector", async () => {
    const session = makeMockSession();
    mockedCreate.mockResolvedValue({ kind: "live", session });

    render(<CanvasPage />);

    // The iframe (canvas surface) is always present and sandboxed.
    const iframe = await screen.findByTestId("canvas-iframe");
    expect(iframe.tagName).toBe("IFRAME");
    expect(iframe.getAttribute("sandbox")).toBe("allow-same-origin");
    expect(iframe.getAttribute("title")).toBe("Canvas surface placeholder");

    // The bottom inspector renders with its toggle.
    expect(screen.getByTestId("canvas-inspector")).toBeInTheDocument();
    expect(screen.getByTestId("canvas-inspector-toggle")).toBeInTheDocument();

    // Session id is displayed in the header.
    await waitFor(() => {
      expect(screen.getByTestId("canvas-session-id")).toHaveTextContent(
        session.id,
      );
    });
  });

  it("sending a frame posts via the client and prepends the event to the inspector", async () => {
    const session = makeMockSession();
    mockedCreate.mockResolvedValue({ kind: "live", session });
    mockedSend.mockResolvedValue({ kind: "live", ok: true });

    render(<CanvasPage />);

    // Wait for session to settle.
    await waitFor(() => {
      expect(screen.getByTestId("canvas-session-id")).toHaveTextContent(
        session.id,
      );
    });

    // Expand the inspector so new rows become assertable.
    fireEvent.click(screen.getByTestId("canvas-inspector-toggle"));

    // Rewrite the payload to something small + predictable.
    const textarea = screen.getByTestId(
      "canvas-send-payload",
    ) as HTMLTextAreaElement;
    fireEvent.change(textarea, { target: { value: '{"ping":1}' } });

    fireEvent.click(screen.getByTestId("canvas-send-submit"));

    await waitFor(() => {
      expect(mockedSend).toHaveBeenCalledWith(
        expect.objectContaining({
          session_id: session.id,
          kind: "a2ui_push",
          payload: { ping: 1 },
        }),
      );
    });

    // The frame is mirrored into the inspector as the newest event.
    const payloads = await screen.findAllByText(/"ping":1/);
    expect(payloads.length).toBeGreaterThan(0);
  });

  it("shows the fallback banner and rotates mock events when the endpoint is disabled", async () => {
    const session = makeMockSession();
    mockedCreate.mockResolvedValue({ kind: "fallback", session });

    render(<CanvasPage />);

    // Fallback banner is announced.
    expect(
      await screen.findByTestId("canvas-fallback-banner"),
    ).toBeInTheDocument();

    // First mock event lands immediately.
    fireEvent.click(screen.getByTestId("canvas-inspector-toggle"));
    await waitFor(() => {
      expect(
        screen.getByTestId("canvas-inspector").textContent,
      ).toMatch(/a2ui_push|navigate|present|eval|snapshot|hide/);
    });

    const firstCount =
      screen.getByTestId("canvas-inspector").querySelectorAll("li").length;

    // Advance 2s — a second mock event should have been appended.
    await act(async () => {
      vi.advanceTimersByTime(2_100);
    });

    await waitFor(() => {
      const nextCount =
        screen.getByTestId("canvas-inspector").querySelectorAll("li").length;
      expect(nextCount).toBeGreaterThan(firstCount);
    });
  });
});
