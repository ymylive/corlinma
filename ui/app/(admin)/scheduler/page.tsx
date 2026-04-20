"use client";

import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
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
import {
  fetchSchedulerJobs,
  fetchSchedulerHistory,
  triggerSchedulerJob,
  type SchedulerJob,
  type SchedulerHistory,
} from "@/lib/api";

/**
 * Scheduler admin page — S6 T3. Cron runtime itself is deferred to M7;
 * this page surfaces the configured `[[scheduler.jobs]]` definitions plus
 * the in-memory trigger-attempt history. The trigger button will POST
 * and display a 501 "not wired" message until the cron loop lands.
 */
export default function SchedulerPage() {
  const qc = useQueryClient();
  const jobs = useQuery<SchedulerJob[]>({
    queryKey: ["admin", "scheduler", "jobs"],
    queryFn: fetchSchedulerJobs,
  });
  const history = useQuery<SchedulerHistory[]>({
    queryKey: ["admin", "scheduler", "history"],
    queryFn: fetchSchedulerHistory,
    refetchInterval: 15_000,
  });

  const [historyOpen, setHistoryOpen] = React.useState(false);
  const [banner, setBanner] = React.useState<string | null>(null);

  const triggerMutation = useMutation({
    mutationFn: (name: string) => triggerSchedulerJob(name),
    onSuccess: () => {
      setBanner("触发请求已提交（结果见历史）");
      qc.invalidateQueries({ queryKey: ["admin", "scheduler", "history"] });
    },
    onError: (err) => {
      // 501 = scheduler not wired; still recorded in history.
      setBanner(
        `后端返回: ${err instanceof Error ? err.message : String(err)} (历史已记录)`,
      );
      qc.invalidateQueries({ queryKey: ["admin", "scheduler", "history"] });
    },
  });

  return (
    <>
      <header className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">定时任务</h1>
        <p className="text-sm text-muted-foreground">
          `[[scheduler.jobs]]` 配置快照。cron 运行时 M7 落地 — 当前触发按钮会记录历史但不执行。
        </p>
      </header>

      {banner ? (
        <div
          role="alert"
          className="flex items-center justify-between rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-sm text-amber-200"
        >
          <span>{banner}</span>
          <Button size="sm" variant="ghost" onClick={() => setBanner(null)}>
            关闭
          </Button>
        </div>
      ) : null}

      <section className="flex justify-end">
        <Button variant="outline" size="sm" onClick={() => setHistoryOpen(true)}>
          查看历史 ({history.data?.length ?? 0})
        </Button>
      </section>

      <section className="rounded-lg border border-border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead>Cron</TableHead>
              <TableHead>Timezone</TableHead>
              <TableHead>Action</TableHead>
              <TableHead>Next Fire</TableHead>
              <TableHead>Last Status</TableHead>
              <TableHead className="w-32">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {jobs.isPending ? (
              <TableRow>
                <TableCell colSpan={7}>
                  <Skeleton className="h-4 w-full" />
                </TableCell>
              </TableRow>
            ) : jobs.data && jobs.data.length === 0 ? (
              <TableRow>
                <TableCell colSpan={7} className="py-8 text-center text-sm text-muted-foreground">
                  no jobs configured
                </TableCell>
              </TableRow>
            ) : (
              jobs.data?.map((j) => (
                <TableRow key={j.name}>
                  <TableCell className="font-medium">{j.name}</TableCell>
                  <TableCell className="font-mono text-xs">{j.cron}</TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {j.timezone ?? "(utc)"}
                  </TableCell>
                  <TableCell>
                    <Badge variant="outline">{j.action_kind}</Badge>
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {j.next_fire_at ?? "—"}
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {j.last_status ?? "—"}
                  </TableCell>
                  <TableCell>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={triggerMutation.isPending}
                      onClick={() => triggerMutation.mutate(j.name)}
                      data-testid={`scheduler-trigger-${j.name}`}
                    >
                      Trigger
                    </Button>
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </section>

      <Dialog open={historyOpen} onOpenChange={setHistoryOpen}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>触发历史</DialogTitle>
          </DialogHeader>
          <div className="max-h-96 overflow-auto">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>At</TableHead>
                  <TableHead>Job</TableHead>
                  <TableHead>Source</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Message</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {(history.data ?? []).length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={5} className="py-4 text-center text-sm text-muted-foreground">
                      no history yet
                    </TableCell>
                  </TableRow>
                ) : (
                  history.data?.map((h, i) => (
                    <TableRow key={`${h.at}-${i}`}>
                      <TableCell className="text-xs">{h.at}</TableCell>
                      <TableCell className="font-medium">{h.job}</TableCell>
                      <TableCell className="text-xs">{h.source}</TableCell>
                      <TableCell className="text-xs">{h.status}</TableCell>
                      <TableCell className="text-xs text-muted-foreground">
                        {h.message}
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
