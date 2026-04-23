"use client";

import * as React from "react";
import { motion } from "framer-motion";
import {
  Database,
  Globe,
  Plug,
  Search,
  Wrench,
  type LucideIcon,
} from "lucide-react";

import { cn } from "@/lib/utils";
import { TiltCard } from "@/components/ui/tilt-card";
import { useMotionVariants } from "@/lib/motion";
import type { Skill } from "@/lib/mocks/skills";

// ---------- categorization --------------------------------------------------

export type SkillCategory = "dev-tools" | "integrations" | "search" | "memory" | "other";

interface CategoryMeta {
  id: SkillCategory;
  label: string;
  /** Tailwind class for the 4px left rail. */
  railClass: string;
  /** Tailwind class for the emoji badge tint. */
  badgeClass: string;
  /** Lucide icon used as fallback + colour-blind safe cue. */
  icon: LucideIcon;
}

export const CATEGORY_META: Record<SkillCategory, CategoryMeta> = {
  "dev-tools": {
    id: "dev-tools",
    label: "Dev tools",
    railClass: "bg-accent-2",
    badgeClass: "bg-accent-2/15 text-accent-2",
    icon: Wrench,
  },
  integrations: {
    id: "integrations",
    label: "Integrations",
    railClass: "bg-accent-3",
    badgeClass: "bg-accent-3/15 text-accent-3",
    icon: Plug,
  },
  search: {
    id: "search",
    label: "Search",
    railClass: "bg-ok",
    badgeClass: "bg-ok/15 text-ok",
    icon: Search,
  },
  memory: {
    id: "memory",
    label: "Memory",
    railClass: "bg-primary",
    badgeClass: "bg-primary/15 text-primary",
    icon: Database,
  },
  other: {
    id: "other",
    label: "Other",
    railClass: "bg-muted-foreground/50",
    badgeClass: "bg-muted text-muted-foreground",
    icon: Globe,
  },
};

const DEV_TOOL_PREFIXES = ["file_ops", "canvas", "coding_agent", "browser"];
const INTEGRATION_PREFIXES = [
  "discord",
  "gh_issues",
  "bear_notes",
  "1password",
  "gemini",
  "clawhub",
];
const SEARCH_PREFIXES = ["web_search"];
const MEMORY_PREFIXES = ["memory"];

/**
 * Derive a category from a skill's name + tool list. Pure function; no React
 * dependencies so it can be unit-tested in isolation.
 */
export function categorize(skillName: string, tools: string[]): SkillCategory {
  const name = skillName.toLowerCase();
  if (MEMORY_PREFIXES.some((p) => name.startsWith(p))) return "memory";
  if (SEARCH_PREFIXES.some((p) => name.startsWith(p))) return "search";
  if (DEV_TOOL_PREFIXES.some((p) => name.startsWith(p))) return "dev-tools";
  if (INTEGRATION_PREFIXES.some((p) => name.startsWith(p))) return "integrations";

  // Fallback: look at tool prefixes if the name wasn't decisive.
  const prefixes = tools.map((t) => t.split(".")[0]?.toLowerCase() ?? "");
  if (prefixes.some((p) => MEMORY_PREFIXES.includes(p))) return "memory";
  if (prefixes.some((p) => SEARCH_PREFIXES.includes(p))) return "search";
  if (prefixes.some((p) => DEV_TOOL_PREFIXES.includes(p))) return "dev-tools";
  if (prefixes.some((p) => INTEGRATION_PREFIXES.includes(p))) return "integrations";

  return "other";
}

// ---------- card ------------------------------------------------------------

export interface SkillCardProps {
  skill: Skill;
  /** Extra Tailwind classes — typically `col-span-*` / `row-span-*` for bento. */
  className?: string;
  onOpen: (skill: Skill) => void;
}

export function SkillCard({ skill, className, onOpen }: SkillCardProps) {
  const { listItem } = useMotionVariants();
  const category = React.useMemo(
    () => categorize(skill.name, skill.allowed_tools),
    [skill.name, skill.allowed_tools],
  );
  const meta = CATEGORY_META[category];
  const CategoryIcon = meta.icon;

  const visibleTools = skill.allowed_tools.slice(0, 3);
  const overflowCount = skill.allowed_tools.length - visibleTools.length;

  const handleKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onOpen(skill);
    }
  };

  return (
    <motion.div
      variants={listItem}
      className={cn("h-full min-h-[148px]", className)}
      data-testid={`skill-card-${skill.name}`}
      data-category={category}
    >
      <TiltCard
        maxTiltDeg={3}
        role="button"
        tabIndex={0}
        aria-label={`Open ${skill.name} skill details`}
        onClick={() => onOpen(skill)}
        onKeyDown={handleKeyDown}
        className={cn(
          "group relative flex h-full cursor-pointer flex-col overflow-hidden rounded-lg border border-border bg-panel shadow-1 transition-shadow duration-200",
          "hover:shadow-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
        )}
      >
        {/* category left-rail (4px) */}
        <span
          aria-hidden="true"
          className={cn(
            "absolute left-0 top-0 h-full w-[4px]",
            meta.railClass,
          )}
        />

        <div className="flex flex-1 flex-col gap-3 p-4 pl-5">
          <div className="flex items-start justify-between gap-2">
            <div
              className={cn(
                "flex h-9 w-9 shrink-0 items-center justify-center rounded-md text-lg",
                meta.badgeClass,
              )}
              aria-hidden="true"
            >
              {skill.emoji ? (
                <span>{skill.emoji}</span>
              ) : (
                <CategoryIcon className="h-4 w-4" />
              )}
            </div>
            <span
              className={cn(
                "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-medium",
                meta.badgeClass,
              )}
            >
              <CategoryIcon className="h-3 w-3" aria-hidden="true" />
              {meta.label}
            </span>
          </div>

          <div className="min-w-0 space-y-1">
            <h3 className="truncate text-sm font-semibold text-foreground">
              {skill.name}
            </h3>
            <p className="line-clamp-2 text-xs text-muted-foreground">
              {skill.description}
            </p>
          </div>

          <div className="mt-auto flex flex-wrap gap-1.5 pt-2">
            {visibleTools.map((tool) => (
              <span
                key={tool}
                className="inline-flex items-center rounded-md bg-state-hover px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
              >
                {tool}
              </span>
            ))}
            {overflowCount > 0 ? (
              <span
                className="inline-flex items-center rounded-md bg-state-hover px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
                aria-label={`${overflowCount} more tools`}
              >
                +{overflowCount} more
              </span>
            ) : null}
          </div>
        </div>
      </TiltCard>
    </motion.div>
  );
}

export default SkillCard;
