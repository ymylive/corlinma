import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, fireEvent } from "@testing-library/react";
import { DetailDrawer } from "./detail-drawer";

afterEach(() => cleanup());

describe("DetailDrawer", () => {
  it("renders title + subsystem + section", () => {
    render(
      <DetailDrawer title={<>403 on <code>ws-tool://node-b/read</code></>} subsystem="plugin:file_fetcher">
        <DetailDrawer.Section label="Payload">
          <div data-testid="content">content</div>
        </DetailDrawer.Section>
      </DetailDrawer>,
    );
    expect(screen.getByText(/403 on/)).toBeInTheDocument();
    expect(screen.getByText("plugin:file_fetcher")).toBeInTheDocument();
    expect(screen.getByText("Payload")).toBeInTheDocument();
    expect(screen.getByTestId("content")).toBeInTheDocument();
  });

  it("renders trace-id + copy button and fires onCopy", () => {
    const onCopy = vi.fn();
    render(
      <DetailDrawer
        title="x"
        trace={{ id: "b0f3c8a9-7e42", onCopy }}
      >
        <div />
      </DetailDrawer>,
    );
    expect(screen.getByText("b0f3c8a9-7e42")).toBeInTheDocument();
    fireEvent.click(screen.getByText("copy"));
    expect(onCopy).toHaveBeenCalledWith("b0f3c8a9-7e42");
  });
});
