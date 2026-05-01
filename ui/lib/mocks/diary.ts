/**
 * Empty data stubs for `/diary` page (B5 prototype). Pure date helpers
 * (`dateKey`, `timeLabel`, `groupByDate`) stay since they're not data.
 *
 * TODO(B5): swap to `apiFetch<DiaryEntry[]>("/admin/diary")` once the
 * gateway exposes a real endpoint.
 */

export interface DiaryEntry {
  id: string;
  created_at_ms: number;
  title: string;
  body_markdown: string;
  tags: string[];
  session_key?: string;
}

export const MOCK_DIARY: readonly DiaryEntry[] = Object.freeze([]);

export function dateKey(ms: number): string {
  const d = new Date(ms);
  const y = d.getUTCFullYear();
  const m = (d.getUTCMonth() + 1).toString().padStart(2, "0");
  const day = d.getUTCDate().toString().padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export function timeLabel(ms: number): string {
  const d = new Date(ms);
  const h = d.getUTCHours().toString().padStart(2, "0");
  const m = d.getUTCMinutes().toString().padStart(2, "0");
  return `${h}:${m}`;
}

export interface DiaryDateGroup {
  date: string;
  entries: DiaryEntry[];
}

export function groupByDate(entries: readonly DiaryEntry[]): DiaryDateGroup[] {
  const groups: DiaryDateGroup[] = [];
  let cur: DiaryDateGroup | null = null;
  for (const e of entries) {
    const k = dateKey(e.created_at_ms);
    if (cur === null || cur.date !== k) {
      cur = { date: k, entries: [] };
      groups.push(cur);
    }
    cur.entries.push(e);
  }
  return groups;
}

export async function fetchDiary(): Promise<DiaryEntry[]> {
  return [];
}
