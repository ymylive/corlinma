"use client";

import * as React from "react";
import { cn } from "@/lib/utils";

/**
 * Motion + assistive-technology primitives shared across the admin UI.
 *
 * - `useMotion()` centralises `prefers-reduced-motion` + coarse-pointer detection.
 * - `<MotionSafe>` gates animated content behind user motion preferences.
 * - `<LiveRegion>` / `<VisuallyHidden>` provide sr-only announcement helpers
 *   (Tailwind's built-in `.sr-only` utility).
 */

export type MotionState = {
  /** User prefers reduced motion (OS-level accessibility setting). */
  reduced: boolean;
  /** Coarse pointer (touch device). */
  touch: boolean;
  /** Animations are safe to run (inverse of `reduced`). */
  motionSafe: boolean;
};

const SSR_STATE: MotionState = {
  reduced: false,
  touch: false,
  motionSafe: true,
};

/**
 * Reactively tracks `(prefers-reduced-motion: reduce)` and `(pointer: coarse)`.
 * SSR-safe: returns `{reduced:false, touch:false, motionSafe:true}` on the
 * server and on the very first client render, then updates after mount.
 */
export function useMotion(): MotionState {
  const [state, setState] = React.useState<MotionState>(SSR_STATE);

  React.useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;

    const reduceMql = window.matchMedia("(prefers-reduced-motion: reduce)");
    const touchMql = window.matchMedia("(pointer: coarse)");

    const update = () => {
      const reduced = reduceMql.matches;
      const touch = touchMql.matches;
      setState({ reduced, touch, motionSafe: !reduced });
    };
    update();

    const subscribe = (
      mql: MediaQueryList,
      handler: (e: MediaQueryListEvent) => void,
    ) => {
      if (mql.addEventListener) {
        mql.addEventListener("change", handler);
        return () => mql.removeEventListener("change", handler);
      }
      // Safari <14 fallback.
      mql.addListener(handler);
      return () => mql.removeListener(handler);
    };

    const unsubReduce = subscribe(reduceMql, update);
    const unsubTouch = subscribe(touchMql, update);
    return () => {
      unsubReduce();
      unsubTouch();
    };
  }, []);

  return state;
}

export interface MotionSafeProps {
  children: React.ReactNode;
  /**
   * Rendered when the user prefers reduced motion. If omitted, `children` is
   * rendered regardless — consumers are expected to read `useMotion()` and
   * strip animations internally.
   */
  fallback?: React.ReactNode;
}

/**
 * Gate for motion-sensitive content. If a `fallback` is supplied, it is
 * rendered whenever `prefers-reduced-motion: reduce` is active; otherwise
 * `children` is passed through unchanged.
 */
export function MotionSafe({ children, fallback }: MotionSafeProps) {
  const { reduced } = useMotion();
  if (reduced && fallback !== undefined) {
    return <>{fallback}</>;
  }
  return <>{children}</>;
}

export type LivePoliteness = "polite" | "assertive";

export interface LiveRegionProps
  extends Omit<React.HTMLAttributes<HTMLSpanElement>, "role" | "aria-live"> {
  politeness?: LivePoliteness;
  /** Optional accessible label for the region. */
  label?: string;
  children?: React.ReactNode;
}

/**
 * Screen-reader-only live region for transient status announcements. Uses
 * `role="status"` + `aria-live` so updates are announced without focus
 * changes.
 */
export const LiveRegion = React.forwardRef<HTMLSpanElement, LiveRegionProps>(
  function LiveRegion(
    { politeness = "polite", label, className, children, ...rest },
    ref,
  ) {
    return (
      <span
        ref={ref}
        role="status"
        aria-live={politeness}
        aria-atomic="true"
        {...(label ? { "aria-label": label } : {})}
        className={cn("sr-only", className)}
        {...rest}
      >
        {children}
      </span>
    );
  },
);

export interface VisuallyHiddenProps
  extends React.HTMLAttributes<HTMLSpanElement> {
  children?: React.ReactNode;
}

/**
 * Renders content visible only to assistive tech. Uses Tailwind's built-in
 * `.sr-only` utility.
 */
export const VisuallyHidden = React.forwardRef<
  HTMLSpanElement,
  VisuallyHiddenProps
>(function VisuallyHidden({ className, children, ...rest }, ref) {
  return (
    <span ref={ref} className={cn("sr-only", className)} {...rest}>
      {children}
    </span>
  );
});

/** Canonical status labels for live regions. Consumers are free to localize. */
export const LiveLabels = {
  updating: "Updating",
  live: "Live",
  complete: "Complete",
  error: "Error",
  loading: "Loading",
} as const;

export type LiveLabel = (typeof LiveLabels)[keyof typeof LiveLabels];
