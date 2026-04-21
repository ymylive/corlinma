import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import {
  DynamicParamsForm,
  validateAgainstSchema,
} from "../dynamic-params-form";

describe("validateAgainstSchema", () => {
  it("flags out-of-range numbers", () => {
    const errors = validateAgainstSchema(
      {
        type: "object",
        properties: {
          temperature: { type: "number", minimum: 0, maximum: 1 },
        },
      },
      { temperature: 2 },
    );
    expect(errors["/temperature"]).toMatch(/≤/);
  });

  it("flags missing required fields", () => {
    const errors = validateAgainstSchema(
      {
        type: "object",
        required: ["model"],
        properties: { model: { type: "string" } },
      },
      {},
    );
    expect(errors["/model"]).toBe("required");
  });

  it("flags enum mismatch", () => {
    const errors = validateAgainstSchema(
      {
        type: "object",
        properties: { mode: { type: "string", enum: ["a", "b"] } },
      },
      { mode: "c" },
    );
    expect(errors["/mode"]).toBe("not in allowed values");
  });
});

describe("DynamicParamsForm", () => {
  it("renders a slider for bounded numbers and a switch for booleans", () => {
    const onChange = vi.fn();
    render(
      <DynamicParamsForm
        schema={{
          type: "object",
          properties: {
            temperature: {
              type: "number",
              minimum: 0,
              maximum: 2,
              title: "temperature",
            },
            stream: { type: "boolean", title: "stream" },
          },
        }}
        value={{ temperature: 0.7, stream: false }}
        onChange={onChange}
      />,
    );

    // slider + linked number input for temperature
    expect(
      screen.getByTestId("params-temperature-range"),
    ).toBeInTheDocument();
    // boolean renders a switch (role="switch")
    expect(screen.getByRole("switch")).toHaveAttribute(
      "aria-checked",
      "false",
    );
  });

  it("commits number changes via the slider", () => {
    const onChange = vi.fn();
    render(
      <DynamicParamsForm
        schema={{
          type: "object",
          properties: {
            temperature: { type: "number", minimum: 0, maximum: 2 },
          },
        }}
        value={{ temperature: 0.5 }}
        onChange={onChange}
      />,
    );

    const slider = screen.getByTestId("params-temperature-range");
    fireEvent.change(slider, { target: { value: "1.5" } });
    expect(onChange).toHaveBeenCalledWith({ temperature: 1.5 });
  });

  it("emits empty form when schema has no properties", () => {
    const { container } = render(
      <DynamicParamsForm
        schema={{ type: "object", properties: {} }}
        value={{}}
        onChange={() => {}}
      />,
    );
    // no form wrapper, just the "empty" hint
    expect(container.querySelector("[data-testid$=-form]")).toBeNull();
  });
});
