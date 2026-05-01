/**
 * Empty data stub for `/skills` page (B2 prototype).
 *
 * TODO(B2-BE1/BE5): swap to `apiFetch<Skill[]>("/admin/skills")` once
 * the gateway exposes the skill catalogue endpoint.
 */

export interface Skill {
  name: string;
  description: string;
  emoji: string;
  allowed_tools: string[];
  requires: string[];
  install: string;
  source_path: string;
  body_markdown: string;
}

export const MOCK_SKILLS: Skill[] = [];
