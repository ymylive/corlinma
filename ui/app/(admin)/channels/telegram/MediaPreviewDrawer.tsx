"use client";

import * as React from "react";
import { toast } from "sonner";
import { Copy } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Drawer } from "@/components/ui/drawer";
import type { TelegramMessage } from "@/lib/api/telegram";

/**
 * Full-size photo preview for a Telegram message. Reuses the shared
 * `<Drawer>` primitive (B4-FE4) with a `lg` width so the image has room to
 * breathe on typical admin viewports.
 *
 * The metadata rail beneath the image echoes the row context (chat, sender,
 * time) so the user does not have to context-switch back to the list.
 */
export function MediaPreviewDrawer({
  message,
  mediaBaseUrl,
  open,
  onOpenChange,
}: {
  message: TelegramMessage | null;
  mediaBaseUrl: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const src = React.useMemo(() => {
    if (!message?.media) return null;
    return resolveMediaUrl(mediaBaseUrl, message.media.local_path);
  }, [message, mediaBaseUrl]);

  const handleCopyPath = React.useCallback(async () => {
    if (!message?.media) return;
    const path = message.media.local_path;
    try {
      if (typeof navigator !== "undefined" && navigator.clipboard) {
        await navigator.clipboard.writeText(path);
        toast.success("Local path copied");
      } else {
        toast.message(path);
      }
    } catch {
      toast.error("Copy failed");
    }
  }, [message]);

  return (
    <Drawer
      open={open}
      onOpenChange={onOpenChange}
      title="Photo preview"
      width="lg"
      footer={
        message?.media ? (
          <Button
            variant="outline"
            size="sm"
            onClick={handleCopyPath}
            data-testid="tg-media-copy-path"
          >
            <Copy className="h-3.5 w-3.5" aria-hidden="true" />
            Copy local path
          </Button>
        ) : null
      }
    >
      <div className="flex flex-col gap-4 p-5">
        {src ? (
          <div className="flex items-center justify-center rounded-md border border-border bg-muted/30 p-2">
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img
              src={src}
              alt={message?.content ?? "Telegram photo preview"}
              className="max-h-[60vh] w-auto max-w-full rounded"
              data-testid="tg-media-preview-img"
            />
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">No image selected.</p>
        )}

        {message ? (
          <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-2 text-xs">
            <dt className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
              Chat
            </dt>
            <dd className="font-mono">
              {message.chat_title ?? message.chat_id}
            </dd>

            <dt className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
              Sender
            </dt>
            <dd className="font-mono">{message.from_username ?? "—"}</dd>

            <dt className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
              Time
            </dt>
            <dd className="font-mono">{formatTime(message.timestamp_ms)}</dd>

            <dt className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
              Local path
            </dt>
            <dd
              className="break-all font-mono text-[11px]"
              data-testid="tg-media-path"
            >
              {message.media?.local_path ?? "—"}
            </dd>

            {message.content ? (
              <>
                <dt className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
                  Caption
                </dt>
                <dd className="whitespace-pre-wrap break-words">
                  {message.content}
                </dd>
              </>
            ) : null}
          </dl>
        ) : null}
      </div>
    </Drawer>
  );
}

function resolveMediaUrl(baseUrl: string, path: string): string {
  if (!baseUrl) return path;
  if (path.startsWith("http://") || path.startsWith("https://")) return path;
  const sep = baseUrl.endsWith("/") || path.startsWith("/") ? "" : "/";
  return `${baseUrl}${sep}${path}`;
}

function formatTime(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "—";
  const d = new Date(ms);
  return d.toLocaleString();
}

export default MediaPreviewDrawer;
