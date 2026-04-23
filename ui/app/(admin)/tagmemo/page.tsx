/**
 * Tag Memo Dashboard (B5-FE1).
 *
 * Three linked visualisations over the EPA / Residual-Pyramid pipeline:
 *   ① scatter  — first 2 projections, depth by colour, energy by radius.
 *   ② dual-line — entropy + logic_depth, drawn in over 1.2s.
 *   ③ residual pyramid — one row per chunk, pyramid levels as segments.
 *
 * Hovering a mark in any panel lights up the same chunk in the other two
 * (via `HoveredIdProvider`). Mock data today; real endpoint from
 * `corlinman-tagmemo` (B3-BE4) will slot in behind the same component API.
 */
"use client";

import * as React from "react";
import { useTranslation } from "react-i18next";
import { motion } from "framer-motion";

import { AnimatedNumber } from "@/components/ui/animated-number";
import { useMotionVariants } from "@/lib/motion";
import { EpaScatter } from "@/components/viz/epa-scatter";
import { DualLine } from "@/components/viz/dual-line";
import { ResidualPyramid } from "@/components/viz/residual-pyramid";
import { HoveredIdProvider } from "@/components/viz/use-hovered-id";
import {
  generateMockChunks,
  summariseChunks,
  type TagMemoChunk,
} from "@/lib/mocks/tagmemo";

// TODO(B3-BE4): swap mock for apiFetch<TagMemoChunk[]>("/admin/tagmemo/chunks")
export default function TagMemoPage() {
  const { t } = useTranslation();
  const variants = useMotionVariants();

  const chunks = React.useMemo<TagMemoChunk[]>(
    () => generateMockChunks(),
    [],
  );
  const stats = React.useMemo(() => summariseChunks(chunks), [chunks]);

  const [parentWidth, setParentWidth] = React.useState(900);
  const pyramidRef = React.useRef<HTMLDivElement>(null);
  React.useEffect(() => {
    if (!pyramidRef.current) return;
    const el = pyramidRef.current;
    const update = () => setParentWidth(el.getBoundingClientRect().width);
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  return (
    <HoveredIdProvider>
      <div className="flex flex-col gap-5">
        <header className="flex flex-col gap-3">
          <div className="space-y-1">
            <h1 className="text-2xl font-semibold tracking-tight">
              {t("nav.tagmemo")}
            </h1>
            <p className="text-sm text-muted-foreground">
              EPA projections · logic depth · residual pyramid — per-chunk
              telemetry from <code>corlinman-tagmemo</code>.
            </p>
          </div>

          <motion.div
            aria-label="Tag memo stats"
            className="grid grid-cols-2 gap-3 sm:grid-cols-4"
            variants={variants.stagger}
            initial="hidden"
            animate="visible"
          >
            <StatCard
              label="chunks"
              value={stats.chunkCount}
              format="number"
            />
            <StatCard
              label="avg entropy"
              value={stats.avgEntropy}
              format="percent"
            />
            <StatCard
              label="avg logic_depth"
              value={stats.avgLogicDepth}
              format="percent"
            />
            <StatCard
              label="unique axes"
              value={stats.uniqueAxes}
              format="number"
            />
          </motion.div>
        </header>

        <section className="grid grid-cols-1 gap-4 xl:grid-cols-2">
          <Panel title="EPA 3-axis scatter" testid="panel-scatter">
            <EpaScatter chunks={chunks} />
          </Panel>
          <Panel
            title="Entropy · logic_depth"
            testid="panel-dual-line"
          >
            <DualLine chunks={chunks} />
          </Panel>
        </section>

        <section ref={pyramidRef} data-testid="panel-pyramid">
          <Panel title="Residual pyramid" testid="panel-pyramid-inner">
            <ResidualPyramid
              chunks={chunks}
              parentWidth={parentWidth}
            />
          </Panel>
        </section>

        {/* Screen-reader / no-JS fallback. Present in DOM; visually hidden
            for sighted users (the panels above carry the same data). */}
        <details className="sr-only">
          <summary>Data table (for screen readers)</summary>
          <table data-testid="fallback-table">
            <thead>
              <tr>
                <th>chunk_id</th>
                <th>entropy</th>
                <th>logic_depth</th>
                <th>top axis</th>
              </tr>
            </thead>
            <tbody>
              {chunks.map((c) => (
                <tr key={c.chunk_id} data-testid={`fallback-row-${c.chunk_id}`}>
                  <td>{c.chunk_id}</td>
                  <td>{c.entropy.toFixed(3)}</td>
                  <td>{c.logic_depth.toFixed(3)}</td>
                  <td>{c.dominant_axes[0]?.label ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </details>
      </div>
    </HoveredIdProvider>
  );
}

function StatCard({
  label,
  value,
  format,
}: {
  label: string;
  value: number;
  format: "number" | "percent";
}) {
  const variants = useMotionVariants();
  return (
    <motion.div
      variants={variants.listItem}
      className="rounded-md border border-border bg-panel p-3"
    >
      <div className="text-[11px] uppercase tracking-wider text-muted-foreground">
        {label}
      </div>
      <div className="font-mono text-xl font-semibold">
        <AnimatedNumber
          value={value}
          format={format}
          formatOptions={
            format === "percent"
              ? { maximumFractionDigits: 1 }
              : undefined
          }
        />
      </div>
    </motion.div>
  );
}

function Panel({
  title,
  testid,
  children,
}: {
  title: string;
  testid: string;
  children: React.ReactNode;
}) {
  return (
    <div
      className="rounded-lg border border-border bg-panel p-4"
      data-testid={testid}
    >
      <div className="mb-3 text-sm font-medium text-foreground">{title}</div>
      {children}
    </div>
  );
}
