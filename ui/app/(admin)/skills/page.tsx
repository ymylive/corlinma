"use client";

import * as React from "react";
import { useQuery } from "@tanstack/react-query";
import { motion } from "framer-motion";
import { Search, Wrench, X } from "lucide-react";

import { cn } from "@/lib/utils";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { useMotionVariants } from "@/lib/motion";
import { MOCK_SKILLS, type Skill } from "@/lib/mocks/skills";
import { SkillCard } from "@/components/skills/skill-card";
import { SkillDrawer } from "@/components/skills/skill-drawer";

/**
 * Skills Gallery (B2-FE3).
 *
 * Bento-style grid of installed agent skills. Data is currently served from a
 * static mock module; real fetch wires in once B2-BE5 lands.
 */

// TODO(B2-BE5): replace with real /admin/skills fetch.
async function fetchSkills(): Promise<Skill[]> {
  return MOCK_SKILLS;
}

/**
 * Size variants applied to each card to create the bento rhythm. The pattern
 * loops every 7 cards: {big, small, tall, small, small, wide, small}. Indices
 * that produce `col-span-2` / `row-span-2` only activate at `sm+` / `md+`
 * breakpoints so the mobile layout stays a predictable 1-col stream.
 */
const BENTO_VARIANTS = [
  "sm:col-span-2 lg:col-span-2", // 0: wide
  "",                              // 1: standard
  "lg:row-span-2",                 // 2: tall
  "",                              // 2
  "",                              // 4
  "sm:col-span-2",                 // 5: wide on tablet
  "",                              // 6
];

function bentoClass(index: number): string {
  return BENTO_VARIANTS[index % BENTO_VARIANTS.length] ?? "";
}

export default function SkillsPage() {
  const { stagger } = useMotionVariants();
  const [search, setSearch] = React.useState("");
  const [activeTool, setActiveTool] = React.useState<string | null>(null);
  const [drawerSkill, setDrawerSkill] = React.useState<Skill | null>(null);

  const query = useQuery<Skill[]>({
    queryKey: ["skills"],
    queryFn: fetchSkills,
  });

  const skills = React.useMemo(() => query.data ?? [], [query.data]);

  // Compute distinct tool list for the filter-pill row.
  const toolUniverse = React.useMemo(() => {
    const seen = new Set<string>();
    for (const s of skills) {
      for (const t of s.allowed_tools) seen.add(t);
    }
    return Array.from(seen).sort();
  }, [skills]);

  const filtered = React.useMemo(() => {
    const q = search.trim().toLowerCase();
    return skills.filter((s) => {
      if (activeTool && !s.allowed_tools.includes(activeTool)) return false;
      if (!q) return true;
      return (
        s.name.toLowerCase().includes(q) ||
        s.description.toLowerCase().includes(q) ||
        s.allowed_tools.some((t) => t.toLowerCase().includes(q))
      );
    });
  }, [skills, search, activeTool]);

  const openSkill = React.useCallback((skill: Skill) => {
    setDrawerSkill(skill);
  }, []);

  return (
    <>
      <header className="flex flex-col gap-3">
        <div className="space-y-1">
          <h1 className="text-2xl font-semibold tracking-tight">Skills</h1>
          <p className="text-sm text-muted-foreground">
            Installed agent skills discovered under <code className="font-mono text-xs">skills/</code>.
            Click a card for install + tool details.
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <div className="relative">
            <Search
              className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground"
              aria-hidden="true"
            />
            <Input
              placeholder="Filter skills by name, description or tool..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              aria-label="Search skills"
              className="h-9 w-full pl-8 text-xs sm:w-80"
            />
          </div>
          {activeTool ? (
            <button
              type="button"
              onClick={() => setActiveTool(null)}
              className="inline-flex h-7 items-center gap-1 rounded-full bg-primary/15 px-2 font-mono text-[10px] text-primary transition-colors hover:bg-primary/25"
              aria-label={`Clear filter ${activeTool}`}
            >
              {activeTool}
              <X className="h-3 w-3" aria-hidden="true" />
            </button>
          ) : null}
        </div>

        {toolUniverse.length > 0 ? (
          <div
            className="flex flex-wrap items-center gap-1.5"
            role="group"
            aria-label="Filter by tool"
          >
            {toolUniverse.slice(0, 16).map((tool) => {
              const active = tool === activeTool;
              return (
                <button
                  key={tool}
                  type="button"
                  onClick={() => setActiveTool(active ? null : tool)}
                  aria-pressed={active}
                  className={cn(
                    "inline-flex items-center rounded-full px-2 py-0.5 font-mono text-[10px] transition-colors",
                    active
                      ? "bg-primary text-primary-foreground"
                      : "bg-state-hover text-muted-foreground hover:bg-state-press hover:text-foreground",
                  )}
                >
                  {tool}
                </button>
              );
            })}
            {toolUniverse.length > 16 ? (
              <span className="font-mono text-[10px] text-muted-foreground">
                +{toolUniverse.length - 16} more
              </span>
            ) : null}
          </div>
        ) : null}
      </header>

      {query.isPending ? (
        <div className="grid auto-rows-[148px] grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {Array.from({ length: 8 }).map((_, i) => (
            <Skeleton key={i} className={cn("h-full w-full", bentoClass(i))} />
          ))}
        </div>
      ) : query.isError ? (
        <EmptyState
          icon={<Wrench />}
          title="Could not load skills"
          description={(query.error as Error)?.message ?? "Unknown error."}
        />
      ) : filtered.length === 0 ? (
        <EmptyState
          icon={<Wrench />}
          title={
            search || activeTool ? "No skills match" : "No skills installed"
          }
          description={
            search || activeTool
              ? "Try clearing the search or tool filter."
              : "Add a skill under skills/ in your config dir."
          }
        />
      ) : (
        <motion.div
          initial="hidden"
          animate="visible"
          variants={stagger}
          className="grid auto-rows-[minmax(148px,1fr)] grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4"
          data-testid="skills-grid"
        >
          {filtered.map((skill, i) => (
            <SkillCard
              key={skill.name}
              skill={skill}
              className={bentoClass(i)}
              onOpen={openSkill}
            />
          ))}
        </motion.div>
      )}

      <SkillDrawer
        skill={drawerSkill}
        open={drawerSkill !== null}
        onOpenChange={(next) => {
          if (!next) setDrawerSkill(null);
        }}
      />
    </>
  );
}
