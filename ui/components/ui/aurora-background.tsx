"use client";

import * as React from "react";
import { cn } from "@/lib/utils";

/**
 * Fixed-position aurora background — three soft radial gradients layered over
 * a diagonal base. Reads the `--tp-aurora-*` and `--tp-bg-*` tokens so it
 * automatically retints on theme flip.
 *
 * Mount **once** at the admin layout root. Not for per-page use — the three
 * radial layers are costly if repeated.
 *
 * Usage:
 *   <AuroraBackground />  // covers the viewport, z-index: -1
 *
 * Or as a wrapper over a hero surface:
 *   <AuroraBackground asChild>
 *     <section className="...">…</section>
 *   </AuroraBackground>
 */

export interface AuroraBackgroundProps
  extends React.HTMLAttributes<HTMLDivElement> {
  /** When true, render as a `fixed` full-viewport element (default). */
  fixed?: boolean;
}

export const AuroraBackground = React.forwardRef<
  HTMLDivElement,
  AuroraBackgroundProps
>(function AuroraBackground({ fixed = true, className, style, ...rest }, ref) {
  return (
    <div
      ref={ref}
      aria-hidden="true"
      className={cn(
        fixed && "fixed inset-0 -z-10",
        "bg-tp-aurora",
        className,
      )}
      style={{ backgroundAttachment: "fixed", ...style }}
      {...rest}
    />
  );
});

export default AuroraBackground;
