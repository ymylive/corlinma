"use client";

import * as React from "react";
import dynamic from "next/dynamic";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { fetchAgent, saveAgent, type AgentContent } from "@/lib/api";

const Editor = dynamic(() => import("@monaco-editor/react"), { ssr: false });

/**
 * Agent detail page — S6 T6. Monaco edits the full file body. Agent name
 * is passed as `?name=<encoded>` (not a dynamic segment — keeps the Next
 * static export happy without a `generateStaticParams()` handshake).
 */
export default function AgentDetailPage() {
  const search = useSearchParams();
  const name = search?.get("name") ?? "";
  const qc = useQueryClient();

  const agent = useQuery<AgentContent>({
    queryKey: ["admin", "agents", name],
    queryFn: () => fetchAgent(name),
    enabled: !!name,
  });

  const [draft, setDraft] = React.useState("");
  const [initialized, setInitialized] = React.useState(false);
  React.useEffect(() => {
    if (agent.data && !initialized) {
      setDraft(agent.data.content);
      setInitialized(true);
    }
  }, [agent.data, initialized]);

  const save = useMutation({
    mutationFn: () => saveAgent(name, draft),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["admin", "agents", name] });
      qc.invalidateQueries({ queryKey: ["admin", "agents"] });
    },
  });

  if (!name) {
    return (
      <p className="text-sm text-muted-foreground">
        missing `?name=…` in URL — go via <Link href="/agents" className="underline">agent list</Link>
      </p>
    );
  }

  return (
    <>
      <header className="flex items-start justify-between gap-4">
        <div className="space-y-1">
          <Link href="/agents" className="text-xs text-muted-foreground hover:text-foreground">
            ← 返回 Agent 列表
          </Link>
          <h1 className="text-2xl font-semibold tracking-tight">{name}</h1>
          {agent.data ? (
            <p className="font-mono text-xs text-muted-foreground">
              {agent.data.file_path} · {agent.data.bytes} bytes
              {agent.data.last_modified ? ` · ${agent.data.last_modified}` : ""}
            </p>
          ) : null}
        </div>
        <Button
          size="sm"
          onClick={() => save.mutate()}
          disabled={!initialized || save.isPending}
          data-testid="agent-save-btn"
        >
          {save.isPending ? "Saving…" : "Save"}
        </Button>
      </header>

      {agent.isPending ? (
        <Skeleton className="h-[600px] w-full" />
      ) : agent.isError ? (
        <p className="text-sm text-destructive">
          load failed: {(agent.error as Error).message}
        </p>
      ) : (
        <section className="overflow-hidden rounded-lg border border-border">
          <Editor
            height="600px"
            defaultLanguage="markdown"
            value={draft}
            onChange={(v) => setDraft(v ?? "")}
            theme="vs-dark"
            options={{
              fontSize: 13,
              minimap: { enabled: false },
              wordWrap: "on",
              scrollBeyondLastLine: false,
            }}
          />
        </section>
      )}

      {save.isError ? (
        <p className="text-sm text-destructive">
          save failed: {(save.error as Error).message}
        </p>
      ) : save.isSuccess ? (
        <p className="text-sm text-emerald-500">保存成功</p>
      ) : null}
    </>
  );
}
