/**
 * Motion tokens for framer-motion (B1-FE2).
 *
 * All variants are static, serializable objects. Use `useMotionVariants()` from
 * within a React component to receive instant-transition versions when the user
 * has enabled `prefers-reduced-motion`.
 */
import { useReducedMotion, type Variants, type Transition } from "framer-motion";

/** Fade + slide up. Used for panel/section mounts. */
export const fadeUp: Variants = {
  hidden: { opacity: 0, y: 8 },
  visible: {
    opacity: 1,
    y: 0,
    transition: { duration: 0.28, ease: [0.22, 1, 0.36, 1] },
  },
};

/** Parent orchestrator for staggered children (lists, grids). */
export const stagger: Variants = {
  hidden: {},
  visible: {
    transition: { staggerChildren: 0.06, delayChildren: 0.04 },
  },
};

/** Pop-in with subtle overshoot. Good for toasts, dialog content, badges. */
export const springPop: Variants = {
  hidden: { opacity: 0, scale: 0.96 },
  visible: {
    opacity: 1,
    scale: 1,
    transition: { type: "spring", stiffness: 420, damping: 26, mass: 0.7 },
  },
};

/** List-item default — pair with `stagger` on the parent. */
export const listItem: Variants = {
  hidden: { opacity: 0, y: 8 },
  visible: {
    opacity: 1,
    y: 0,
    transition: { duration: 0.24, ease: "easeOut" },
  },
};

/** Spread onto a `<motion.*>` for shared-layout card transitions. */
export const sharedCard = {
  layout: true as const,
  transition: { type: "spring", stiffness: 380, damping: 30 } as Transition,
};

// ---------- reduced-motion friendly copies ----------

const instantFadeUp: Variants = {
  hidden: { opacity: 0, y: 0 },
  visible: { opacity: 1, y: 0, transition: { duration: 0 } },
};

const instantStagger: Variants = {
  hidden: {},
  visible: { transition: { staggerChildren: 0, delayChildren: 0 } },
};

const instantSpringPop: Variants = {
  hidden: { opacity: 0, scale: 1 },
  visible: { opacity: 1, scale: 1, transition: { duration: 0 } },
};

const instantListItem: Variants = {
  hidden: { opacity: 0, y: 0 },
  visible: { opacity: 1, y: 0, transition: { duration: 0 } },
};

const instantSharedCard = {
  layout: true as const,
  transition: { duration: 0 } as Transition,
};

export interface MotionVariants {
  fadeUp: Variants;
  stagger: Variants;
  springPop: Variants;
  listItem: Variants;
  sharedCard: { layout: true; transition: Transition };
}

/**
 * Returns animated or instant variants based on the user's reduced-motion
 * preference. Must be called from within a React component.
 */
export function useMotionVariants(): MotionVariants {
  const reduced = useReducedMotion();
  if (reduced) {
    return {
      fadeUp: instantFadeUp,
      stagger: instantStagger,
      springPop: instantSpringPop,
      listItem: instantListItem,
      sharedCard: instantSharedCard,
    };
  }
  return { fadeUp, stagger, springPop, listItem, sharedCard };
}
