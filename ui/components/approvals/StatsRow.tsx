"use client";

import { useTranslation } from "react-i18next";
import { StatChip } from "@/components/ui/stat-chip";

/**
 * Four-stat row for the approvals page. The `/admin/approvals/metrics`
 * endpoints don't exist yet — non-live chips surface "—" as value and
 * "endpoint pending" in the footer so the layout is honest about missing
 * data rather than inventing numbers.
 *
 * Spark paths are the same baked geometry used on the Dashboard — keeps
 * the visual dialect consistent across pages without a deps hop.
 */
const PENDING_SPARK =
  "M0 28 L30 26 L60 22 L90 24 L120 18 L150 22 L180 14 L210 18 L240 10 L270 14 L300 6 L300 36 L0 36 Z";
const APPROVED_SPARK =
  "M0 22 L30 22 L60 20 L90 22 L120 18 L150 20 L180 18 L210 20 L240 16 L270 18 L300 16 L300 36 L0 36 Z";
const DENIED_SPARK =
  "M0 10 L30 14 L60 16 L90 20 L120 22 L150 24 L180 26 L210 28 L240 30 L270 30 L300 32 L300 36 L0 36 Z";
const AVG_SPARK =
  "M0 18 L30 20 L60 16 L90 22 L120 14 L150 20 L180 18 L210 22 L240 16 L270 20 L300 14 L300 36 L0 36 Z";

export interface StatsRowProps {
  pendingCount: number;
  /** `true` when the approvals endpoint is reachable. Drives "live" flag. */
  pendingLive: boolean;
}

export function StatsRow({ pendingCount, pendingLive }: StatsRowProps) {
  const { t } = useTranslation();
  const endpointPending = t("approvals.tp.statEndpointPending");

  return (
    <section className="grid grid-cols-1 gap-3.5 md:grid-cols-2 xl:grid-cols-4">
      <StatChip
        variant="primary"
        live={pendingLive}
        label={t("approvals.tp.statPending")}
        value={pendingLive ? pendingCount : "—"}
        foot={
          pendingLive
            ? pendingCount > 0
              ? t("approvals.tp.statPendingFoot")
              : t("approvals.tp.statCaughtUp")
            : endpointPending
        }
        sparkPath={PENDING_SPARK}
        sparkTone="amber"
      />
      <StatChip
        label={t("approvals.tp.statApproved24h")}
        value="—"
        foot={endpointPending}
        sparkPath={APPROVED_SPARK}
        sparkTone="peach"
      />
      <StatChip
        label={t("approvals.tp.statDenied24h")}
        value="—"
        foot={endpointPending}
        sparkPath={DENIED_SPARK}
        sparkTone="ember"
      />
      <StatChip
        label={t("approvals.tp.statAvgDecide")}
        value="—"
        foot={endpointPending}
        sparkPath={AVG_SPARK}
        sparkTone="amber"
      />
    </section>
  );
}
