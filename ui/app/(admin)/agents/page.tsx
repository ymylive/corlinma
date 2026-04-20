"use client";

import * as React from "react";
import Link from "next/link";
import { useQuery } from "@tanstack/react-query";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useI18n } from "@/components/providers";
import { apiFetch, type AgentSummary } from "@/lib/api";

/**
 * Agents admin page — lists Agent/*.txt, click a row to see the (future)
 * Monaco editor. Currently the dialog is a placeholder per plan §17.
 *
 * Backing endpoint: GET /admin/agents served by ui/mock/server.ts in dev.
 */

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / (1024 * 1024)).toFixed(2)} MiB`;
}

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}

export default function AgentsPage() {
  const { t } = useI18n();

  const query = useQuery<AgentSummary[]>({
    queryKey: ["admin", "agents"],
    queryFn: () => apiFetch<AgentSummary[]>("/admin/agents"),
  });

  return (
    <>
      <header className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">
          {t("nav.agents")}
        </h1>
        <p className="text-sm text-muted-foreground">
          编辑 `Agent/*.md`，点击行进入 Monaco 编辑器（frontmatter + 正文）。
        </p>
      </header>

      <section className="rounded-lg border border-border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t("table.agent.name")}</TableHead>
              <TableHead>{t("table.agent.path")}</TableHead>
              <TableHead className="w-32">{t("table.agent.bytes")}</TableHead>
              <TableHead className="w-56">
                {t("table.agent.last_modified")}
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {query.isPending ? (
              Array.from({ length: 3 }).map((_, i) => (
                <TableRow key={`sk-${i}`}>
                  {Array.from({ length: 4 }).map((_, j) => (
                    <TableCell key={j}>
                      <Skeleton className="h-4 w-24" />
                    </TableCell>
                  ))}
                </TableRow>
              ))
            ) : query.isError ? (
              <TableRow>
                <TableCell
                  colSpan={4}
                  className="py-8 text-center text-sm text-destructive"
                >
                  {t("state.error")}: {(query.error as Error).message}
                </TableCell>
              </TableRow>
            ) : !query.data || query.data.length === 0 ? (
              <TableRow>
                <TableCell
                  colSpan={4}
                  className="py-8 text-center text-sm text-muted-foreground"
                >
                  {t("state.empty")}
                </TableCell>
              </TableRow>
            ) : (
              query.data.map((a) => (
                <TableRow key={a.name}>
                  <TableCell className="font-medium">
                    <Link
                      href={{
                        pathname: "/agents/detail",
                        query: { name: a.name },
                      }}
                      className="hover:underline"
                      data-testid={`agent-link-${a.name}`}
                    >
                      {a.name}
                    </Link>
                  </TableCell>
                  <TableCell className="font-mono text-xs text-muted-foreground">
                    {a.file_path}
                  </TableCell>
                  <TableCell className="font-mono text-xs">
                    {formatBytes(a.bytes)}
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {formatTime(a.last_modified)}
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </section>

    </>
  );
}
