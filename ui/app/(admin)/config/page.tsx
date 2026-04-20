"use client";

import * as React from "react";
import dynamic from "next/dynamic";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import {
  fetchConfig,
  fetchConfigSchema,
  postConfig,
  type ConfigGetResponse,
  type ConfigPostResponse,
} from "@/lib/api";

// Monaco isn't SSR-safe; lazy-load on the client only.
const Editor = dynamic(() => import("@monaco-editor/react"), { ssr: false });

/**
 * Config editor — S6 T4. Left nav lists top-level TOML sections scraped
 * from the current document; right pane hosts Monaco in TOML mode. The
 * schema endpoint is fetched once and exposed via
 * `window.__corlinmanConfigSchema` for any future custom validator; we
 * stop short of wiring a TOML language-service hover because Monaco
 * doesn't ship one by default and the validator-derive on the backend
 * gives a per-field error list anyway.
 *
 * Save flow:
 *   1. "Validate" → `POST /admin/config { dry_run: true }` — shows issues.
 *   2. "Save" → `dry_run: false`. On success we show restart-required
 *      fields + bump the version hash.
 */
const SECTION_HEADERS = [
  "server",
  "admin",
  "providers",
  "models",
  "channels",
  "rag",
  "approvals",
  "scheduler",
  "logging",
  "meta",
];

export default function ConfigPage() {
  const qc = useQueryClient();
  const config = useQuery<ConfigGetResponse>({
    queryKey: ["admin", "config"],
    queryFn: fetchConfig,
  });
  // Schema fetch is fire-and-forget; we expose the result via window for
  // any consumer that wants to wire richer validation later.
  const schema = useQuery({
    queryKey: ["admin", "config", "schema"],
    queryFn: fetchConfigSchema,
    staleTime: Infinity,
  });
  React.useEffect(() => {
    if (schema.data && typeof window !== "undefined") {
      (window as unknown as Record<string, unknown>).__corlinmanConfigSchema =
        schema.data;
    }
  }, [schema.data]);

  const [draft, setDraft] = React.useState<string>("");
  const [initialized, setInitialized] = React.useState(false);
  const [activeSection, setActiveSection] = React.useState<string>("server");
  const [validateResult, setValidateResult] =
    React.useState<ConfigPostResponse | null>(null);
  const [saveResult, setSaveResult] =
    React.useState<ConfigPostResponse | null>(null);

  React.useEffect(() => {
    if (config.data && !initialized) {
      setDraft(config.data.toml);
      setInitialized(true);
    }
  }, [config.data, initialized]);

  const validateMutation = useMutation({
    mutationFn: () => postConfig(draft, true),
    onSuccess: (r) => {
      setValidateResult(r);
      setSaveResult(null);
    },
    onError: () => setValidateResult(null),
  });
  const saveMutation = useMutation({
    mutationFn: () => postConfig(draft, false),
    onSuccess: (r) => {
      setSaveResult(r);
      setValidateResult(null);
      qc.invalidateQueries({ queryKey: ["admin", "config"] });
    },
    onError: () => setSaveResult(null),
  });

  // Jump the editor to the given section header in the current draft.
  const editorRef = React.useRef<unknown>(null);
  const onMount = (editor: unknown) => {
    editorRef.current = editor;
  };
  const jumpToSection = (section: string) => {
    setActiveSection(section);
    const ed = editorRef.current as
      | { revealLineInCenter?: (n: number) => void; setPosition?: (p: { lineNumber: number; column: number }) => void }
      | null;
    if (!ed) return;
    const lines = draft.split("\n");
    const marker = `[${section}]`;
    const markerTable = `[${section}.`;
    const markerArray = `[[${section}.`;
    for (let i = 0; i < lines.length; i++) {
      const l = lines[i]!.trimStart();
      if (
        l.startsWith(marker) ||
        l.startsWith(markerTable) ||
        l.startsWith(markerArray)
      ) {
        ed.revealLineInCenter?.(i + 1);
        ed.setPosition?.({ lineNumber: i + 1, column: 1 });
        break;
      }
    }
  };

  return (
    <>
      <header className="flex items-start justify-between gap-4">
        <div className="space-y-1">
          <h1 className="text-2xl font-semibold tracking-tight">配置</h1>
          <p className="text-sm text-muted-foreground">
            Monaco 编辑 `config.toml`；保存后写盘 + ArcSwap 热切换。
          </p>
        </div>
        <div className="flex items-center gap-2">
          {config.data ? (
            <code className="rounded bg-muted px-2 py-1 text-xs">
              version: {config.data.version}
            </code>
          ) : null}
          <Button
            size="sm"
            variant="outline"
            onClick={() => validateMutation.mutate()}
            disabled={!initialized || validateMutation.isPending}
            data-testid="config-validate-btn"
          >
            {validateMutation.isPending ? "验证中…" : "Validate (dry-run)"}
          </Button>
          <Button
            size="sm"
            onClick={() => saveMutation.mutate()}
            disabled={!initialized || saveMutation.isPending}
            data-testid="config-save-btn"
          >
            {saveMutation.isPending ? "Saving…" : "Save"}
          </Button>
        </div>
      </header>

      <section className="grid grid-cols-1 gap-4 md:grid-cols-[180px_1fr]">
        <aside className="space-y-1 rounded-lg border border-border p-2">
          {SECTION_HEADERS.map((s) => (
            <button
              type="button"
              key={s}
              onClick={() => jumpToSection(s)}
              className={cn(
                "block w-full rounded px-2 py-1 text-left text-sm",
                activeSection === s
                  ? "bg-accent text-accent-foreground"
                  : "text-muted-foreground hover:bg-accent/40",
              )}
            >
              [{s}]
            </button>
          ))}
        </aside>
        <div className="overflow-hidden rounded-lg border border-border">
          {config.isPending ? (
            <Skeleton className="h-[600px] w-full" />
          ) : config.isError ? (
            <div className="p-4 text-sm text-destructive">
              load failed: {(config.error as Error).message}
            </div>
          ) : (
            <Editor
              height="600px"
              defaultLanguage="ini"
              value={draft}
              onChange={(v) => setDraft(v ?? "")}
              theme="vs-dark"
              onMount={onMount}
              options={{
                fontSize: 13,
                minimap: { enabled: false },
                wordWrap: "on",
                scrollBeyondLastLine: false,
              }}
            />
          )}
        </div>
      </section>

      {validateResult ? (
        <ResultPanel title="Dry-run 结果" result={validateResult} />
      ) : null}
      {saveResult ? (
        <ResultPanel title="保存结果" result={saveResult} highlight />
      ) : null}
      {validateMutation.isError ? (
        <p className="text-sm text-destructive">
          validate 失败: {(validateMutation.error as Error).message}
        </p>
      ) : null}
      {saveMutation.isError ? (
        <p className="text-sm text-destructive">
          save 失败: {(saveMutation.error as Error).message}
        </p>
      ) : null}
    </>
  );
}

function ResultPanel({
  title,
  result,
  highlight,
}: {
  title: string;
  result: ConfigPostResponse;
  highlight?: boolean;
}) {
  return (
    <section
      className={cn(
        "space-y-2 rounded-lg border p-3 text-sm",
        highlight ? "border-emerald-500/40 bg-emerald-500/5" : "border-border",
      )}
    >
      <div className="flex items-center gap-2">
        <span className="font-semibold">{title}</span>
        {result.status === "ok" ? (
          <Badge className="border-transparent bg-emerald-600/20 text-emerald-300">
            ok
          </Badge>
        ) : (
          <Badge variant="destructive">invalid</Badge>
        )}
        {result.version ? (
          <code className="rounded bg-muted px-2 py-0.5 text-xs">
            new version: {result.version}
          </code>
        ) : null}
      </div>
      {result.issues.length > 0 ? (
        <ul className="space-y-1">
          {result.issues.map((iss, i) => (
            <li key={i} className="text-xs">
              <Badge
                variant={iss.level === "error" ? "destructive" : "secondary"}
                className="mr-2"
              >
                {iss.level}
              </Badge>
              <code className="text-muted-foreground">{iss.path}</code>:{" "}
              {iss.message}
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-xs text-muted-foreground">no issues</p>
      )}
      {result.requires_restart.length > 0 ? (
        <p className="text-xs text-amber-400">
          需要重启才能完全生效: {result.requires_restart.join(", ")}
        </p>
      ) : null}
    </section>
  );
}
