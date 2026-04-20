"use client";

import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { apiFetch, openEventStream } from "@/lib/api";

/**
 * Admin approvals page — Sprint 2 T3.
 *
 * Dual-channel data model:
 *   1. React Query polls `GET /admin/approvals` for the authoritative
 *      queue (initial load + after mutations + every 15 s as a safety
 *      net in case the SSE subscriber missed anything).
 *   2. `EventSource` subscribes to `GET /admin/approvals/stream` and
 *      nudges the query cache so a new Pending event appears instantly
 *      and a Decided event removes it without waiting for the next poll.
 *
 * The optimistic `decide` mutation flips the local state the moment the
 * operator clicks, then rolls back if the POST fails — matches the UX
 * expectations the plugins page already set.
 */

interface Approval {
  id: string;
  plugin: string;
  tool: string;
  session_key: string;
  args_json: string;
  requested_at: string;
  decided_at: string | null;
  decision: string | null;
}

type StreamEvent =
  | { kind: "pending"; approval: Approval }
  | { kind: "decided"; id: string; decision: string; reason: string | null }
  | { kind: "lag"; message?: string };

type Tab = "pending" | "history";

function fetchApprovals(includeDecided: boolean): Promise<Approval[]> {
  const qs = includeDecided ? "?include_decided=true" : "";
  return apiFetch<Approval[]>(`/admin/approvals${qs}`);
}

function decideApproval(
  id: string,
  approve: boolean,
  reason?: string,
): Promise<{ id: string; decision: string }> {
  return apiFetch(`/admin/approvals/${id}/decide`, {
    method: "POST",
    body: { approve, reason },
  });
}

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}

function DecisionBadge({ decision }: { decision: string | null }) {
  if (!decision) return <Badge variant="secondary">pending</Badge>;
  if (decision === "approved")
    return (
      <Badge className="border-transparent bg-emerald-600/20 text-emerald-300">
        approved
      </Badge>
    );
  if (decision === "denied")
    return <Badge variant="destructive">denied</Badge>;
  return <Badge variant="outline">{decision}</Badge>;
}

function ArgsDialog({ approval }: { approval: Approval }) {
  let pretty = approval.args_json;
  try {
    pretty = JSON.stringify(JSON.parse(approval.args_json), null, 2);
  } catch {
    // Non-JSON payload (base64-encoded bytes) — render verbatim.
  }
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button size="sm" variant="outline">
          view args
        </Button>
      </DialogTrigger>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>
            {approval.plugin}.{approval.tool}
          </DialogTitle>
          <DialogDescription>
            session_key: <span className="font-mono">{approval.session_key || "(empty)"}</span>
          </DialogDescription>
        </DialogHeader>
        <pre className="max-h-96 overflow-auto rounded-md bg-muted p-3 font-mono text-xs">
          {pretty}
        </pre>
      </DialogContent>
    </Dialog>
  );
}

export default function ApprovalsPage() {
  const [tab, setTab] = useState<Tab>("pending");
  const qc = useQueryClient();

  const queryKey = ["admin", "approvals", tab];
  const query = useQuery<Approval[]>({
    queryKey,
    queryFn: () => fetchApprovals(tab === "history"),
    refetchInterval: 15_000,
  });

  const mutation = useMutation({
    mutationFn: ({
      id,
      approve,
      reason,
    }: {
      id: string;
      approve: boolean;
      reason?: string;
    }) => decideApproval(id, approve, reason),
    onMutate: async ({ id }) => {
      // Optimistically drop the row from the pending list.
      await qc.cancelQueries({ queryKey: ["admin", "approvals", "pending"] });
      const prev = qc.getQueryData<Approval[]>(["admin", "approvals", "pending"]);
      if (prev) {
        qc.setQueryData<Approval[]>(
          ["admin", "approvals", "pending"],
          prev.filter((r) => r.id !== id),
        );
      }
      return { prev };
    },
    onError: (_err, _vars, ctx) => {
      if (ctx?.prev) {
        qc.setQueryData(["admin", "approvals", "pending"], ctx.prev);
      }
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: ["admin", "approvals"] });
    },
  });

  useEffect(() => {
    const close = openEventStream<StreamEvent>("/admin/approvals/stream", {
      onMessage: ({ data }) => {
        if (!data || typeof data !== "object") return;
        if (data.kind === "pending") {
          qc.setQueryData<Approval[]>(
            ["admin", "approvals", "pending"],
            (prev) => {
              const next = prev ? [...prev] : [];
              if (!next.some((r) => r.id === data.approval.id)) {
                next.push(data.approval);
              }
              return next;
            },
          );
        } else if (data.kind === "decided") {
          // Remove from pending, invalidate history.
          qc.setQueryData<Approval[]>(
            ["admin", "approvals", "pending"],
            (prev) => (prev ? prev.filter((r) => r.id !== data.id) : prev),
          );
          qc.invalidateQueries({ queryKey: ["admin", "approvals", "history"] });
        }
      },
    });
    return close;
  }, [qc]);

  const rows = query.data ?? [];

  return (
    <>
      <header className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">工具审批</h1>
        <p className="text-sm text-muted-foreground">
          `[[approvals.rules]]` 匹配到 prompt 模式的待审批工具调用队列。
          对应 corlinman-gateway::middleware::approval（Sprint 2 T3）。
        </p>
      </header>

      <div className="flex gap-2">
        <Button
          size="sm"
          variant={tab === "pending" ? "default" : "outline"}
          onClick={() => setTab("pending")}
        >
          待审批
        </Button>
        <Button
          size="sm"
          variant={tab === "history" ? "default" : "outline"}
          onClick={() => setTab("history")}
        >
          历史
        </Button>
      </div>

      <section className="rounded-lg border border-border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Plugin.Tool</TableHead>
              <TableHead>Session</TableHead>
              <TableHead>Requested</TableHead>
              <TableHead>Status</TableHead>
              <TableHead className="w-72">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {query.isPending ? (
              Array.from({ length: 3 }).map((_, i) => (
                <TableRow key={`sk-${i}`}>
                  {Array.from({ length: 5 }).map((_, j) => (
                    <TableCell key={j}>
                      <Skeleton className="h-4 w-24" />
                    </TableCell>
                  ))}
                </TableRow>
              ))
            ) : query.isError ? (
              <TableRow>
                <TableCell colSpan={5} className="py-8 text-center text-sm text-destructive">
                  load failed: {(query.error as Error).message}
                </TableCell>
              </TableRow>
            ) : rows.length === 0 ? (
              <TableRow>
                <TableCell colSpan={5} className="py-8 text-center text-sm text-muted-foreground">
                  {tab === "pending" ? "no pending approvals" : "no history"}
                </TableCell>
              </TableRow>
            ) : (
              rows.map((row) => (
                <TableRow key={row.id}>
                  <TableCell className="font-mono text-xs">
                    {row.plugin}.{row.tool}
                  </TableCell>
                  <TableCell className="font-mono text-xs text-muted-foreground">
                    {row.session_key || "(none)"}
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {formatTime(row.requested_at)}
                  </TableCell>
                  <TableCell>
                    <DecisionBadge decision={row.decision} />
                  </TableCell>
                  <TableCell>
                    <div className="flex gap-2">
                      <ArgsDialog approval={row} />
                      {row.decision === null ? (
                        <>
                          <Button
                            size="sm"
                            onClick={() =>
                              mutation.mutate({ id: row.id, approve: true })
                            }
                            disabled={mutation.isPending}
                          >
                            Approve
                          </Button>
                          <Button
                            size="sm"
                            variant="destructive"
                            onClick={() =>
                              mutation.mutate({
                                id: row.id,
                                approve: false,
                                reason: "denied via admin UI",
                              })
                            }
                            disabled={mutation.isPending}
                          >
                            Deny
                          </Button>
                        </>
                      ) : null}
                    </div>
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
