import { describe, expect, it } from "vitest";
import config from "./tailwind.config";

describe("Tidepool glass blur tokens", () => {
  it("keeps soft and strong glass blur consistent and slightly lighter", () => {
    const extend = config.theme?.extend;

    expect(extend?.backdropBlur).toMatchObject({
      glass: "8px",
      "glass-strong": "8px",
    });
  });
});
