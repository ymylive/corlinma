"use client";

import * as React from "react";
import { useTranslation } from "react-i18next";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

/**
 * QQ scan-login dialog.
 *
 * Embeds NapCat's own WebUI — reverse-proxied same-origin at `/webui` —
 * in an iframe. NapCat's native WebUI owns the QR lifecycle: it refreshes
 * the code live over its own websocket and reports login state itself,
 * which stays reliable across NapCat releases.
 *
 * This replaces the former relay flow (`POST /admin/channels/qq/qrcode`
 * snapshot + 2s status poll). That relay raced NapCat's ~120s QR rotation
 * and silently served stale codes, so scans never landed — it was dropped
 * in favour of embedding NapCat's first-party UI directly.
 *
 * `/webui` is expected to resolve to the NapCat WebUI; the deployment's
 * reverse proxy is responsible for injecting the WebUI access token.
 * Nothing here is NapCat-version specific — NapCat drives its own login.
 */
export function ScanLoginDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl border-tp-glass-edge bg-tp-glass-2 backdrop-blur-glass-strong backdrop-saturate-glass-strong">
        <DialogHeader>
          <DialogTitle className="text-tp-ink">
            {t("channels.qq.scanLogin.title")}
          </DialogTitle>
          <DialogDescription className="text-tp-ink-3">
            {t("channels.qq.scanLogin.subtitle")}
          </DialogDescription>
        </DialogHeader>

        {open ? (
          <iframe
            data-testid="qq-napcat-webui"
            src="/webui"
            title="NapCat WebUI"
            className="h-[620px] w-full rounded-xl border border-tp-glass-edge bg-white"
          />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
