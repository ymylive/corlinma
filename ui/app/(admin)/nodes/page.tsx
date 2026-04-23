"use client";

import * as React from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";

import { AnimatedNumber } from "@/components/ui/animated-number";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { TopologyGraph } from "@/components/nodes/topology-graph";
import { RunnerDetailPanel } from "@/components/nodes/runner-detail-panel";
import {
  fetchRunnersMock,
  summariseRunners,
  type Runner,
} from "@/lib/mocks/nodes";

/**
 * Distributed Nodes page (B4-FE2).
 *
 * Radial topology of WebSocket tool runners connected to the gateway. Real
 * data flows in over `/wstool/runners` + SSE once B4-BE3 lands; for now we
 * render the static mock at a 5-second refetch cadence to exercise the
 * layout-animation paths.
 */
// TODO(B4-BE3): replace with real apiFetch<Runner[]>("/wstool/runners") + SSE.
export default function NodesPage() {
  const { t } = useTranslation();
  const query = useQuery<Runner[]>({
    queryKey: ["nodes"],
    queryFn: fetchRunnersMock,
    refetchInterval: 5_000,
  });

  const runners = React.useMemo(() => query.data ?? [], [query.data]);

  const [selectedId, setSelectedId] = React.useState<string | null>(null);

  const selected = React.useMemo(
    () => runners.find((r) => r.id === selectedId) ?? null,
    [runners, selectedId],
  );

  const stats = React.useMemo(() => summariseRunners(runners), [runners]);

  const onSelect = React.useCallback((runner: Runner | null) => {
    if (runner === null) {
      setSelectedId(null);
      return;
    }
    setSelectedId((prev) => (prev === runner.id ? null : runner.id));
  }, []);

  return (
    <div className="flex flex-col gap-5">
      <header className="flex flex-col gap-3">
        <div className="space-y-1">
          <h1 className="text-2xl font-semibold tracking-tight">
            Distributed Nodes · 节点拓扑
          </h1>
          <p className="text-sm text-muted-foreground">
            WebSocket tool runners orbiting the gateway. Hover or tab to a node
            for details.
          </p>
        </div>
        <dl
          className="grid grid-cols-2 gap-3 sm:grid-cols-4"
          data-testid="nodes-stats"
        >
          <StatCard
            label={t("nodes.connected")}
            value={stats.connected}
            tone="ok"
          />
          <StatCard
            label={t("nodes.disconnected")}
            value={stats.disconnected}
            tone="muted"
          />
          <StatCard
            label={t("nodes.avgLatency")}
            value={stats.avgLatencyMs}
            suffix=" ms"
            tone="neutral"
          />
          <StatCard
            label={t("nodes.tasksPerMin")}
            value={stats.tasksPerMin}
            tone="neutral"
          />
        </dl>
      </header>

      {query.isPending ? (
        <Skeleton className="h-[540px] w-full rounded-lg" />
      ) : query.isError ? (
        <EmptyState
          title={t("common.loadFailed")}
          description={(query.error as Error)?.message ?? t("common.error")}
        />
      ) : runners.length === 0 ? (
        <EmptyState
          title={t("nodes.empty")}
          description={t("nodes.emptyHint")}
        />
      ) : (
        <div className="flex flex-col gap-4 lg:flex-row lg:items-start">
          <div className="mx-auto w-full max-w-[900px] flex-1">
            <TopologyGraph
              runners={runners}
              selectedId={selectedId}
              onSelect={onSelect}
            />
          </div>
          {selected ? (
            <RunnerDetailPanel
              runner={selected}
              onClose={() => setSelectedId(null)}
            />
          ) : null}
        </div>
      )}

      {/* Screen-reader / no-JS fallback: a plain data table summarising every
          runner. Visually hidden via `sr-only`, but present in the DOM so
          assistive tech can enumerate the topology without parsing SVG. */}
      <details className="sr-only">
        <summary>Runner table (accessibility fallback)</summary>
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Ring</th>
              <th>Health</th>
              <th>Latency</th>
              <th>Tools</th>
            </tr>
          </thead>
          <tbody>
            {runners.map((r) => (
              <tr key={r.id} data-testid={`runner-row-${r.id}`}>
                <td>{r.hostname}</td>
                <td>{r.ring === 0 ? "inner" : "outer"}</td>
                <td>{r.health}</td>
                <td>{r.latencyMs}ms</td>
                <td>{r.toolCount}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </div>
  );
}

// ---------- local helpers ----------

interface StatCardProps {
  label: string;
  value: number;
  suffix?: string;
  tone: "ok" | "muted" | "neutral";
}

function StatCard({ label, value, suffix, tone }: StatCardProps) {
  const valueClass =
    tone === "ok"
      ? "text-ok"
      : tone === "muted"
        ? "text-muted-foreground"
        : "text-foreground";
  return (
    <div className="rounded-lg border border-border bg-card/40 px-3 py-2">
      <dt className="text-[10px] uppercase tracking-wider text-muted-foreground">
        {label}
      </dt>
      <dd className={`mt-0.5 text-xl font-semibold tabular-nums ${valueClass}`}>
        <AnimatedNumber value={value} />
        {suffix ?? ""}
      </dd>
    </div>
  );
}
