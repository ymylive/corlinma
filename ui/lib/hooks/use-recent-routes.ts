"use client";

/**
 * Tracks the last N admin routes the user navigated to. Persists to
 * localStorage so the command palette's "Recent" section survives reloads.
 *
 * Used by:
 *   - <CommandPaletteProvider> — registers a listener on `usePathname` to
 *     push visited routes.
 *   - <CommandPalette /> — reads the list to render the Recent section.
 *
 * The list is de-duplicated (most-recent wins) and capped at `RECENT_MAX`.
 * All localStorage access is wrapped in try/catch so Safari private mode
 * and SSR never throw.
 */

import * as React from "react";

export const RECENT_ROUTES_KEY = "corlinman.cmdk.recent-routes.v1";
export const RECENT_ROUTES_MAX = 5;

function readStorage(): string[] {
  if (typeof window === "undefined") return [];
  try {
    const raw = window.localStorage.getItem(RECENT_ROUTES_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed)
      ? parsed.filter((x): x is string => typeof x === "string").slice(0, RECENT_ROUTES_MAX)
      : [];
  } catch {
    return [];
  }
}

function writeStorage(list: string[]): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(RECENT_ROUTES_KEY, JSON.stringify(list));
  } catch {
    /* ignore */
  }
}

/**
 * Public hook: returns the current recent-routes list and a `record(href)`
 * function the caller invokes when a navigation happens.
 */
export function useRecentRoutes(): {
  routes: string[];
  record: (href: string) => void;
  reload: () => void;
} {
  const [routes, setRoutes] = React.useState<string[]>([]);

  React.useEffect(() => {
    setRoutes(readStorage());
  }, []);

  const record = React.useCallback((href: string) => {
    if (!href) return;
    setRoutes((prev) => {
      const next = [href, ...prev.filter((x) => x !== href)].slice(
        0,
        RECENT_ROUTES_MAX,
      );
      writeStorage(next);
      return next;
    });
  }, []);

  const reload = React.useCallback(() => {
    setRoutes(readStorage());
  }, []);

  return { routes, record, reload };
}
