/**
 * Mock runner topology for B4-FE2 (Distributed Nodes page).
 *
 * Shape approximates the upcoming `GET /wstool/runners` payload shipped by
 * B4-BE3 (WebSocket tool runner registry). The page queries this stub via
 * `fetchRunnersMock()` until the real endpoint is wired in.
 *
 * TODO(B4-BE3): swap to `apiFetch<Runner[]>("/wstool/runners")` with an SSE
 * stream for live updates.
 */

export type RunnerHealth = "healthy" | "degraded" | "offline";

export interface Runner {
  /** Stable runner id (uuid-ish). */
  id: string;
  /** Operator-friendly hostname. */
  hostname: string;
  /** Orbit ring index — 0 = inner (closer to gateway), 1 = outer. */
  ring: 0 | 1;
  /** 0-based slot within the ring (ring 0: 0..5, ring 1: 0..11). */
  slot: number;
  /** Current health classification. */
  health: RunnerHealth;
  /** Round-trip latency to gateway in milliseconds. */
  latencyMs: number;
  /** Number of tools this runner advertises. */
  toolCount: number;
  /** Seconds since the WebSocket connection was established. */
  connectedForSec: number;
  /** Milliseconds since the last heartbeat ping. */
  lastPingMs: number;
  /** Fraction of tool invocations that errored in the last minute. */
  errorRate: number;
  /** Sample advertised tool names for the detail panel. */
  tools: string[];
}

const TOOL_POOL = [
  "web_search.query",
  "web_search.fetch_page",
  "browser.navigate",
  "browser.click",
  "file_ops.read",
  "file_ops.write",
  "canvas.create",
  "canvas.patch",
  "gh_issues.list",
  "memory.recall",
  "memory.store",
  "gemini.chat",
  "discord.send_message",
  "bear_notes.search",
] as const;

function pickTools(seed: number, n: number): string[] {
  const out: string[] = [];
  for (let i = 0; i < n; i += 1) {
    out.push(TOOL_POOL[(seed + i * 7) % TOOL_POOL.length]!);
  }
  return out;
}

/**
 * 18 runners: 6 on the inner ring (mostly healthy, close to gateway), 12 on
 * the outer ring (mixed health, farther from gateway).
 */
const MOCK_RUNNERS: Runner[] = [
  // --- inner ring (6 slots, healthy by default) ---
  {
    id: "rnr_a1b2c3d4e5f6",
    hostname: "runner-tokyo-01",
    ring: 0,
    slot: 0,
    health: "healthy",
    latencyMs: 32,
    toolCount: 8,
    connectedForSec: 3_612,
    lastPingMs: 420,
    errorRate: 0.0,
    tools: pickTools(1, 8),
  },
  {
    id: "rnr_b2c3d4e5f6a7",
    hostname: "runner-osaka-01",
    ring: 0,
    slot: 1,
    health: "healthy",
    latencyMs: 48,
    toolCount: 6,
    connectedForSec: 7_204,
    lastPingMs: 612,
    errorRate: 0.001,
    tools: pickTools(2, 6),
  },
  {
    id: "rnr_c3d4e5f6a7b8",
    hostname: "runner-sf-01",
    ring: 0,
    slot: 2,
    health: "healthy",
    latencyMs: 71,
    toolCount: 10,
    connectedForSec: 14_812,
    lastPingMs: 330,
    errorRate: 0.0,
    tools: pickTools(3, 10),
  },
  {
    id: "rnr_d4e5f6a7b8c9",
    hostname: "runner-lax-01",
    ring: 0,
    slot: 3,
    health: "healthy",
    latencyMs: 54,
    toolCount: 7,
    connectedForSec: 2_401,
    lastPingMs: 560,
    errorRate: 0.0,
    tools: pickTools(4, 7),
  },
  {
    id: "rnr_e5f6a7b8c9d0",
    hostname: "runner-fra-01",
    ring: 0,
    slot: 4,
    health: "degraded",
    latencyMs: 612,
    toolCount: 9,
    connectedForSec: 902,
    lastPingMs: 1_240,
    errorRate: 0.014,
    tools: pickTools(5, 9),
  },
  {
    id: "rnr_f6a7b8c9d0e1",
    hostname: "runner-sin-01",
    ring: 0,
    slot: 5,
    health: "healthy",
    latencyMs: 88,
    toolCount: 5,
    connectedForSec: 18_003,
    lastPingMs: 450,
    errorRate: 0.0,
    tools: pickTools(6, 5),
  },

  // --- outer ring (12 slots, mixed health) ---
  {
    id: "rnr_11223344aabb",
    hostname: "runner-nyc-02",
    ring: 1,
    slot: 0,
    health: "healthy",
    latencyMs: 92,
    toolCount: 4,
    connectedForSec: 1_211,
    lastPingMs: 680,
    errorRate: 0.0,
    tools: pickTools(7, 4),
  },
  {
    id: "rnr_22334455bbcc",
    hostname: "runner-sea-02",
    ring: 1,
    slot: 1,
    health: "degraded",
    latencyMs: 540,
    toolCount: 6,
    connectedForSec: 5_502,
    lastPingMs: 2_010,
    errorRate: 0.021,
    tools: pickTools(8, 6),
  },
  {
    id: "rnr_33445566ccdd",
    hostname: "runner-hkg-02",
    ring: 1,
    slot: 2,
    health: "healthy",
    latencyMs: 76,
    toolCount: 7,
    connectedForSec: 9_014,
    lastPingMs: 410,
    errorRate: 0.0,
    tools: pickTools(9, 7),
  },
  {
    id: "rnr_44556677ddee",
    hostname: "runner-syd-02",
    ring: 1,
    slot: 3,
    health: "offline",
    latencyMs: 0,
    toolCount: 3,
    connectedForSec: 0,
    lastPingMs: 14_200,
    errorRate: 0.0,
    tools: pickTools(10, 3),
  },
  {
    id: "rnr_55667788eeff",
    hostname: "runner-cdg-02",
    ring: 1,
    slot: 4,
    health: "healthy",
    latencyMs: 64,
    toolCount: 8,
    connectedForSec: 11_102,
    lastPingMs: 520,
    errorRate: 0.0,
    tools: pickTools(11, 8),
  },
  {
    id: "rnr_66778899ff00",
    hostname: "runner-yyz-02",
    ring: 1,
    slot: 5,
    health: "degraded",
    latencyMs: 780,
    toolCount: 5,
    connectedForSec: 3_320,
    lastPingMs: 2_800,
    errorRate: 0.033,
    tools: pickTools(12, 5),
  },
  {
    id: "rnr_778899aa1122",
    hostname: "runner-gru-02",
    ring: 1,
    slot: 6,
    health: "healthy",
    latencyMs: 145,
    toolCount: 4,
    connectedForSec: 602,
    lastPingMs: 770,
    errorRate: 0.002,
    tools: pickTools(13, 4),
  },
  {
    id: "rnr_8899aabb2233",
    hostname: "runner-ams-02",
    ring: 1,
    slot: 7,
    health: "healthy",
    latencyMs: 58,
    toolCount: 6,
    connectedForSec: 22_410,
    lastPingMs: 390,
    errorRate: 0.0,
    tools: pickTools(14, 6),
  },
  {
    id: "rnr_99aabbcc3344",
    hostname: "runner-bom-02",
    ring: 1,
    slot: 8,
    health: "offline",
    latencyMs: 0,
    toolCount: 2,
    connectedForSec: 0,
    lastPingMs: 22_000,
    errorRate: 0.0,
    tools: pickTools(15, 2),
  },
  {
    id: "rnr_aabbccdd4455",
    hostname: "runner-icn-02",
    ring: 1,
    slot: 9,
    health: "healthy",
    latencyMs: 81,
    toolCount: 7,
    connectedForSec: 4_014,
    lastPingMs: 540,
    errorRate: 0.001,
    tools: pickTools(16, 7),
  },
  {
    id: "rnr_bbccddee5566",
    hostname: "runner-mex-02",
    ring: 1,
    slot: 10,
    health: "degraded",
    latencyMs: 620,
    toolCount: 4,
    connectedForSec: 1_502,
    lastPingMs: 1_800,
    errorRate: 0.018,
    tools: pickTools(17, 4),
  },
  {
    id: "rnr_ccddeeff6677",
    hostname: "runner-zrh-02",
    ring: 1,
    slot: 11,
    health: "healthy",
    latencyMs: 52,
    toolCount: 9,
    connectedForSec: 30_501,
    lastPingMs: 310,
    errorRate: 0.0,
    tools: pickTools(18, 9),
  },
];

/**
 * Returns the full runner list. Wrapped in a `Promise` so the page's
 * `useQuery` call matches the shape the real fetch will have.
 */
export async function fetchRunnersMock(): Promise<Runner[]> {
  return MOCK_RUNNERS;
}

/** Aggregate stats surfaced in the page header. */
export function summariseRunners(runners: Runner[]): {
  connected: number;
  disconnected: number;
  avgLatencyMs: number;
  tasksPerMin: number;
} {
  let connected = 0;
  let disconnected = 0;
  let latencySum = 0;
  let latencyCount = 0;
  for (const r of runners) {
    if (r.health === "offline") {
      disconnected += 1;
    } else {
      connected += 1;
      latencySum += r.latencyMs;
      latencyCount += 1;
    }
  }
  const avgLatencyMs =
    latencyCount === 0 ? 0 : Math.round(latencySum / latencyCount);
  // Deterministic mock: roughly 4 tasks/min per healthy runner.
  const tasksPerMin = connected * 4;
  return { connected, disconnected, avgLatencyMs, tasksPerMin };
}
