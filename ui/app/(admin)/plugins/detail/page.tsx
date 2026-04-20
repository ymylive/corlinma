"use client";

import * as React from "react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import {
  fetchPluginDetail,
  invokePlugin,
  type PluginDetail,
  type PluginInvokeResponse,
} from "@/lib/api";

/**
 * Plugin detail page — S6 T6. Renders the manifest summary and a
 * `Test invoke` form driven by the manifest's declared tools.
 *
 * The plugin name is passed as `?name=<encoded>`. We used a query param
 * rather than a dynamic segment because `next build --output=export`
 * requires `generateStaticParams` for dynamic segments, and we don't know
 * the set of plugin names at build time.
 */

interface JsonSchemaLike {
  type?: string;
  properties?: Record<string, JsonSchemaLike>;
  required?: string[];
  description?: string;
  enum?: unknown[];
  default?: unknown;
}

interface ToolManifest {
  name: string;
  description?: string;
  input_schema?: JsonSchemaLike;
}

export default function PluginDetailPage() {
  const search = useSearchParams();
  const name = search?.get("name") ?? "";

  const detail = useQuery<PluginDetail>({
    queryKey: ["admin", "plugins", name],
    queryFn: () => fetchPluginDetail(name),
    enabled: !!name,
  });

  const tools = extractTools(detail.data);
  const [selectedTool, setSelectedTool] = React.useState<string>("");
  React.useEffect(() => {
    if (tools.length > 0 && !selectedTool) {
      setSelectedTool(tools[0]!.name);
    }
  }, [tools, selectedTool]);

  if (!name) {
    return (
      <p className="text-sm text-muted-foreground">
        missing `?name=…` in URL — go via <Link href="/plugins" className="underline">plugins list</Link>
      </p>
    );
  }

  return (
    <>
      <header className="space-y-1">
        <Link href="/plugins" className="text-xs text-muted-foreground hover:text-foreground">
          ← 返回插件列表
        </Link>
        <h1 className="text-2xl font-semibold tracking-tight">{name}</h1>
      </header>

      {detail.isPending ? (
        <Skeleton className="h-32 w-full" />
      ) : detail.isError ? (
        <p className="text-sm text-destructive">
          load failed: {(detail.error as Error).message}
        </p>
      ) : detail.data ? (
        <>
          <Summary detail={detail.data} />
          <section className="space-y-3 rounded-lg border border-border p-4">
            <div className="flex items-center gap-2">
              <h2 className="text-sm font-semibold">Test invoke</h2>
              {tools.length > 1 ? (
                <select
                  value={selectedTool}
                  onChange={(e) => setSelectedTool(e.target.value)}
                  className="rounded-md border border-input bg-transparent px-2 py-1 text-sm"
                >
                  {tools.map((t) => (
                    <option key={t.name} value={t.name}>
                      {t.name}
                    </option>
                  ))}
                </select>
              ) : tools.length === 1 ? (
                <code className="text-xs">{tools[0]!.name}</code>
              ) : null}
            </div>
            {tools.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                plugin declares no tools — nothing to invoke
              </p>
            ) : (
              <InvokeForm
                pluginName={name}
                tool={tools.find((t) => t.name === selectedTool) ?? tools[0]!}
              />
            )}
          </section>
        </>
      ) : null}
    </>
  );
}

function Summary({ detail }: { detail: PluginDetail }) {
  return (
    <section className="rounded-lg border border-border p-4">
      <div className="grid grid-cols-2 gap-x-6 gap-y-2 text-sm">
        <Field label="version" value={detail.summary.version} mono />
        <Field label="plugin_type" value={detail.summary.plugin_type} />
        <Field label="origin" value={detail.summary.origin} />
        <Field label="tools" value={String(detail.summary.capabilities.length)} />
      </div>
      <p className="mt-3 text-sm text-muted-foreground">
        {detail.summary.description}
      </p>
      {detail.summary.capabilities.length > 0 ? (
        <div className="mt-3 flex flex-wrap gap-1">
          {detail.summary.capabilities.map((c) => (
            <Badge key={c} variant="outline">
              {c}
            </Badge>
          ))}
        </div>
      ) : null}
    </section>
  );
}

function Field({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="text-xs uppercase text-muted-foreground">{label}:</span>
      <span className={mono ? "font-mono text-xs" : "text-sm"}>{value}</span>
    </div>
  );
}

function extractTools(detail: PluginDetail | undefined): ToolManifest[] {
  if (!detail) return [];
  const manifest = detail.manifest as {
    capabilities?: { tools?: ToolManifest[] };
  };
  return manifest?.capabilities?.tools ?? [];
}

function InvokeForm({ pluginName, tool }: { pluginName: string; tool: ToolManifest }) {
  const schema = tool.input_schema ?? {};
  const props = schema.properties ?? {};
  const simpleFields = Object.entries(props).filter(([, v]) =>
    ["string", "number", "integer", "boolean"].includes(v.type ?? ""),
  );
  const hasRichFields =
    Object.keys(props).length > 0 && simpleFields.length < Object.keys(props).length;

  const [simpleValues, setSimpleValues] = React.useState<Record<string, unknown>>({});
  const [rawJson, setRawJson] = React.useState<string>("{}");
  const [useRaw, setUseRaw] = React.useState<boolean>(
    simpleFields.length === 0 || hasRichFields,
  );
  const [lastResponse, setLastResponse] =
    React.useState<PluginInvokeResponse | null>(null);

  React.useEffect(() => {
    setSimpleValues({});
    setRawJson("{}");
    setUseRaw(simpleFields.length === 0 || hasRichFields);
    setLastResponse(null);
  }, [tool.name, simpleFields.length, hasRichFields]);

  const invoke = useMutation({
    mutationFn: (args: unknown) => invokePlugin(pluginName, tool.name, args),
    onSuccess: (r) => setLastResponse(r),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    let args: unknown;
    if (useRaw) {
      try {
        args = JSON.parse(rawJson);
      } catch {
        invoke.reset();
        setLastResponse(null);
        alert("invalid JSON");
        return;
      }
    } else {
      args = simpleValues;
    }
    invoke.mutate(args);
  };

  return (
    <form className="space-y-3" onSubmit={handleSubmit} data-testid="plugin-invoke-form">
      {tool.description ? (
        <p className="text-xs text-muted-foreground">{tool.description}</p>
      ) : null}

      {!useRaw && simpleFields.length > 0 ? (
        <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
          {simpleFields.map(([key, fieldSchema]) => (
            <label key={key} className="flex flex-col gap-1 text-sm">
              <span className="font-mono text-xs text-muted-foreground">
                {key}
                {(schema.required ?? []).includes(key) ? " *" : ""}
                {fieldSchema.description ? (
                  <span className="ml-2 text-[10px]">— {fieldSchema.description}</span>
                ) : null}
              </span>
              <SimpleFieldInput
                schema={fieldSchema}
                value={simpleValues[key]}
                onChange={(v) =>
                  setSimpleValues({ ...simpleValues, [key]: v })
                }
              />
            </label>
          ))}
        </div>
      ) : (
        <label className="flex flex-col gap-1 text-sm">
          <span className="font-mono text-xs text-muted-foreground">arguments (JSON)</span>
          <textarea
            value={rawJson}
            onChange={(e) => setRawJson(e.target.value)}
            rows={6}
            className="rounded-md border border-input bg-transparent p-2 font-mono text-xs"
          />
        </label>
      )}

      {simpleFields.length > 0 && !hasRichFields ? (
        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          <input
            type="checkbox"
            checked={useRaw}
            onChange={(e) => setUseRaw(e.target.checked)}
          />
          编辑原始 JSON
        </label>
      ) : null}

      <Button type="submit" size="sm" disabled={invoke.isPending} data-testid="plugin-invoke-submit">
        {invoke.isPending ? "Invoking..." : "Invoke"}
      </Button>

      {invoke.isError ? (
        <p className="text-sm text-destructive">
          {(invoke.error as Error).message}
        </p>
      ) : null}

      {lastResponse ? <ResponseBlock response={lastResponse} /> : null}
    </form>
  );
}

function SimpleFieldInput({
  schema,
  value,
  onChange,
}: {
  schema: JsonSchemaLike;
  value: unknown;
  onChange: (v: unknown) => void;
}) {
  const type = schema.type;
  if (schema.enum && schema.enum.length > 0) {
    return (
      <select
        value={String(value ?? "")}
        onChange={(e) => onChange(e.target.value)}
        className="rounded-md border border-input bg-transparent px-2 py-1 text-sm"
      >
        <option value="">(select)</option>
        {schema.enum.map((e) => (
          <option key={String(e)} value={String(e)}>
            {String(e)}
          </option>
        ))}
      </select>
    );
  }
  if (type === "boolean") {
    return (
      <input
        type="checkbox"
        checked={Boolean(value)}
        onChange={(e) => onChange(e.target.checked)}
      />
    );
  }
  if (type === "number" || type === "integer") {
    return (
      <input
        type="number"
        value={typeof value === "number" ? value : ""}
        onChange={(e) => onChange(e.target.value === "" ? undefined : Number(e.target.value))}
        className="h-9 rounded-md border border-input bg-transparent px-2 text-sm"
      />
    );
  }
  return (
    <input
      type="text"
      value={typeof value === "string" ? value : ""}
      onChange={(e) => onChange(e.target.value)}
      className="h-9 rounded-md border border-input bg-transparent px-2 text-sm"
    />
  );
}

function ResponseBlock({ response }: { response: PluginInvokeResponse }) {
  return (
    <div className="rounded-md border border-border p-3 text-xs">
      <div className="flex items-center gap-2">
        {response.status === "success" ? (
          <Badge className="border-transparent bg-emerald-600/20 text-emerald-300">
            success
          </Badge>
        ) : response.status === "accepted" ? (
          <Badge variant="outline">accepted</Badge>
        ) : (
          <Badge variant="destructive">error</Badge>
        )}
        <span className="text-muted-foreground">{response.duration_ms} ms</span>
        {response.task_id ? (
          <code className="text-muted-foreground">task_id: {response.task_id}</code>
        ) : null}
      </div>
      {response.message ? (
        <p className="mt-2 text-destructive">{response.message}</p>
      ) : null}
      {response.result !== undefined && response.result !== null ? (
        <pre className="mt-2 max-h-60 overflow-auto whitespace-pre-wrap font-mono">
          {JSON.stringify(response.result, null, 2)}
        </pre>
      ) : response.result_raw ? (
        <pre className="mt-2 max-h-60 overflow-auto whitespace-pre-wrap font-mono">
          {response.result_raw}
        </pre>
      ) : null}
    </div>
  );
}
