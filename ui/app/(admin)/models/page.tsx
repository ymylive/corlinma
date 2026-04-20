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
  fetchModels,
  updateAliases,
  type ModelsResponse,
} from "@/lib/api";

/**
 * Models admin page — S6 T5. Lists configured providers (enabled + api-key
 * kind) and a CRUD grid for `models.aliases`. Save posts the whole map to
 * `POST /admin/models/aliases`; enabling a provider is handled via the
 * config editor page (not here) because it touches `[providers.*]`.
 */
export default function ModelsPage() {
  const qc = useQueryClient();
  const models = useQuery<ModelsResponse>({
    queryKey: ["admin", "models"],
    queryFn: fetchModels,
  });

  const [aliases, setAliases] = React.useState<Array<[string, string]>>([]);
  const [defaultModel, setDefaultModel] = React.useState("");
  const [initialized, setInitialized] = React.useState(false);
  React.useEffect(() => {
    if (models.data && !initialized) {
      setAliases(Object.entries(models.data.aliases));
      setDefaultModel(models.data.default);
      setInitialized(true);
    }
  }, [models.data, initialized]);

  const saveMutation = useMutation({
    mutationFn: () => {
      const map: Record<string, string> = {};
      for (const [k, v] of aliases) {
        if (k.trim() && v.trim()) map[k.trim()] = v.trim();
      }
      return updateAliases(map, defaultModel.trim() || undefined);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["admin", "models"] });
    },
  });

  return (
    <>
      <header className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">模型路由</h1>
        <p className="text-sm text-muted-foreground">
          实时对接 `GET /admin/models` + `POST /admin/models/aliases`。
          Provider 开关在配置编辑页修改（跨节）。
        </p>
      </header>

      <section className="rounded-lg border border-border">
        <div className="border-b border-border p-3 text-sm font-medium">
          Providers
        </div>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead>Enabled</TableHead>
              <TableHead>API Key</TableHead>
              <TableHead>Base URL</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {models.isPending ? (
              <TableRow>
                <TableCell colSpan={4}>
                  <Skeleton className="h-4 w-full" />
                </TableCell>
              </TableRow>
            ) : models.data && models.data.providers.length === 0 ? (
              <TableRow>
                <TableCell colSpan={4} className="py-4 text-center text-sm text-muted-foreground">
                  no providers configured
                </TableCell>
              </TableRow>
            ) : (
              models.data?.providers.map((p) => (
                <TableRow key={p.name}>
                  <TableCell className="font-medium">{p.name}</TableCell>
                  <TableCell>
                    {p.enabled ? (
                      <Badge className="border-transparent bg-emerald-600/20 text-emerald-300">
                        enabled
                      </Badge>
                    ) : (
                      <Badge variant="secondary">disabled</Badge>
                    )}
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {p.has_api_key ? (
                      <Badge variant="outline">{p.api_key_kind}</Badge>
                    ) : (
                      <span className="text-destructive">missing</span>
                    )}
                  </TableCell>
                  <TableCell className="font-mono text-xs text-muted-foreground">
                    {p.base_url ?? "(provider default)"}
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </section>

      <section className="space-y-3 rounded-lg border border-border p-4">
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold">Aliases</h2>
          <div className="flex items-center gap-2">
            <span className="text-xs text-muted-foreground">default:</span>
            <Input
              value={defaultModel}
              onChange={(e) => setDefaultModel(e.target.value)}
              className="w-56"
              placeholder="claude-sonnet-4-5"
            />
            <Button
              size="sm"
              variant="outline"
              onClick={() => setAliases([...aliases, ["", ""]])}
            >
              + Alias
            </Button>
            <Button
              size="sm"
              onClick={() => saveMutation.mutate()}
              disabled={saveMutation.isPending}
              data-testid="models-save-btn"
            >
              {saveMutation.isPending ? "Saving..." : "Save"}
            </Button>
          </div>
        </div>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-52">Alias</TableHead>
              <TableHead>Target Model</TableHead>
              <TableHead className="w-20">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {aliases.length === 0 ? (
              <TableRow>
                <TableCell colSpan={3} className="py-4 text-center text-sm text-muted-foreground">
                  no aliases
                </TableCell>
              </TableRow>
            ) : (
              aliases.map(([alias, target], idx) => (
                <TableRow key={idx}>
                  <TableCell>
                    <Input
                      value={alias}
                      onChange={(e) => {
                        const next = [...aliases];
                        next[idx] = [e.target.value, target];
                        setAliases(next);
                      }}
                      placeholder="smart"
                    />
                  </TableCell>
                  <TableCell>
                    <Input
                      value={target}
                      onChange={(e) => {
                        const next = [...aliases];
                        next[idx] = [alias, e.target.value];
                        setAliases(next);
                      }}
                      placeholder="claude-opus-4-7"
                    />
                  </TableCell>
                  <TableCell>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() =>
                        setAliases(aliases.filter((_, i) => i !== idx))
                      }
                    >
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
            {(saveMutation.error as Error).message}
          </p>
        ) : saveMutation.isSuccess ? (
          <p className="text-xs text-emerald-500">aliases 保存成功</p>
        ) : null}
      </section>
    </>
  );
}
