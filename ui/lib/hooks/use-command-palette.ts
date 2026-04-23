"use client";

/**
 * Thin re-export of the `useCommandPalette` context hook plus a small
 * standalone hotkey-registration helper.
 *
 * Why this file exists:
 *   The palette provider (`components/cmdk-palette.tsx`) owns the dialog
 *   state. Consumers outside that file need a stable import path that
 *   doesn't pull in the full UI component tree. This module gives them one.
 *
 *   B3-FE5 also needs a way to register extra hotkeys (`?`, `/`) without
 *   growing the provider. `useCommandPaletteHotkeys()` attaches a keydown
 *   listener that respects inputs / contenteditable so the `?` key keeps
 *   working inside textareas.
 */

import * as React from "react";

import { useCommandPalette as useCommandPaletteCtx } from "@/components/cmdk-palette";

export const useCommandPalette = useCommandPaletteCtx;

/**
 * Returns `true` when the event target is a text-accepting element where
 * single-character shortcuts (`?`, `/`) should NOT hijack input. Follows
 * the same rule GitHub / Linear use for their `?` help shortcuts.
 */
function isTypingTarget(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null;
  if (!t) return false;
  const tag = t.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
  if (t.isContentEditable) return true;
  return false;
}

export interface CommandPaletteHotkeysOptions {
  /** Toggle the palette open/closed. */
  toggle: () => void;
}

/**
 * Registers the global keyboard shortcuts that open the palette:
 *   - `Cmd+K` / `Ctrl+K` — always, even inside inputs.
 *   - `?` (Shift+/) — only when NOT typing into an input.
 *
 * Returns nothing. Intended to be called once, at the provider level.
 */
export function useCommandPaletteHotkeys({
  toggle,
}: CommandPaletteHotkeysOptions): void {
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Cmd+K / Ctrl+K — universal.
      if ((e.key === "k" || e.key === "K") && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        toggle();
        return;
      }
      // `?` (Shift+/) — skip when typing so help doesn't eat keystrokes.
      if (e.key === "?" && !e.metaKey && !e.ctrlKey && !e.altKey) {
        if (isTypingTarget(e)) return;
        e.preventDefault();
        toggle();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggle]);
}
