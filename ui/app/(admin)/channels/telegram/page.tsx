"use client";

import * as React from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, Eye, EyeOff, Send } from "lucide-react";

import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { AnimatedNumber } from "@/components/ui/animated-number";
import { useMotion } from "@/components/ui/motion-safe";
import { ChannelShell } from "@/components/channels/channel-shell";
import {
  fetchTelegramMessages,
  fetchTelegramStatus,
  type TelegramMessage,
  type TelegramStatusResponse,
} from "@/lib/api/telegram";
import { MessageList } from "./MessageList";
import { MediaPreviewDrawer } from "./MediaPreviewDrawer";
import { SendTestDrawer } from "./SendTestDrawer";

/**
 * Telegram channel admin page (B4-FE1).
 *
 * Wires the B3-FE2 skeleton to the real API client (`lib/api/telegram.ts`),
 * which transparently falls back to the B3-FE2 mock when the admin surface
 * isn't deployed yet. Adds:
 *   - `last_error` banner (pulses on reduced-motion off)
 *   - Routing badges + media previews on the message list
 *   - Photo preview drawer (reuses `<Drawer>`)
 *   - Send-test drawer (reuses `<Drawer>`)
 */
export default function TelegramChannelPage() {
  const statusQuery = useQuery<TelegramStatusResponse>({
    queryKey: ["admin", "channels", "telegram", "status"],
    queryFn: fetchTelegramStatus,
    refetchInterval: 3_000,
  });
  const messagesQuery = useQuery<TelegramMessage[]>({
    queryKey: ["admin", "channels", "telegram", "messages"],
    queryFn: () => fetchTelegramMessages({ limit: 20 }),
    refetchInterval: 3_000,
  });

  const [sendOpen, setSendOpen] = React.useState(false);
  const [previewMessage, setPreviewMessage] = React.useState<
    TelegramMessage | null
  >(null);

  const connected = statusQuery.data?.connected ?? false;
  const runtime = statusQuery.data?.runtime ?? "unknown";
  const lastError = statusQuery.data?.last_error ?? null;

  const runtimeChip = (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full border px-2.5 py-0.5 font-mono text-[10px] uppercase tracking-wider",
        connected
          ? "border-ok/40 bg-ok/10 text-ok"
          : "border-err/40 bg-err/10 text-err",
      )}
      data-testid="tg-connection-chip"
    >
      runtime={runtime}
    </span>
  );

  const headerActions = (
    <>
      {runtimeChip}
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={() => setSendOpen(true)}
        data-testid="tg-send-test-open"
      >
        <Send className="h-3.5 w-3.5" aria-hidden="true" />
        Send test message
      </Button>
    </>
  );

  const messages = messagesQuery.data ?? [];

  return (
    <ChannelShell
      channelId="telegram"
      title="Telegram Channel"
      subtitle="Webhook bridge · bot token + secret-token handshake · live dispatch stats."
      connected={connected}
      actions={headerActions}
    >
      {lastError ? <ErrorBanner error={lastError} /> : null}

      {statusQuery.isPending ? (
        <Skeleton className="h-28 w-full" />
      ) : statusQuery.isError ? (
        <p className="text-sm text-destructive">
          Load failed: {(statusQuery.error as Error).message}
        </p>
      ) : statusQuery.data ? (
        <>
          <ConfigCard status={statusQuery.data} />
          <StatsRow
            messagesToday={statusQuery.data.stats.messages_today}
            messagesWeek={statusQuery.data.stats.messages_week}
            p50={statusQuery.data.stats.latency_p50_ms}
            p95={statusQuery.data.stats.latency_p95_ms}
            activeChats={statusQuery.data.stats.active_chats}
          />

          <section className="rounded-lg border border-border bg-panel">
            <div className="border-b border-border px-4 py-3 text-sm font-semibold">
              Recent messages
            </div>
            <div className="max-h-[420px] overflow-auto">
              {messagesQuery.isPending ? (
                <div className="space-y-2 p-4">
                  <Skeleton className="h-12 w-full" />
                  <Skeleton className="h-12 w-full" />
                  <Skeleton className="h-12 w-full" />
                </div>
              ) : messagesQuery.isError ? (
                <p className="px-4 py-8 text-center text-sm text-destructive">
                  Messages load failed:{" "}
                  {(messagesQuery.error as Error).message}
                </p>
              ) : (
                <MessageList
                  messages={messages.slice(0, 10)}
                  onPhotoClick={setPreviewMessage}
                />
              )}
            </div>
          </section>

          <DebugPanel
            payload={statusQuery.data.last_webhook_payload ?? null}
            error={lastError}
          />
        </>
      ) : null}

      <MediaPreviewDrawer
        message={previewMessage}
        mediaBaseUrl=""
        open={previewMessage !== null}
        onOpenChange={(v) => {
          if (!v) setPreviewMessage(null);
        }}
      />
      <SendTestDrawer open={sendOpen} onOpenChange={setSendOpen} />
    </ChannelShell>
  );
}

/* ------------------------------------------------------------------ */
/*                           Error banner                              */
/* ------------------------------------------------------------------ */

function ErrorBanner({ error }: { error: string }) {
  const { reduced } = useMotion();
  return (
    <div
      role="alert"
      data-testid="tg-last-error-banner"
      className={cn(
        "flex items-start gap-2 rounded-lg border border-err/40 bg-state-error/40 px-3 py-2 text-xs text-err",
        !reduced && "animate-pulse-glow",
      )}
    >
      <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
      <div className="min-w-0 flex-1">
        <div className="font-semibold uppercase tracking-wider">
          Last dispatch error
        </div>
        <p className="mt-0.5 whitespace-pre-wrap break-words font-mono text-[11px]">
          {error}
        </p>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*                           Config card                              */
/* ------------------------------------------------------------------ */

function ConfigCard({ status }: { status: TelegramStatusResponse }) {
  const { bot_token, webhook_url, secret_token, drop_pending_updates } =
    status.config;
  const [revealed, setRevealed] = React.useState(false);

  const toggleReveal = React.useCallback(() => {
    setRevealed((v) => !v);
  }, []);

  const onRevealKeyDown = React.useCallback(
    (e: React.KeyboardEvent<HTMLButtonElement>) => {
      if (e.key === " " || e.key === "Enter") {
        e.preventDefault();
        toggleReveal();
      }
    },
    [toggleReveal],
  );

  return (
    <section
      className="space-y-3 rounded-lg border border-border bg-panel p-4"
      aria-label="Telegram bot configuration"
    >
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-sm font-semibold">Bot configuration</h2>
          <p className="text-xs text-muted-foreground">
            Read-only for now. Mutations land with the webhook bridge.
          </p>
        </div>
        <span className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
          read-only
        </span>
      </div>

      <dl className="grid grid-cols-1 gap-3 md:grid-cols-2">
        <ConfigField label="Bot token">
          <div className="flex items-center gap-2">
            <code
              className="flex-1 truncate rounded bg-muted px-2 py-1 font-mono text-[11px]"
              aria-readonly="true"
              data-testid="tg-bot-token"
            >
              {revealed ? bot_token : maskToken(bot_token)}
            </code>
            <button
              type="button"
              onClick={toggleReveal}
              onKeyDown={onRevealKeyDown}
              aria-pressed={revealed}
              aria-label={revealed ? "Hide bot token" : "Reveal bot token"}
              className="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              data-testid="tg-reveal-token"
            >
              {revealed ? (
                <EyeOff className="h-3.5 w-3.5" aria-hidden="true" />
              ) : (
                <Eye className="h-3.5 w-3.5" aria-hidden="true" />
              )}
            </button>
          </div>
        </ConfigField>

        <ConfigField label="Webhook URL">
          <code
            className="block truncate rounded bg-muted px-2 py-1 font-mono text-[11px]"
            aria-readonly="true"
          >
            {webhook_url}
          </code>
        </ConfigField>

        <ConfigField label="Secret token">
          <code
            className="block truncate rounded bg-muted px-2 py-1 font-mono text-[11px]"
            aria-readonly="true"
          >
            {maskToken(secret_token)}
          </code>
        </ConfigField>

        <ConfigField label="Drop pending updates on reconnect">
          <div
            className="flex items-center gap-2"
            role="switch"
            aria-checked={drop_pending_updates}
            aria-readonly="true"
          >
            <span
              className={cn(
                "inline-flex h-4 w-7 items-center rounded-full border border-border p-0.5 transition-colors",
                drop_pending_updates ? "bg-primary/30" : "bg-muted",
              )}
              aria-hidden="true"
            >
              <span
                className={cn(
                  "h-3 w-3 rounded-full bg-background shadow transition-transform",
                  drop_pending_updates ? "translate-x-3" : "translate-x-0",
                )}
              />
            </span>
            <span className="font-mono text-[11px] text-muted-foreground">
              {drop_pending_updates ? "on" : "off"}
            </span>
          </div>
        </ConfigField>
      </dl>
    </section>
  );
}

function ConfigField({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-1">
      <dt className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
        {label}
      </dt>
      <dd>{children}</dd>
    </div>
  );
}

/** Keep only the last 4 chars of a token; mask the rest. */
function maskToken(token: string): string {
  if (!token) return "";
  const tail = token.slice(-4);
  return `••••••••${tail}`;
}

/* ------------------------------------------------------------------ */
/*                             Stats row                              */
/* ------------------------------------------------------------------ */

function StatsRow({
  messagesToday,
  messagesWeek,
  p50,
  p95,
  activeChats,
}: {
  messagesToday: number;
  messagesWeek: number;
  p50: number;
  p95: number;
  activeChats: number;
}) {
  return (
    <section
      className="grid grid-cols-2 gap-3 md:grid-cols-4"
      aria-label="Telegram channel stats"
    >
      <StatCard label="Messages today" value={messagesToday} />
      <StatCard label="Messages · week" value={messagesWeek} />
      <StatCard
        label="Avg latency"
        valueNode={
          <span className="flex items-baseline gap-1">
            <AnimatedNumber value={p50} className="text-2xl font-semibold" />
            <span className="font-mono text-[10px] text-muted-foreground">
              p50ms
            </span>
            <span className="mx-1 text-muted-foreground">·</span>
            <AnimatedNumber
              value={p95}
              className="text-sm font-semibold text-muted-foreground"
            />
            <span className="font-mono text-[10px] text-muted-foreground">
              p95ms
            </span>
          </span>
        }
      />
      <StatCard label="Active chats" value={activeChats} />
    </section>
  );
}

function StatCard({
  label,
  value,
  valueNode,
}: {
  label: string;
  value?: number;
  valueNode?: React.ReactNode;
}) {
  return (
    <div className="rounded-lg border border-border bg-panel p-3">
      <div className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
        {label}
      </div>
      <div className="mt-1">
        {valueNode ?? (
          <AnimatedNumber
            value={value ?? 0}
            className="text-2xl font-semibold tabular-nums"
          />
        )}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*                            Debug panel                             */
/* ------------------------------------------------------------------ */

function DebugPanel({
  payload,
  error,
}: {
  payload: Record<string, unknown> | null;
  error: string | null;
}) {
  return (
    <details className="rounded-lg border border-border bg-panel">
      <summary className="cursor-pointer px-4 py-3 text-sm font-semibold outline-none focus-visible:ring-2 focus-visible:ring-ring">
        Debug
      </summary>
      <div className="space-y-3 border-t border-border p-4">
        <div>
          <div className="mb-1 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
            Last webhook payload
          </div>
          <pre
            className="max-h-[240px] overflow-auto rounded-md bg-muted p-3 font-mono text-[11px] leading-relaxed"
            data-testid="tg-debug-payload"
          >
            {payload ? JSON.stringify(payload, null, 2) : "(none)"}
          </pre>
        </div>
        <div>
          <div className="mb-1 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
            Last dispatch error
          </div>
          {error ? (
            <pre
              className="rounded-md bg-destructive/10 p-3 font-mono text-[11px] text-destructive"
              data-testid="tg-debug-error"
            >
              {error}
            </pre>
          ) : (
            <p className="text-xs text-muted-foreground">No recent errors.</p>
          )}
        </div>
      </div>
    </details>
  );
}

