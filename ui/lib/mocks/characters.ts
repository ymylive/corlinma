/**
 * Empty data stubs for `/characters` page (B2 prototype). The
 * `fetchAgents()` function still resolves so existing async callers
 * compile, but it returns `[]` — no fake characters paint to a real
 * user.
 *
 * TODO(B2-BE3): swap to `apiFetch<AgentCard[]>("/admin/agents")` when
 * the gateway exposes the real endpoint.
 */

export interface AgentCard {
  name: string;
  description: string;
  emoji: string;
  system_prompt: string;
  variables: Record<string, string>;
  tools_allowed: string[];
  skill_refs: string[];
  source_path: string;
}

export const MOCK_CHARACTERS: AgentCard[] = [];

export async function fetchAgents(): Promise<AgentCard[]> {
  return [];
}
