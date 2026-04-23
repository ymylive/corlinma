"use client";

import * as React from "react";
import { useTranslation } from "react-i18next";

import { Drawer } from "@/components/ui/drawer";
import { JsonView } from "@/components/ui/json-view";
import { cn } from "@/lib/utils";
import { CATEGORY_META, categorize } from "./skill-card";
import type { Skill } from "@/lib/mocks/skills";

/**
 * Right-side modal drawer that renders the full skill detail view in
 * Tidepool chrome.
 *
 * The overlay + slide animation + focus-trap + Esc-to-close all come from the
 * shared `<Drawer>` primitive (Radix Dialog under the hood). This component
 * only owns the body layout — sections for meta / description / allowed
 * tools / install / requires / frontmatter / markdown body.
 *
 * Frontmatter is rendered via `<JsonView>` with the canonical
 * `metadata.openclaw.{emoji,requires,install}` nesting so the preview matches
 * what lives on disk.
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
      width="lg"
      title={skill?.name ?? ""}
      description={skill?.description}
      className="bg-tp-glass-2 backdrop-blur-glass-strong backdrop-saturate-glass-strong"
    >
      {skill ? <SkillDrawerBody skill={skill} /> : null}
    </Drawer>
  );
}

function SkillDrawerBody({ skill }: { skill: Skill }) {
  const { t } = useTranslation();
  const category = categorize(skill.name, skill.allowed_tools);
  const meta = CATEGORY_META[category];
  const CategoryIcon = meta.icon;

  const hasInstall = skill.install.trim().length > 0;
  const hasRequires = skill.requires.length > 0;
  const hasTools = skill.allowed_tools.length > 0;
  const hasBody = skill.body_markdown.trim().length > 0;

  // Reconstruct a minimal frontmatter preview from the Skill shape. This
  // matches the on-disk layout: `name` + `description` at the top, plus the
  // `metadata.openclaw.*` + `allowed-tools` we render throughout the page.
  const frontmatterPreview = React.useMemo(() => {
    return {
      name: skill.name,
      description: skill.description,
      "allowed-tools": skill.allowed_tools,
      metadata: {
        openclaw: {
          emoji: skill.emoji,
          requires: skill.requires,
          install: skill.install,
        },
      },
    };
  }, [skill]);

  return (
    <div className="flex flex-col gap-5 px-5 py-5 text-sm">
      {/* Meta row — emoji + name (large) + requires pill + source path */}
      <div className="flex flex-wrap items-center gap-3">
        <div
          className={cn(
            "flex h-11 w-11 shrink-0 items-center justify-center rounded-full",
            "border border-tp-amber/25 bg-tp-amber-soft text-[20px] leading-none",
          )}
          aria-hidden
        >
          {skill.emoji ? (
            <span className="opacity-85">{skill.emoji}</span>
          ) : (
            <CategoryIcon className="h-5 w-5 text-tp-amber" />
          )}
        </div>
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-[18px] font-medium leading-tight tracking-[-0.01em] text-tp-ink">
            {skill.name}
          </h2>
          <div className="mt-0.5 flex flex-wrap items-center gap-2 font-mono text-[10.5px] uppercase tracking-[0.08em] text-tp-ink-4">
            <span className="inline-flex items-center gap-1">
              <CategoryIcon className="h-3 w-3" aria-hidden />
              {meta.label}
            </span>
            <span aria-hidden>·</span>
            <span className="truncate normal-case tracking-normal">
              {skill.source_path}
            </span>
          </div>
        </div>
        {hasInstall ? (
          <span
            className="inline-flex items-center gap-1.5 rounded-full border border-tp-amber/30 bg-tp-amber-soft px-2.5 py-[3px] font-mono text-[10.5px] text-tp-amber"
            title={skill.install}
          >
            <span aria-hidden className="h-[5px] w-[5px] rounded-full bg-tp-amber" />
            install
          </span>
        ) : (
          <span className="inline-flex items-center gap-1.5 rounded-full border border-tp-ok/30 bg-tp-ok-soft px-2.5 py-[3px] font-mono text-[10.5px] text-tp-ok">
            <span aria-hidden className="h-[5px] w-[5px] rounded-full bg-tp-ok" />
            ready
          </span>
        )}
      </div>

      {/* Description */}
      <p className="text-[14px] leading-[1.6] text-tp-ink-2">
        {skill.description}
      </p>

      {/* Allowed tools */}
      <Section title={`${t("skills.tp.detailAllowedTools")} (${skill.allowed_tools.length})`}>
        {hasTools ? (
          <ul className="flex flex-wrap gap-1.5">
            {skill.allowed_tools.map((tool) => (
              <li
                key={tool}
                className="inline-flex items-center rounded-md border border-tp-glass-edge bg-tp-glass-inner px-2 py-[3px] font-mono text-[11px] text-tp-ink-2"
              >
                {tool}
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-[12.5px] text-tp-ink-4">
            {t("skills.tp.detailAllowedToolsEmpty")}
          </p>
        )}
      </Section>

      {/* Install */}
      <Section title={t("skills.tp.detailInstall")}>
        {hasInstall ? (
          <pre
            className={cn(
              "rounded-lg border border-tp-glass-edge bg-tp-glass-inner",
              "p-3 font-mono text-[11.5px] leading-[1.65] text-tp-ink-2",
              "whitespace-pre-wrap break-words",
            )}
          >
            {skill.install}
          </pre>
        ) : (
          <p className="text-[12.5px] text-tp-ink-4">
            {t("skills.tp.detailInstallEmpty")}
          </p>
        )}
      </Section>

      {/* Requires */}
      <Section title={`${t("skills.tp.detailRequires")} (${skill.requires.length})`}>
        {hasRequires ? (
          <ul className="flex flex-wrap gap-1.5">
            {skill.requires.map((req) => (
              <li
                key={req}
                className="inline-flex items-center rounded-md border border-tp-glass-edge bg-tp-glass-inner px-2 py-[3px] font-mono text-[11px] text-tp-ink-2"
              >
                {req}
              </li>
            ))}
          </ul>
        ) : (
          <p className="text-[12.5px] text-tp-ink-4">
            {t("skills.tp.detailRequiresEmpty")}
          </p>
        )}
      </Section>

      {/* Frontmatter preview */}
      <Section title={t("skills.tp.detailFrontmatter")}>
        <JsonView value={frontmatterPreview} />
      </Section>

      {/* Body — rendered as whitespace-preserving plain text (no markdown
          renderer is available in the codebase today; the plain monospace
          block keeps the raw content readable without pulling a new dep). */}
      <Section title={t("skills.tp.detailBody")}>
        {hasBody ? (
          <p className="whitespace-pre-wrap text-[13.5px] leading-[1.6] text-tp-ink-2">
            {skill.body_markdown}
          </p>
        ) : (
          <p className="text-[12.5px] text-tp-ink-4">
            {t("skills.tp.detailBodyEmpty")}
          </p>
        )}
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
      <h4 className="font-mono text-[10px] uppercase tracking-[0.12em] text-tp-ink-4">
        {title}
      </h4>
      {children}
    </section>
  );
}

export default SkillDrawer;
