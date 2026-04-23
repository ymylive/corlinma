"use client";

import * as React from "react";

import { Drawer } from "@/components/ui/drawer";
import { cn } from "@/lib/utils";
import { CATEGORY_META, categorize } from "./skill-card";
import type { Skill } from "@/lib/mocks/skills";

/**
 * Right-side drawer that renders the full skill detail view.
 *
 * Chrome (overlay, slide animation, focus-trap, Esc-to-close, close button)
 * is delegated to the shared `<Drawer>` primitive. This component owns only
 * the body layout specific to skills.
 */

export interface SkillDrawerProps {
  skill: Skill | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function SkillDrawer({ skill, open, onOpenChange }: SkillDrawerProps) {
  return (
    <Drawer
      open={open}
      onOpenChange={onOpenChange}
      width="md"
      title={skill?.name ?? ""}
      description={skill?.description}
    >
      {skill ? <SkillDrawerBody skill={skill} /> : null}
    </Drawer>
  );
}

function SkillDrawerBody({ skill }: { skill: Skill }) {
  const category = categorize(skill.name, skill.allowed_tools);
  const meta = CATEGORY_META[category];
  const CategoryIcon = meta.icon;

  return (
    <div className="flex flex-col gap-5 px-5 py-5 text-sm">
      <div className="flex items-center gap-3">
        <div
          className={cn(
            "flex h-10 w-10 shrink-0 items-center justify-center rounded-md text-xl",
            meta.badgeClass,
          )}
          aria-hidden="true"
        >
          {skill.emoji ? (
            <span>{skill.emoji}</span>
          ) : (
            <CategoryIcon className="h-5 w-5" />
          )}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <span
            className={cn(
              "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-medium",
              meta.badgeClass,
            )}
          >
            <CategoryIcon className="h-3 w-3" aria-hidden="true" />
            {meta.label}
          </span>
          <span className="font-mono text-[10px] text-muted-foreground">
            {skill.source_path}
          </span>
        </div>
      </div>

      <Section title="Overview">
        <p className="whitespace-pre-wrap text-sm text-foreground/90">
          {skill.body_markdown}
        </p>
      </Section>

      <Section title={`Allowed tools (${skill.allowed_tools.length})`}>
        <ul className="flex flex-wrap gap-1.5">
          {skill.allowed_tools.map((tool) => (
            <li
              key={tool}
              className="inline-flex items-center rounded-md bg-state-hover px-2 py-0.5 font-mono text-[11px] text-foreground/90"
            >
              {tool}
            </li>
          ))}
        </ul>
      </Section>

      <Section title={`Requires (${skill.requires.length})`}>
        {skill.requires.length === 0 ? (
          <p className="text-xs text-muted-foreground">No runtime dependencies.</p>
        ) : (
          <ul className="space-y-1">
            {skill.requires.map((req) => (
              <li
                key={req}
                className="rounded-md border border-border bg-surface/60 px-2 py-1 font-mono text-[11px] text-foreground/90"
              >
                {req}
              </li>
            ))}
          </ul>
        )}
      </Section>

      <Section title="Install">
        <p className="whitespace-pre-wrap text-xs text-muted-foreground">
          {skill.install}
        </p>
      </Section>
    </div>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="space-y-2">
      <h4 className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
        {title}
      </h4>
      {children}
    </section>
  );
}

export default SkillDrawer;
