"use client";

import { useTranslation } from "react-i18next";
import { StatChip } from "@/components/ui/stat-chip";

/**
 * Four-stat row for the Scheduler page: Total · Enabled · Paused · Errored.
 *
 * Spark paths reuse the baked geometry shared with Dashboard/Approvals so
 * the visual dialect stays consistent. When the gateway is offline every
 * value collapses to `—` rather than a fake zero.
 */

const TOTAL_SPARK =
  "M0 28 L30 26 L60 22 L90 24 L120 18 L150 22 L180 14 L210 18 L240 10 L270 14 L300 6 L300 36 L0 36 Z";
const ENABLED_SPARK =
  "M0 22 L30 22 L60 20 L90 22 L120 18 L150 20 L180 18 L210 20 L240 16 L270 18 L300 16 L300 36 L0 36 Z";
const PAUSED_SPARK =
  "M0 18 L30 20 L60 16 L90 22 L120 14 L150 20 L180 18 L210 22 L240 16 L270 20 L300 14 L300 36 L0 36 Z";
const ERRORED_SPARK =
  "M0 10 L30 14 L60 16 L90 20 L120 22 L150 24 L180 26 L210 28 L240 30 L270 30 L300 32 L300 36 L0 36 Z";

export interface SchedulerStatsRowProps {
  total: number;
  enabled: number;
  paused: number;
  errored: number;
  /** Live state reflects whether `/admin/scheduler/jobs` is reachable. */
  live: boolean;
}

export function SchedulerStatsRow({
  total,
  enabled,
  paused,
  errored,
  live,
}: SchedulerStatsRowProps) {
  const { t } = useTranslation();
  const offlineFoot = t("scheduler.tp.statOfflineFoot");

  return (
    <section className="grid grid-cols-1 gap-3.5 md:grid-cols-2 xl:grid-cols-4">
      <StatChip
        variant="primary"
        live={live}
        label={t("scheduler.tp.statTotal")}
        value={live ? total : "—"}
        foot={live ? t("scheduler.tp.statFootTotal") : offlineFoot}
        sparkPath={TOTAL_SPARK}
        sparkTone="amber"
      />
      <StatChip
        label={t("scheduler.tp.statEnabled")}
        value={live ? enabled : "—"}
        delta={
          live && total > 0
            ? {
                label: `${enabled} / ${total}`,
                tone: enabled === total ? "up" : "flat",
              }
            : undefined
        }
        foot={live ? t("scheduler.tp.statFootEnabled") : offlineFoot}
        sparkPath={ENABLED_SPARK}
        sparkTone="ember"
      />
      <StatChip
        label={t("scheduler.tp.statPaused")}
        value={live ? paused : "—"}
        foot={live ? t("scheduler.tp.statFootPaused") : offlineFoot}
        sparkPath={PAUSED_SPARK}
        sparkTone="peach"
      />
      <StatChip
        label={t("scheduler.tp.statErrored")}
        value={live ? errored : "—"}
        delta={
          live
            ? errored === 0
              ? { label: t("scheduler.tp.caughtUp"), tone: "up" }
              : { label: t("scheduler.tp.needsAttention"), tone: "down" }
            : undefined
        }
        foot={live ? t("scheduler.tp.statFootErrored") : offlineFoot}
        sparkPath={ERRORED_SPARK}
        sparkTone="ember"
      />
    </section>
  );
}

export default SchedulerStatsRow;
