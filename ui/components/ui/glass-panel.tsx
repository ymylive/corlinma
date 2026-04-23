"use client";

import * as React from "react";
import { cn } from "@/lib/utils";

/**
 * Glass surface primitive — the core container of the Tidepool theme.
 *
 * Three variants trade visual weight against performance:
 *   - `soft` (default) — 24px blur + 1.5x saturation, subtle inset highlight.
 *     Used for lists, sidebars, plain panels.
 *   - `strong` — 28px blur + 1.7x saturation, deeper shadow. Hero-class
 *     surfaces (dashboard hero, palette modal card).
 *   - `subtle` — NO `backdrop-filter`; solid panel with --tp-glass-inner
 *     background + soft shadow. Use inside a scroll container with many
 *     stacked panels to preserve scroll perf — 9 blur layers per viewport
 *     is the budget ceiling.
 *   - `primary` — same as strong, plus the ring/glow outline that marks a
 *     stat chip as the "most active" metric.
 *
 * All variants:
 *   - border: `var(--tp-glass-edge)`
 *   - inset highlight: `var(--tp-glass-hl)` on top edge
 *   - shadow: matches `--tp-shadow-panel` (or `-hero` / `-primary`)
 *
 * Day/night automatic via token substitution — no prop needed.
 */

export type GlassPanelVariant = "soft" | "strong" | "subtle" | "primary";

export type GlassPanelTag = "div" | "section" | "aside" | "article" | "main" | "header" | "footer";

type DivProps = React.HTMLAttributes<HTMLDivElement>;

export interface GlassPanelProps extends DivProps {
  variant?: GlassPanelVariant;
  /** Override rounded corner (default `rounded-2xl` = 16px for soft/strong). */
  rounded?: string;
  /** Render as a different HTML tag. Constrained to block-level semantic tags. */
  as?: GlassPanelTag;
}

const variantClasses: Record<GlassPanelVariant, string> = {
  soft: cn(
    "bg-tp-glass border-tp-glass-edge",
    "backdrop-blur-glass backdrop-saturate-glass",
    "shadow-tp-panel",
  ),
  strong: cn(
    "bg-tp-glass-2 border-tp-glass-edge",
    "backdrop-blur-glass-strong backdrop-saturate-glass-strong",
    "shadow-tp-hero",
  ),
  subtle: cn(
    // Intentionally no backdrop-filter — solid + inner highlight only.
    // Use inside viewports where ≥6 glass panels would otherwise degrade LCP.
    "bg-tp-glass-inner border-tp-glass-edge",
    "shadow-tp-panel",
  ),
  primary: cn(
    "bg-tp-glass-2 border-tp-glass-edge",
    "backdrop-blur-glass backdrop-saturate-glass",
    "shadow-tp-primary",
  ),
};

export const GlassPanel = React.forwardRef<HTMLDivElement, GlassPanelProps>(
  function GlassPanel(
    { variant = "soft", rounded = "rounded-2xl", as: Tag = "div", className, children, ...rest },
    ref,
  ) {
    // Each panel carries a top inset highlight via a pseudo-like layer — we use
    // a real child element so shadow layering doesn't interfere with the outer
    // shadow from the variant. This is a 1px highlight at the top edge that
    // makes the glass feel lit rather than painted.
    const mergedClassName = cn(
      "relative border",
      rounded,
      variantClasses[variant],
      className,
    );
    const commonProps = {
      ref: ref as React.Ref<HTMLDivElement>,
      className: mergedClassName,
      "data-glass-variant": variant,
      ...(rest as DivProps),
    };
    const highlight = (
      <span
        aria-hidden="true"
        className={cn(
          "pointer-events-none absolute inset-x-0 top-0 h-px",
          rounded,
          "bg-tp-glass-hl opacity-80",
        )}
        style={{ mixBlendMode: "overlay" }}
      />
    );
    // Specialised per-tag render to avoid JSX's complex polymorphic union type.
    switch (Tag) {
      case "section":
        return <section {...commonProps}>{highlight}{children}</section>;
      case "aside":
        return <aside {...commonProps}>{highlight}{children}</aside>;
      case "article":
        return <article {...commonProps}>{highlight}{children}</article>;
      case "main":
        return <main {...commonProps}>{highlight}{children}</main>;
      case "header":
        return <header {...commonProps}>{highlight}{children}</header>;
      case "footer":
        return <footer {...commonProps}>{highlight}{children}</footer>;
      default:
        return <div {...commonProps}>{highlight}{children}</div>;
    }
  },
);

export default GlassPanel;
