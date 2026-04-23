"use client";

import * as React from "react";
import { motion } from "framer-motion";
import {
  AtSign,
  EyeOff,
  FileText,
  MessageCircle,
  MessageSquare,
  Mic,
  Reply,
  Users,
} from "lucide-react";

import { cn } from "@/lib/utils";
import { useMotionVariants } from "@/lib/motion";
import { CountdownRing } from "@/components/ui/countdown-ring";
import type { TelegramMessage } from "@/lib/api/telegram";

/**
 * Recent-message list for the Telegram channel page.
 *
 * Each row shows:
 *   - A chat-kind glyph (DM vs group, preserved from B3-FE2).
 *   - A routing badge indicating how the gateway routed the update —
 *     private DM / group-mention / group-reply-to-bot / group-ignored.
 *     Ignored rows dim to 60% opacity to make the scan fast.
 *   - The message body, with an optional media affordance:
 *       * photo    → 64×64 thumbnail (click → `onPhotoClick`).
 *       * voice    → mic icon + duration + inline `<audio controls>`.
 *       * document → file icon + filename + human-readable size.
 *   - A `<CountdownRing/>` when a reply is still in flight.
 */
export function MessageList({
  messages,
  onPhotoClick,
  mediaBaseUrl = "",
}: {
  messages: TelegramMessage[];
  onPhotoClick?: (msg: TelegramMessage) => void;
  /** Prefix prepended to `media.local_path` when rendering thumbnails. */
  mediaBaseUrl?: string;
}) {
  const variants = useMotionVariants();

  if (messages.length === 0) {
    return (
      <p className="px-4 py-8 text-center text-sm text-muted-foreground">
        No messages yet.
      </p>
    );
  }

  return (
    <motion.ul
      initial="hidden"
      animate="visible"
      variants={variants.stagger}
      className="space-y-2 p-4"
    >
      {messages.map((msg) => {
        const ignored =
          msg.kind === "group" &&
          (msg.routing === "ignored" || msg.mention_reason === "none");
        return (
          <motion.li
            key={msg.id}
            variants={variants.listItem}
            className={cn(
              "flex items-start gap-3 rounded-md border border-border bg-surface p-3 transition-opacity",
              ignored && "opacity-60",
            )}
            data-testid={`tg-message-${msg.id}`}
          >
            <KindIcon kind={msg.kind} />
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2 text-xs">
                {msg.from_username ? (
                  <span className="font-mono font-semibold">
                    {msg.from_username}
                  </span>
                ) : null}
                {msg.chat_title ? (
                  <span className="text-muted-foreground">
                    (group: <span className="font-mono">{msg.chat_title}</span>)
                  </span>
                ) : (
                  <span className="text-muted-foreground">(DM)</span>
                )}
                <span className="text-muted-foreground">
                  • {formatTs(msg.timestamp_ms)}
                </span>
                <RoutingBadge msg={msg} />
              </div>
              {msg.content ? (
                <p className="mt-1 whitespace-pre-wrap break-words text-xs">
                  {msg.content}
                </p>
              ) : null}
              {msg.media ? (
                <MediaPreview
                  message={msg}
                  baseUrl={mediaBaseUrl}
                  onPhotoClick={onPhotoClick}
                />
              ) : null}
            </div>
            {msg.reply_deadline_ms && msg.reply_total_ms ? (
              <CountdownRing
                size={14}
                strokeWidth={2}
                remainingMs={msg.reply_deadline_ms}
                totalMs={msg.reply_total_ms}
                label={`reply due in ${Math.ceil(msg.reply_deadline_ms / 1000)}s`}
                className="shrink-0"
                data-testid={`tg-reply-ring-${msg.id}`}
              />
            ) : null}
          </motion.li>
        );
      })}
    </motion.ul>
  );
}

/* ------------------------------------------------------------------ */
/*                          Kind / routing chrome                      */
/* ------------------------------------------------------------------ */

function KindIcon({ kind }: { kind: TelegramMessage["kind"] }) {
  const isGroup = kind === "group";
  const Icon = isGroup ? Users : MessageSquare;
  const label = isGroup ? "group" : "direct message";
  return (
    <div
      className={cn(
        "flex h-7 w-7 shrink-0 items-center justify-center rounded-full",
        isGroup
          ? "bg-primary/15 text-primary"
          : "bg-accent/50 text-accent-foreground",
      )}
      aria-label={label}
      title={label}
    >
      <Icon className="h-3.5 w-3.5" aria-hidden="true" />
    </div>
  );
}

interface BadgeSpec {
  Icon: React.ComponentType<{ className?: string }>;
  label: string;
  className: string;
  testId: string;
}

function routingSpec(msg: TelegramMessage): BadgeSpec {
  if (msg.kind === "private") {
    return {
      Icon: MessageCircle,
      label: "Private DM",
      className: "border-accent/40 bg-accent/10 text-accent-foreground",
      testId: "route-private",
    };
  }
  if (msg.mention_reason === "mention") {
    return {
      Icon: AtSign,
      label: "Group · @mention",
      className: "border-ok/40 bg-ok/10 text-ok",
      testId: "route-mention",
    };
  }
  if (msg.mention_reason === "reply_to_bot") {
    return {
      Icon: Reply,
      label: "Group · reply-to-bot",
      className: "border-ok/40 bg-ok/10 text-ok",
      testId: "route-reply",
    };
  }
  return {
    Icon: EyeOff,
    label: "Group · ignored",
    className: "border-border bg-muted/50 text-muted-foreground",
    testId: "route-ignored",
  };
}

function RoutingBadge({ msg }: { msg: TelegramMessage }) {
  const spec = routingSpec(msg);
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider",
        spec.className,
      )}
      data-testid={`tg-${spec.testId}-${msg.id}`}
      title={spec.label}
    >
      <spec.Icon className="h-3 w-3" aria-hidden="true" />
      {spec.label}
    </span>
  );
}

/* ------------------------------------------------------------------ */
/*                           Media preview                             */
/* ------------------------------------------------------------------ */

function MediaPreview({
  message,
  baseUrl,
  onPhotoClick,
}: {
  message: TelegramMessage;
  baseUrl: string;
  onPhotoClick?: (msg: TelegramMessage) => void;
}) {
  const media = message.media;
  if (!media) return null;
  const src = resolveMediaUrl(baseUrl, media.local_path);

  if (media.kind === "photo") {
    return (
      <button
        type="button"
        onClick={() => onPhotoClick?.(message)}
        className="mt-2 inline-flex overflow-hidden rounded border border-border bg-muted/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        aria-label="Open photo preview"
        data-testid={`tg-photo-thumb-${message.id}`}
      >
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img
          src={src}
          alt={message.content ?? "Telegram photo"}
          className="h-16 w-16 object-cover"
          loading="lazy"
        />
      </button>
    );
  }

  if (media.kind === "voice") {
    const duration = media.duration_sec
      ? `${Math.round(media.duration_sec)}s`
      : null;
    return (
      <div
        className="mt-2 flex flex-wrap items-center gap-2 rounded border border-border bg-muted/30 px-2 py-1.5 text-xs"
        data-testid={`tg-voice-${message.id}`}
      >
        <Mic className="h-3.5 w-3.5 text-accent-foreground" aria-hidden="true" />
        <span className="font-mono text-[11px] text-muted-foreground">
          voice{duration ? ` · ${duration}` : ""}
        </span>
        {/* eslint-disable-next-line jsx-a11y/media-has-caption */}
        <audio
          controls
          preload="none"
          src={src}
          className="h-7 max-w-[220px]"
        />
      </div>
    );
  }

  // document
  const label = media.filename ?? media.local_path.split("/").pop() ?? "file";
  return (
    <a
      href={src}
      target="_blank"
      rel="noreferrer"
      className="mt-2 inline-flex items-center gap-2 rounded border border-border bg-muted/30 px-2 py-1.5 text-xs hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      data-testid={`tg-doc-${message.id}`}
    >
      <FileText className="h-3.5 w-3.5 text-muted-foreground" aria-hidden="true" />
      <span className="truncate font-mono text-[11px]">{label}</span>
      {typeof media.size_bytes === "number" ? (
        <span className="text-[10px] text-muted-foreground">
          {formatBytes(media.size_bytes)}
        </span>
      ) : null}
    </a>
  );
}

/* ------------------------------------------------------------------ */
/*                               Helpers                               */
/* ------------------------------------------------------------------ */

function resolveMediaUrl(baseUrl: string, path: string): string {
  if (!baseUrl) return path;
  if (path.startsWith("http://") || path.startsWith("https://")) return path;
  const sep = baseUrl.endsWith("/") || path.startsWith("/") ? "" : "/";
  return `${baseUrl}${sep}${path}`;
}

function formatTs(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "--:--";
  const d = new Date(ms);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  return `${hh}:${mm}`;
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export default MessageList;
