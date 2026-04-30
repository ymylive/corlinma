/**
 * Tenant switcher tests (Phase 4 W1 4-1B).
 *
 * Covers:
 *   - Renders a `<select>` populated from /admin/tenants
 *   - Selecting a non-default slug calls `navigate` with `?tenant=<slug>`
 *   - Selecting `default` strips the `?tenant=` param
 *   - 403 tenants_disabled renders the disabled-state select
 *   - Hard error / 401 hides the switcher entirely
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import * as React from "react";

let pathname = "/plugins";
let searchString = "";

vi.mock("next/navigation", () => ({
  usePathname: () => pathname,
  useSearchParams: () => new URLSearchParams(searchString),
}));

vi.mock("@/lib/api/tenants", async () => {
  const actual = await vi.importActual<typeof import("@/lib/api/tenants")>(
    "@/lib/api/tenants",
  );
  return {
    ...actual,
    fetchTenants: vi.fn(),
  };
});

import { fetchTenants } from "@/lib/api/tenants";
import { TenantSwitcher } from "./tenant-switcher";

const mockedFetch = vi.mocked(fetchTenants);

function renderSwitcher(navigate?: (href: string) => void) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <TenantSwitcher navigate={navigate} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  pathname = "/plugins";
  searchString = "";
  mockedFetch.mockReset();
});

afterEach(() => cleanup());

describe("TenantSwitcher — populated", () => {
  beforeEach(() => {
    mockedFetch.mockResolvedValue({
      kind: "ok",
      data: {
        tenants: [
          {
            tenant_id: "default",
            display_name: "Default tenant",
            created_at: "2026-04-01T00:00:00Z",
          },
          {
            tenant_id: "acme",
            display_name: "ACME",
            created_at: "2026-04-12T00:00:00Z",
          },
          {
            tenant_id: "bravo",
            display_name: "Bravo",
            created_at: "2026-04-18T00:00:00Z",
          },
        ],
        allowed: ["default", "acme", "bravo"],
      },
    });
  });

  it("renders a select option per tenant", async () => {
    renderSwitcher();
    const select = (await screen.findByTestId(
      "tenant-switcher-select",
    )) as HTMLSelectElement;
    const opts = Array.from(select.options).map((o) => o.value);
    expect(opts).toEqual(["default", "acme", "bravo"]);
  });

  it("appends ?tenant=<slug> when picking a non-default tenant", async () => {
    const nav = vi.fn();
    renderSwitcher(nav);
    const select = (await screen.findByTestId(
      "tenant-switcher-select",
    )) as HTMLSelectElement;
    fireEvent.change(select, { target: { value: "acme" } });
    expect(nav).toHaveBeenCalledWith("/plugins?tenant=acme");
  });

  it("strips ?tenant when selecting the default", async () => {
    pathname = "/plugins";
    searchString = "tenant=acme";
    const nav = vi.fn();
    renderSwitcher(nav);
    const select = (await screen.findByTestId(
      "tenant-switcher-select",
    )) as HTMLSelectElement;
    fireEvent.change(select, { target: { value: "default" } });
    expect(nav).toHaveBeenCalledWith("/plugins");
  });

  it("preserves unrelated query params when switching tenant", async () => {
    pathname = "/plugins";
    searchString = "filter=loaded";
    const nav = vi.fn();
    renderSwitcher(nav);
    const select = (await screen.findByTestId(
      "tenant-switcher-select",
    )) as HTMLSelectElement;
    fireEvent.change(select, { target: { value: "acme" } });
    expect(nav).toHaveBeenCalledWith("/plugins?filter=loaded&tenant=acme");
  });

  it("reflects the current ?tenant= param as the active option", async () => {
    pathname = "/plugins";
    searchString = "tenant=bravo";
    renderSwitcher();
    const select = (await screen.findByTestId(
      "tenant-switcher-select",
    )) as HTMLSelectElement;
    expect(select.value).toBe("bravo");
  });
});

describe("TenantSwitcher — degraded states", () => {
  it("renders disabled when the API reports tenants_disabled", async () => {
    mockedFetch.mockResolvedValue({ kind: "disabled" });
    renderSwitcher();
    const select = (await screen.findByTestId(
      "tenant-switcher-select",
    )) as HTMLSelectElement;
    expect(select.disabled).toBe(true);
  });

  it("hides itself entirely on a hard error", async () => {
    mockedFetch.mockResolvedValue({ kind: "error", message: "boom" });
    const { container } = renderSwitcher();
    // Wait for the query to settle, then assert nothing rendered.
    await waitFor(() => expect(mockedFetch).toHaveBeenCalled());
    expect(
      container.querySelector("[data-testid='tenant-switcher']"),
    ).toBeNull();
  });

  it("hides itself on 401", async () => {
    mockedFetch.mockResolvedValue({ kind: "unauthenticated" });
    const { container } = renderSwitcher();
    await waitFor(() => expect(mockedFetch).toHaveBeenCalled());
    expect(
      container.querySelector("[data-testid='tenant-switcher']"),
    ).toBeNull();
  });
});
