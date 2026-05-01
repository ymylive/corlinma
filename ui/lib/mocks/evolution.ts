/**
 * Empty data stubs for `/evolution` page. All production calls now hit
 * the real `/admin/evolution/*` routes via `apiFetch`. This file kept
 * only so existing importers continue to compile; values are empty so
 * stale dev-time data can never leak to a real user.
 */

import type {
  BudgetSnapshot,
  EvolutionProposal,
  HistoryEntry,
} from "@/lib/api";

export const MOCK_EVOLUTION_BUDGET: BudgetSnapshot = {
  enabled: false,
  window_start_ms: 0,
  window_end_ms: 0,
  weekly_total: { limit: 0, used: 0, remaining: 0 },
  per_kind: [],
};

export const MOCK_EVOLUTION_PENDING: EvolutionProposal[] = [];
export const MOCK_EVOLUTION_APPROVED: EvolutionProposal[] = [];
export const MOCK_EVOLUTION_HISTORY: HistoryEntry[] = [];
