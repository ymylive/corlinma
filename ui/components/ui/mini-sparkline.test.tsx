import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { MiniSparkline } from "./mini-sparkline";

afterEach(() => {
  cleanup();
});

describe("MiniSparkline", () => {
  const bars = [
    { height: 90 },
    { height: 80 },
    { height: 95 },
    { height: 70, tone: "warn" as const },
    { height: 100 },
    { height: 85 },
  ];

  it("renders one child per bar", () => {
    const { container } = render(<MiniSparkline bars={bars} />);
    const kids = container.querySelectorAll("div > span");
    expect(kids.length).toBe(bars.length);
  });

  it("is aria-hidden without a label", () => {
    render(<MiniSparkline bars={bars} data-testid="spark" />);
    expect(screen.getByTestId("spark")).toHaveAttribute("aria-hidden", "true");
  });

  it("exposes role=img + aria-label when labelled", () => {
    render(
      <MiniSparkline bars={bars} label="gateway 99.98%" data-testid="spark" />,
    );
    const el = screen.getByTestId("spark");
    expect(el).toHaveAttribute("role", "img");
    expect(el).toHaveAttribute("aria-label", "gateway 99.98%");
  });

  it("clamps height values out of 0–100 range", () => {
    const { container } = render(
      <MiniSparkline bars={[{ height: -5 }, { height: 200 }, { height: 42 }]} />,
    );
    const spans = container.querySelectorAll("div > span");
    expect((spans[0] as HTMLElement).style.height).toBe("0%");
    expect((spans[1] as HTMLElement).style.height).toBe("100%");
    expect((spans[2] as HTMLElement).style.height).toBe("42%");
  });
});
