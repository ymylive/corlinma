"use client";

import { useTranslation } from "react-i18next";
import { Maximize2 } from "lucide-react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { JsonView } from "@/components/ui/json-view";
import { cn } from "@/lib/utils";
import type { Approval } from "./types";

/**
 * Full-args Dialog — secondary affordance on the approval card. The primary
 * path to inspect arguments is the right-side detail drawer; this dialog is
 * kept for mobile / modal-style inspection.
 *
 * Tidepool (Phase 5a) refresh: swaps the hand-rolled `<pre>` for the shared
 * `<JsonView>` primitive (syntax-highlighted) and wraps content in the
 * glass card aesthetic.
 */
function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}

export function ArgsDialog({ approval }: { approval: Approval }) {
  const { t } = useTranslation();
  return (
    <Dialog>
      <DialogTrigger asChild>
        <button
          type="button"
          aria-label={t("approvals.viewArgs")}
          className={cn(
            "inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-[11px]",
            "border-tp-glass-edge bg-tp-glass-inner text-tp-ink-3",
            "hover:bg-tp-glass-inner-hover hover:text-tp-ink-2",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-tp-amber/40",
          )}
        >
          <Maximize2 className="h-3 w-3" aria-hidden />
          {t("approvals.viewArgs")}
        </button>
      </DialogTrigger>
      <DialogContent
        className={cn(
          "max-w-2xl rounded-2xl border-tp-glass-edge bg-tp-glass-2 p-6",
          "backdrop-blur-glass-strong backdrop-saturate-glass-strong",
          "shadow-tp-hero",
        )}
      >
        <DialogHeader>
          <DialogTitle className="font-mono text-[15px] font-medium text-tp-ink">
            <span className="text-tp-amber">{approval.plugin}</span>
            <span className="text-tp-ink-3">.</span>
            {approval.tool}
          </DialogTitle>
          <DialogDescription asChild>
            <div className="space-y-1 text-[12px] text-tp-ink-3">
              <div>
                {t("approvals.argsSessionKey")}:{" "}
                <span className="font-mono text-tp-ink-2">
                  {approval.session_key || t("approvals.emptyValue")}
                </span>
              </div>
              <div>
                {t("approvals.argsRequestedAt")}:{" "}
                <span className="font-mono text-tp-ink-2">
                  {formatTime(approval.requested_at)}
                </span>
              </div>
              {approval.decided_at ? (
                <div>
                  {t("approvals.argsDecidedAt")}:{" "}
                  <span className="font-mono text-tp-ink-2">
                    {formatTime(approval.decided_at)}
                  </span>{" "}
                  <span className="text-tp-ink-3">— {approval.decision ?? "?"}</span>
                </div>
              ) : null}
            </div>
          </DialogDescription>
        </DialogHeader>
        <JsonView raw={prettifyJson(approval.args_json)} className="max-h-96" />
      </DialogContent>
    </Dialog>
  );
}

function prettifyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}
