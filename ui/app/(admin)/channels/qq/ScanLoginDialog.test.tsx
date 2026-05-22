import { describe, expect, it, afterEach } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { ScanLoginDialog } from "./ScanLoginDialog";

/**
 * ScanLoginDialog now embeds NapCat's first-party WebUI (reverse-proxied
 * at `/webui`) in an iframe — the relay-based QR flow was removed. These
 * tests just assert the iframe is mounted only while the dialog is open.
 */
describe("ScanLoginDialog", () => {
  afterEach(() => cleanup());

  it("embeds the NapCat WebUI iframe when open", () => {
    render(<ScanLoginDialog open onOpenChange={() => {}} />);
    const frame = screen.getByTestId("qq-napcat-webui");
    expect(frame.tagName).toBe("IFRAME");
    expect(frame.getAttribute("src")).toBe("/webui");
  });

  it("does not mount the iframe while closed", () => {
    render(<ScanLoginDialog open={false} onOpenChange={() => {}} />);
    expect(screen.queryByTestId("qq-napcat-webui")).toBeNull();
  });
});
