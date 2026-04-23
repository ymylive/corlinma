"use client";

import { usePathname } from "next/navigation";
import { useEffect } from "react";

/**
 * Resets window scroll to the top on every pathname change so routes that
 * share a layout (Batches 2-5) don't inherit the previous page's scroll
 * position. Skips when the URL carries a hash so in-page anchor navigation
 * still works.
 *
 * Mount high in the tree (above `<PageTransition>`); renders nothing.
 */
export function RouteScrollRestore() {
  const pathname = usePathname();
  useEffect(() => {
    // Scroll main content area to top on route change,
    // but not on hash navigation
    if (typeof window === "undefined") return;
    if (window.location.hash) return;
    window.scrollTo({ top: 0, behavior: "instant" });
  }, [pathname]);
  return null;
}
