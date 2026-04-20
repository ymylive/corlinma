"use client";

import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Badge } from "@/components/ui/badge";
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
  fetchQqStatus,
  reconnectQq,
  updateQqKeywords,
  type QqStatus,
} from "@/lib/api";

/**
 * QQ channel admin page — S6 T2. Reads
 * `GET /admin/channels/qq/status`, edits `group_keywords` via
 * `POST /admin/channels/qq/keywords`, and surfaces a reconnect button
 * (backend returns 501 until the runtime hook lands).
 */
export default function QqChannelPage() {
  const qc = useQueryClient();
  const status = useQuery<QqStatus>({
    queryKey: ["admin", "channels", "qq"],
    queryFn: fetchQqStatus,
    refetchInterval: 10_000,
  });

  const [draft, setDraft] = React.useState<Record<string, string[]>>({});
  const [draftInitialized, setDraftInitialized] = React.useState(false);
  React.useEffect(() => {
    if (status.data && !draftInitialized) {
      setDraft(status.data.group_keywords ?? {});
      setDraftInitialized(true);
    }
  }, [status.data, draftInitialized]);

  const saveMutation = useMutation({
    mutationFn: (next: Record<string, string[]>) => updateQqKeywords(next),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["admin", "channels", "qq"] });
    },
  });

  const reconnectMutation = useMutation({
    mutationFn: reconnectQq,
  });

  const addGroup = () => {
    const id = window.prompt("输入群号 (group_id):");
    if (!id) return;
    if (draft[id]) return;
    setDraft({ ...draft, [id]: [] });
  };
  const removeGroup = (id: string) => {
    const next = { ...draft };
    delete next[id];
    setDraft(next);
  };
  const updateGroupKeywords = (id: string, raw: string) => {
    const kws = raw
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    setDraft({ ...draft, [id]: kws });
  };

  return (
    <>
      <header className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">QQ 通道</h1>
        <p className="text-sm text-muted-foreground">
          实时对接 `/admin/channels/qq/status` + `/keywords` + `/reconnect`。
          运行时状态依赖 corlinman-channels 暴露 — M7 前显示 `unknown`。
        </p>
      </header>

      {status.isPending ? (
        <Skeleton className="h-20 w-full" />
      ) : status.isError ? (
        <p className="text-sm text-destructive">
          load failed: {(status.error as Error).message}
        </p>
      ) : status.data ? (
        <section className="space-y-3 rounded-lg border border-border p-4">
          <div className="flex flex-wrap items-center gap-3">
            <StatusIndicator status={status.data} />
            <span className="text-xs text-muted-foreground">ws_url:</span>
            <code className="rounded bg-muted px-2 py-1 text-xs">
              {status.data.ws_url ?? "(none)"}
            </code>
            <span className="text-xs text-muted-foreground">self_ids:</span>
            <code className="rounded bg-muted px-2 py-1 text-xs">
              [{status.data.self_ids.join(", ")}]
            </code>
            <Button
              size="sm"
              variant="outline"
              onClick={() => reconnectMutation.mutate()}
              disabled={!status.data.configured || reconnectMutation.isPending}
              data-testid="qq-reconnect-btn"
            >
              {reconnectMutation.isPending ? "Reconnecting..." : "Reconnect"}
            </Button>
          </div>
          {reconnectMutation.isError ? (
            <p className="text-xs text-amber-500">
              {(reconnectMutation.error as Error).message}
            </p>
          ) : null}
        </section>
      ) : null}

      <section className="space-y-3 rounded-lg border border-border p-4">
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold">群关键词 (group_keywords)</h2>
          <div className="flex gap-2">
            <Button size="sm" variant="outline" onClick={addGroup}>
              + 群
            </Button>
            <Button
              size="sm"
              onClick={() => saveMutation.mutate(draft)}
              disabled={saveMutation.isPending}
              data-testid="qq-save-keywords-btn"
            >
              {saveMutation.isPending ? "Saving..." : "Save"}
            </Button>
          </div>
        </div>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-40">Group ID</TableHead>
              <TableHead>Keywords (逗号分隔)</TableHead>
              <TableHead className="w-20">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {Object.keys(draft).length === 0 ? (
              <TableRow>
                <TableCell colSpan={3} className="py-4 text-center text-sm text-muted-foreground">
                  no overrides
                </TableCell>
              </TableRow>
            ) : (
              Object.entries(draft)
                .sort(([a], [b]) => a.localeCompare(b))
                .map(([id, kws]) => (
                  <TableRow key={id}>
                    <TableCell className="font-mono text-xs">{id}</TableCell>
                    <TableCell>
                      <Input
                        value={kws.join(", ")}
                        onChange={(e) => updateGroupKeywords(id, e.target.value)}
                      />
                    </TableCell>
                    <TableCell>
                      <Button size="sm" variant="ghost" onClick={() => removeGroup(id)}>
                        Remove
                      </Button>
                    </TableCell>
                  </TableRow>
                ))
            )}
          </TableBody>
        </Table>
        {saveMutation.isError ? (
          <p className="text-xs text-destructive">
            Save failed: {(saveMutation.error as Error).message}
          </p>
        ) : saveMutation.isSuccess ? (
          <p className="text-xs text-emerald-500">保存成功</p>
        ) : null}
      </section>
    </>
  );
}

function StatusIndicator({ status }: { status: QqStatus }) {
  if (!status.configured) {
    return <Badge variant="secondary">未配置</Badge>;
  }
  if (!status.enabled) {
    return <Badge variant="outline">已禁用</Badge>;
  }
  if (status.runtime === "connected") {
    return (
      <Badge className="border-transparent bg-emerald-600/20 text-emerald-300">
        connected
      </Badge>
    );
  }
  if (status.runtime === "disconnected") {
    return <Badge variant="destructive">disconnected</Badge>;
  }
  return (
    <Badge className="border-transparent bg-amber-600/20 text-amber-300">
      enabled (runtime unknown)
    </Badge>
  );
}
