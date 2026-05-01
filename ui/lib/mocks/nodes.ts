/**
 * Empty data stubs for `/nodes` page (B4 prototype).
 *
 * TODO(B4-BE3): swap to `apiFetch<Runner[]>("/wstool/runners")` once
 * the gateway exposes the runner registry + SSE stream.
 */

export type RunnerHealth = "healthy" | "degraded" | "offline";

export interface Runner {
  id: string;
  hostname: string;
  ring: 0 | 1;
  slot: number;
  health: RunnerHealth;
  latencyMs: number;
  toolCount: number;
  connectedForSec: number;
  lastPingMs: number;
  errorRate: number;
  tools: string[];
}

export async function fetchRunnersMock(): Promise<Runner[]> {
  return [];
}

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
  return { connected, disconnected, avgLatencyMs, tasksPerMin: 0 };
}
