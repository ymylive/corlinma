/**
 * Cross-panel hover state for the tagmemo dashboard.
 *
 * The tagmemo page hosts three linked visualisations (scatter / dual-line /
 * pyramid). Hovering one should highlight the same chunk in the other two.
 * Rather than drill a pair of props through every leaf, we expose the
 * `hoveredId` state through a tiny React context. The context is scoped to
 * the page root, so stories / tests that render panels standalone can also
 * provide their own value.
 *
 * This module stays JSX-free so it can keep the `.ts` extension called for
 * in the B5-FE1 spec — `HoveredIdProvider` is built with `createElement`.
 */
"use client";

import * as React from "react";

export interface HoveredIdValue {
  hoveredId: number | null;
  setHoveredId: (id: number | null) => void;
}

const HoveredIdContext = React.createContext<HoveredIdValue | null>(null);

export function HoveredIdProvider(props: {
  children: React.ReactNode;
}): React.ReactElement {
  const [hoveredId, setHoveredId] = React.useState<number | null>(null);
  const value = React.useMemo<HoveredIdValue>(
    () => ({ hoveredId, setHoveredId }),
    [hoveredId],
  );
  return React.createElement(
    HoveredIdContext.Provider,
    { value },
    props.children,
  );
}

const FALLBACK: HoveredIdValue = {
  hoveredId: null,
  setHoveredId: () => {
    /* no-op outside provider */
  },
};

/**
 * Returns the shared hover state. When rendered outside a provider (tests,
 * stories) we return a null/no-op pair so callers don't have to guard.
 */
export function useHoveredId(): HoveredIdValue {
  const ctx = React.useContext(HoveredIdContext);
  return ctx ?? FALLBACK;
}
