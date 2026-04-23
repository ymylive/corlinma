"use client";

import * as React from "react";
import Link from "next/link";
import { motion } from "framer-motion";

import { cn } from "@/lib/utils";
import { LiveDot } from "@/components/ui/live-dot";
import { useMotion } from "@/components/ui/motion-safe";

/**
 * Single tab entry for the sub-nav beneath the title. `id` drives the
 * `data-state` + active-underline animation; `href` is optional so tabs can
 * be pure state toggles (caller owns activation via `activeTabId`).
 */
export interface ChannelShellTab {
  id: string;
  label: string;
  href?: string;
}

export interface ChannelShellProps {
  /** Stable channel identifier (e.g. "qq", "telegram"). */
  channelId: "qq" | "telegram" | (string & {});
  /** Bold title shown top-left. */
  title: string;
  /** Optional subtitle beneath the title. */
  subtitle?: string;
  /** Connection state drives the LiveDot variant (ok / err). */
  connected: boolean;
  /** Overrides the default "Live" / "Offline" label next to the dot. */
  connectionLabel?: string;
  /** Optional header actions rendered top-right (e.g. Reconnect button). */
  actions?: React.ReactNode;
  /** Optional tab bar under the title. */
  tabs?: ChannelShellTab[];
  /** The id of the currently active tab — matched against `tabs[].id`. */
  activeTabId?: string;
  /** Called when a non-linked tab is clicked. */
  onTabChange?: (tabId: string) => void;
  /** Page body. */
  children: React.ReactNode;
}

/**
 * Shared chrome for channel admin pages.
 *
 * Top bar: title + subtitle + LiveDot connection indicator, with optional
 * `actions` slot on the right.
 *
 * Tab bar: optional; the active tab grows a shared-`layoutId` underline that
 * animates between tabs via framer-motion. Collapses to an instant swap
 * under `prefers-reduced-motion`.
 */
export function ChannelShell({
  channelId,
  title,
  subtitle,
  connected,
  connectionLabel,
  actions,
  tabs,
  activeTabId,
  onTabChange,
  children,
}: ChannelShellProps) {
  const { reduced } = useMotion();

  const label = connectionLabel ?? (connected ? "Live" : "Offline");
  const variant = connected ? "ok" : "err";

  return (
    <div
      className="flex flex-col gap-4"
      data-channel-id={channelId}
      data-testid={`channel-shell-${channelId}`}
    >
      <header className="flex items-start justify-between gap-3">
        <div className="min-w-0 space-y-1">
          <div className="flex items-center gap-2">
            <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
            <LiveDot
              variant={variant}
              pulse
              label={label}
              data-testid="channel-shell-live-dot"
            />
            <span className="text-xs font-medium text-muted-foreground">
              {label}
            </span>
          </div>
          {subtitle ? (
            <p className="text-sm text-muted-foreground">{subtitle}</p>
          ) : null}
        </div>
        {actions ? (
          <div className="flex shrink-0 items-center gap-2">{actions}</div>
        ) : null}
      </header>

      {tabs && tabs.length > 0 ? (
        <div
          role="tablist"
          aria-label={`${title} sections`}
          className="flex items-center gap-1 border-b border-border"
        >
          {tabs.map((tab) => {
            const active = tab.id === activeTabId;
            const className = cn(
              "relative inline-flex h-9 items-center px-3 text-sm font-medium transition-colors",
              active
                ? "text-foreground"
                : "text-muted-foreground hover:text-foreground",
            );
            const underline = active ? (
              reduced ? (
                <span
                  aria-hidden
                  data-testid="channel-shell-tab-underline"
                  className="absolute inset-x-0 bottom-0 h-[2px] rounded-full bg-primary"
                />
              ) : (
                <motion.span
                  aria-hidden
                  layoutId="channel-tab-underline"
                  data-testid="channel-shell-tab-underline"
                  className="absolute inset-x-0 bottom-0 h-[2px] rounded-full bg-primary"
                  transition={{
                    type: "spring",
                    stiffness: 500,
                    damping: 40,
                    mass: 0.6,
                  }}
                />
              )
            ) : null;

            if (tab.href) {
              return (
                <Link
                  key={tab.id}
                  href={tab.href as never}
                  role="tab"
                  aria-selected={active}
                  data-state={active ? "active" : "inactive"}
                  className={className}
                >
                  {tab.label}
                  {underline}
                </Link>
              );
            }
            return (
              <button
                key={tab.id}
                type="button"
                role="tab"
                aria-selected={active}
                data-state={active ? "active" : "inactive"}
                onClick={() => onTabChange?.(tab.id)}
                className={className}
              >
                {tab.label}
                {underline}
              </button>
            );
          })}
        </div>
      ) : null}

      <div className="flex flex-col gap-4">{children}</div>
    </div>
  );
}

export default ChannelShell;
