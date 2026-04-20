"use client";

import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  fetchRagStats,
  queryRag,
  rebuildRag,
  type RagQueryResponse,
  type RagStats,
} from "@/lib/api";

/**
 * RAG admin page — S6 T1. Live against `GET /admin/rag/stats`,
 * `GET /admin/rag/query`, `POST /admin/rag/rebuild`.
 *
 * Query panel runs a BM25-only debug scan (dense vectors require the
 * embedding service; the Rust route is documented accordingly). Rebuild
 * is guarded by a `window.confirm` because it re-scans the entire
 * `chunks_fts` virtual table and should not be clicked by accident.
 */
export default function RagPage() {
  const qc = useQueryClient();
  const stats = useQuery<RagStats>({
    queryKey: ["admin", "rag", "stats"],
    queryFn: fetchRagStats,
    refetchInterval: 30_000,
  });

  const [q, setQ] = React.useState("");
  const [k, setK] = React.useState(10);
  const [results, setResults] = React.useState<RagQueryResponse | null>(null);
  const [queryError, setQueryError] = React.useState<string | null>(null);

  const queryMutation = useMutation({
    mutationFn: ({ q, k }: { q: string; k: number }) => queryRag(q, k),
    onSuccess: (data) => {
      setResults(data);
      setQueryError(null);
    },
    onError: (err) => {
      setQueryError(err instanceof Error ? err.message : String(err));
      setResults(null);
    },
  });

  const rebuildMutation = useMutation({
    mutationFn: rebuildRag,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["admin", "rag", "stats"] });
    },
  });

  const handleQuery = (e: React.FormEvent) => {
    e.preventDefault();
    if (!q.trim()) return;
    queryMutation.mutate({ q: q.trim(), k });
  };

  const handleRebuild = () => {
    if (!window.confirm("确定重建 chunks_fts 索引？此操作会重新扫描全部 chunks。")) {
      return;
    }
    rebuildMutation.mutate();
  };

  return (
    <>
      <header className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">RAG 调参</h1>
        <p className="text-sm text-muted-foreground">
          实时对接 `GET /admin/rag/stats` + `/query` + `/rebuild`；查询仅走 BM25 旁路。
        </p>
      </header>

      <section className="grid grid-cols-1 gap-4 md:grid-cols-4">
        <StatsCard label="Files" value={stats.data?.files} loading={stats.isPending} />
        <StatsCard label="Chunks" value={stats.data?.chunks} loading={stats.isPending} />
        <StatsCard label="Tags" value={stats.data?.tags} loading={stats.isPending} />
        <div className="flex flex-col justify-between rounded-lg border border-border p-4">
          <div className="text-xs uppercase text-muted-foreground">Rebuild FTS</div>
          <Button
            onClick={handleRebuild}
            disabled={rebuildMutation.isPending || !stats.data?.ready}
            data-testid="rag-rebuild-btn"
          >
            {rebuildMutation.isPending ? "重建中…" : "一键重建"}
          </Button>
          {rebuildMutation.isError ? (
            <p className="text-xs text-destructive">
              失败: {(rebuildMutation.error as Error).message}
            </p>
          ) : rebuildMutation.isSuccess ? (
            <p className="text-xs text-emerald-500">rebuild 成功</p>
          ) : null}
        </div>
      </section>

      <section className="space-y-3 rounded-lg border border-border p-4">
        <form className="flex flex-wrap gap-2" onSubmit={handleQuery}>
          <Input
            placeholder="BM25 query..."
            value={q}
            onChange={(e) => setQ(e.target.value)}
            className="max-w-md"
            data-testid="rag-query-input"
          />
          <Input
            type="number"
            min={1}
            max={100}
            value={k}
            onChange={(e) => setK(Number(e.target.value) || 10)}
            className="w-24"
            aria-label="top-k"
          />
          <Button type="submit" disabled={queryMutation.isPending || !q.trim()}>
            {queryMutation.isPending ? "查询中…" : "Search"}
          </Button>
        </form>

        {queryError ? (
          <p className="text-sm text-destructive">{queryError}</p>
        ) : null}

        {results ? (
          <div className="overflow-hidden rounded-md border border-border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-24">Chunk ID</TableHead>
                  <TableHead className="w-24">Score</TableHead>
                  <TableHead>Preview</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {results.hits.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={3} className="py-4 text-center text-sm text-muted-foreground">
                      no hits
                    </TableCell>
                  </TableRow>
                ) : (
                  results.hits.map((h) => (
                    <TableRow key={h.chunk_id}>
                      <TableCell className="font-mono text-xs">{h.chunk_id}</TableCell>
                      <TableCell className="font-mono text-xs">
                        {h.score.toFixed(3)}
                      </TableCell>
                      <TableCell className="text-xs text-muted-foreground">
                        {h.content_preview}
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </div>
        ) : null}
      </section>
    </>
  );
}

function StatsCard({
  label,
  value,
  loading,
}: {
  label: string;
  value: number | undefined;
  loading: boolean;
}) {
  return (
    <div className="rounded-lg border border-border p-4">
      <div className="text-xs uppercase text-muted-foreground">{label}</div>
      {loading ? (
        <Skeleton className="mt-2 h-8 w-16" />
      ) : (
        <div className="mt-1 font-mono text-2xl">{value ?? 0}</div>
      )}
    </div>
  );
}
