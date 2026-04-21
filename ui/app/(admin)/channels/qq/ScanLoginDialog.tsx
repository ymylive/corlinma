"use client";

import * as React from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import {
  fetchQqAccounts,
  fetchQqQrcodeStatus,
  qqQuickLogin,
  requestQqQrcode,
  type QqAccount,
  type QqQrcode,
  type QqQrcodeStatus,
} from "@/lib/api";

const POLL_INTERVAL_MS = 2_000;

/**
 * QQ scan-login dialog.
 *
 * Flow:
 *   1. Open → POST /admin/channels/qq/qrcode → render image (base64 or URL).
 *   2. Every 2s GET /admin/channels/qq/qrcode/status → update status line.
 *   3. On `confirmed` → show avatar/nick, invalidate ["admin","channels","qq"],
 *      auto-close after 1.5s.
 *   4. Previously-used accounts render beneath the QR; tap → /quick-login.
 *
 * We keep all poll state in this component so the parent page can stay
 * focused on the connection + keyword editor.
 */
export function ScanLoginDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [qr, setQr] = React.useState<QqQrcode | null>(null);
  const [qrError, setQrError] = React.useState<string | null>(null);
  const [status, setStatus] = React.useState<QqQrcodeStatus>({
    status: "waiting",
  });
  const [now, setNow] = React.useState(() => Date.now());

  // Stable ref so the poll loop below can read the current token without
  // becoming its own dep (avoids restart on every token swap).
  const tokenRef = React.useRef<string | null>(null);

  // Reset + request fresh QR each time the dialog opens.
  React.useEffect(() => {
    if (!open) {
      setQr(null);
      setQrError(null);
      setStatus({ status: "waiting" });
      tokenRef.current = null;
      return;
    }
    let cancelled = false;
    setQrError(null);
    setStatus({ status: "waiting" });
    requestQqQrcode()
      .then((res) => {
        if (cancelled) return;
        setQr(res);
        tokenRef.current = res.token;
      })
      .catch((err) => {
        if (cancelled) return;
        setQrError(err instanceof Error ? err.message : String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

  // Countdown ticker — 1 Hz, cheap.
  React.useEffect(() => {
    if (!open || !qr) return;
    const id = setInterval(() => setNow(Date.now()), 1_000);
    return () => clearInterval(id);
  }, [open, qr]);

  // Status polling — 2s while waiting; stops on terminal status.
  React.useEffect(() => {
    if (!open || !qr) return;
    if (status.status === "confirmed" || status.status === "error") return;
    const id = setInterval(async () => {
      const tok = tokenRef.current;
      if (!tok) return;
      try {
        const next = await fetchQqQrcodeStatus(tok);
        setStatus(next);
        if (next.status === "confirmed") {
          qc.invalidateQueries({ queryKey: ["admin", "channels", "qq"] });
          qc.invalidateQueries({ queryKey: ["admin", "channels", "qq", "accounts"] });
          toast.success(t("channels.qq.scanLogin.confirmed"));
          // Give the user a beat to see the avatar, then close.
          setTimeout(() => onOpenChange(false), 1_500);
        }
      } catch (err) {
        setStatus({
          status: "error",
          message: err instanceof Error ? err.message : String(err),
        });
      }
    }, POLL_INTERVAL_MS);
    return () => clearInterval(id);
  }, [open, qr, status.status, qc, t, onOpenChange]);

  const secondsLeft = qr
    ? Math.max(0, Math.ceil((qr.expires_at - now) / 1_000))
    : 0;
  const expired = qr ? secondsLeft <= 0 || status.status === "expired" : false;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{t("channels.qq.scanLogin.title")}</DialogTitle>
          <DialogDescription>
            {t("channels.qq.scanLogin.subtitle")}
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col items-center gap-3">
          {qrError ? (
            <div
              className="rounded-md border border-destructive/40 bg-destructive/10 p-4 text-center text-sm text-destructive"
              data-testid="qq-login-error"
            >
              {qrError}
            </div>
          ) : !qr ? (
            <Skeleton className="h-56 w-56 rounded-md" />
          ) : status.status === "confirmed" && status.account ? (
            <AccountCard account={status.account} />
          ) : (
            <QrImage qr={qr} expired={expired} />
          )}

          <div
            className="text-sm"
            data-testid="qq-login-status"
            aria-live="polite"
          >
            <StatusLine status={status.status} secondsLeft={secondsLeft} />
          </div>
        </div>

        <QuickLoginList
          disabled={status.status === "confirmed"}
          onSelect={(uin) => {
            qqQuickLogin(uin)
              .then((res) => {
                setStatus(res);
                if (res.status === "confirmed") {
                  qc.invalidateQueries({
                    queryKey: ["admin", "channels", "qq"],
                  });
                  qc.invalidateQueries({
                    queryKey: ["admin", "channels", "qq", "accounts"],
                  });
                  toast.success(t("channels.qq.scanLogin.confirmed"));
                  setTimeout(() => onOpenChange(false), 1_500);
                } else {
                  toast.error(
                    res.message ?? t("channels.qq.scanLogin.quickLoginFailed"),
                  );
                }
              })
              .catch((err) =>
                toast.error(err instanceof Error ? err.message : String(err)),
              );
          }}
        />
      </DialogContent>
    </Dialog>
  );
}

function QrImage({ qr, expired }: { qr: QqQrcode; expired: boolean }) {
  // Two cases: NapCat returned a base64 PNG (preferred) or a URL. For the
  // URL case we don't bundle a client-side QR generator (no new deps), so
  // we render a short instruction block — the user can still copy/paste the
  // URL into their phone's QQ app. This is a known trade-off documented in
  // the release notes. When NapCat ships base64 (v2.x default) the image
  // works directly.
  if (qr.image_base64) {
    return (
      <div className="relative h-56 w-56 overflow-hidden rounded-md bg-white">
        {/* Base64 data URL — next/image would require remote loader config
            for data: URLs and offers no real perf win for a one-shot QR. */}
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img
          data-testid="qq-qrcode"
          src={`data:image/png;base64,${qr.image_base64}`}
          alt="QQ scan-login QR code"
          className="h-full w-full object-contain"
        />
        {expired ? (
          <div className="absolute inset-0 flex items-center justify-center bg-black/60 text-xs text-white">
            expired
          </div>
        ) : null}
      </div>
    );
  }
  if (qr.qrcode_url) {
    return (
      <div
        data-testid="qq-qrcode"
        className="flex h-56 w-56 flex-col items-center justify-center gap-2 rounded-md border border-dashed border-border bg-muted p-3 text-center text-xs"
      >
        <span className="text-muted-foreground">
          QR URL (copy into QQ mobile):
        </span>
        <code className="break-all px-2 font-mono text-[10px]">
          {qr.qrcode_url}
        </code>
      </div>
    );
  }
  return null;
}

function AccountCard({ account }: { account: QqAccount }) {
  return (
    <div
      className="flex flex-col items-center gap-2 rounded-md border border-ok/40 bg-ok/10 p-4 text-center"
      data-testid="qq-login-account"
    >
      {account.avatar_url ? (
        // eslint-disable-next-line @next/next/no-img-element
        <img
          src={account.avatar_url}
          alt=""
          className="h-16 w-16 rounded-full"
        />
      ) : (
        <div className="flex h-16 w-16 items-center justify-center rounded-full bg-ok/20 font-mono text-lg text-ok">
          {account.uin.slice(0, 2)}
        </div>
      )}
      <div className="text-sm font-semibold">{account.nickname ?? account.uin}</div>
      <div className="font-mono text-[11px] text-muted-foreground">
        QQ: {account.uin}
      </div>
    </div>
  );
}

function StatusLine({
  status,
  secondsLeft,
}: {
  status: QqQrcodeStatus["status"];
  secondsLeft: number;
}) {
  const { t } = useTranslation();
  switch (status) {
    case "waiting":
      return (
        <span className="text-muted-foreground">
          {t("channels.qq.scanLogin.statusWaiting")}{" "}
          {secondsLeft > 0
            ? `(${t("channels.qq.scanLogin.secondsLeft", { s: secondsLeft })})`
            : null}
        </span>
      );
    case "scanned":
      return (
        <span className="text-primary">
          {t("channels.qq.scanLogin.statusScanned")}
        </span>
      );
    case "confirmed":
      return (
        <span className="text-ok">
          {t("channels.qq.scanLogin.statusConfirmed")}
        </span>
      );
    case "expired":
      return (
        <span className="text-warn">
          {t("channels.qq.scanLogin.statusExpired")}
        </span>
      );
    case "error":
      return (
        <span className="text-destructive">
          {t("channels.qq.scanLogin.statusError")}
        </span>
      );
    default:
      return null;
  }
}

function QuickLoginList({
  onSelect,
  disabled,
}: {
  onSelect: (uin: string) => void;
  disabled: boolean;
}) {
  const { t } = useTranslation();
  const accounts = useQuery({
    queryKey: ["admin", "channels", "qq", "accounts"],
    queryFn: fetchQqAccounts,
  });
  const list = accounts.data?.accounts ?? [];
  if (accounts.isPending) {
    return <Skeleton className="h-12 w-full" />;
  }
  if (list.length === 0) {
    return null;
  }
  return (
    <section className="mt-2 border-t border-border pt-3">
      <h3 className="mb-2 text-xs font-semibold text-muted-foreground">
        {t("channels.qq.scanLogin.quickLogin")}
      </h3>
      <ul className="flex flex-wrap gap-2">
        {list.map((a) => (
          <li key={a.uin}>
            <button
              type="button"
              disabled={disabled}
              onClick={() => onSelect(a.uin)}
              data-testid={`qq-quick-login-${a.uin}`}
              className="inline-flex items-center gap-2 rounded-md border border-border bg-surface px-2 py-1 text-xs hover:bg-accent/40 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {a.avatar_url ? (
                // eslint-disable-next-line @next/next/no-img-element
                <img
                  src={a.avatar_url}
                  alt=""
                  className="h-5 w-5 rounded-full"
                />
              ) : null}
              <span className="font-medium">{a.nickname ?? a.uin}</span>
              <span className="font-mono text-[10px] text-muted-foreground">
                {a.uin}
              </span>
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
