/**
 * Phase 3 W1-C — page-level wiring of the budget endpoint.
 *
 * Covers two scenarios:
 *   1. Happy path: stubbed `fetchBudget` resolves with a known payload, the
 *      page surfaces the same `used` / `total` on its `BudgetGauge` (rendered
 *      twice — once in PageHeader, once in StatsRow via numeric copy).
 *   2. Failure path: `fetchBudget` rejects, the page falls back to 0/0
 *      gracefully — the gauge stays mounted, no error boundary fires.
 *
 * Implementation notes:
 *   - We hoist `vi.mock("@/lib/api", ...)` for `fetchBudget` and
 *     `fetchEvolutionPending` so the page never tries the real fetch.
 *   - `BudgetGauge` exposes `role="meter"` with `aria-valuenow` /
 *     `aria-valuemax`, which is the cleanest assertion surface.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import * as React from "react";

import type { BudgetSnapshot, EvolutionProposal } from "@/lib/api";

const fetchBudgetMock = vi.fn<() => Promise<BudgetSnapshot>>();
const fetchEvolutionPendingMock = vi.fn<() => Promise<EvolutionProposal[]>>();

vi.mock("@/lib/api", async () => {
  const actual =
    await vi.importActual<typeof import("@/lib/api")>("@/lib/api");
  return {
    ...actual,
    fetchBudget: () => fetchBudgetMock(),
    fetchEvolutionPending: () => fetchEvolutionPendingMock(),
    approveEvolutionProposal: vi.fn(),
    denyEvolutionProposal: vi.fn(),
  };
});

import EvolutionPage from "@/app/(admin)/evolution/page";

function renderWithClient(ui: React.ReactElement) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

describe("EvolutionPage budget wiring", () => {
  beforeEach(() => {
    fetchBudgetMock.mockReset();
    fetchEvolutionPendingMock.mockReset();
    fetchEvolutionPendingMock.mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
  });

  it("renders BudgetGauge with values from fetchBudget", async () => {
    fetchBudgetMock.mockResolvedValue({
      enabled: true,
      window_start_ms: 0,
      window_end_ms: 0,
      weekly_total: { limit: 12, used: 7, remaining: 5 },
      per_kind: [],
    });

    renderWithClient(<EvolutionPage />);

    // Wait for the budget query to settle and the gauge to reflect the limit.
    await waitFor(() => {
      const meters = screen.getAllByRole("meter");
      expect(meters.length).toBeGreaterThan(0);
      // First meter belongs to the PageHeader BudgetPill.
      expect(meters[0]).toHaveAttribute("aria-valuemax", "12");
      expect(meters[0]).toHaveAttribute("aria-valuenow", "7");
    });
  });

  it("falls back to 0/0 and keeps the gauge mounted when fetchBudget rejects", async () => {
    fetchBudgetMock.mockRejectedValue(new Error("budget endpoint offline"));

    renderWithClient(<EvolutionPage />);

    // The gauge stays mounted (still in the DOM) and reports the 0/0
    // fallback. We poll briefly to let the query failure propagate.
    await waitFor(() => {
      const meters = screen.getAllByRole("meter");
      expect(meters.length).toBeGreaterThan(0);
      // Fallback: total → 0 → BudgetGauge clamps aria-valuemax to 0,
      // aria-valuenow to min(used=0, total=0) = 0.
      expect(meters[0]).toHaveAttribute("aria-valuemax", "0");
      expect(meters[0]).toHaveAttribute("aria-valuenow", "0");
    });
  });
});
