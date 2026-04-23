"use client";

import * as React from "react";

import { LiveDot } from "@/components/ui/live-dot";
import { SplitPane } from "@/components/playground/split-pane";
import { TokenStream } from "@/components/playground/token-stream";
import {
  streamBlockProtocol,
  streamFunctionCall,
  type ProtocolVariant,
} from "@/lib/mocks/protocol-streams";
import { cn } from "@/lib/utils";

const MODELS = ["gpt-4o", "claude-3.5-sonnet", "qwen2.5-72b"] as const;
type Model = (typeof MODELS)[number];

interface PaneState {
  tokens: string[];
  done: boolean;
  running: boolean;
}

const EMPTY_PANE: PaneState = { tokens: [], done: false, running: false };

type StreamFn = (
  prompt: string,
  opts?: { tokenDelayMs?: number; signal?: AbortSignal },
) => AsyncGenerator<string, void, unknown>;

const STREAM_FNS: Record<ProtocolVariant, StreamFn> = {
  block: streamBlockProtocol,
  "function-call": streamFunctionCall,
};

/**
 * Protocol Playground · 协议对比
 *
 * Split-pane comparison of the block-style `<<<[TOOL_REQUEST]>>>` +
 * 「始」「末」 protocol (left) vs. OpenAI function-call JSON (right).
 * Same prompt drives both panes; they stream token-by-token in parallel and
 * pulse-highlight divergent lines once both finish.
 *
 * Mock streams live in `lib/mocks/protocol-streams.ts`. TODO(B3-BE1/BE2):
 * swap for the real gateway SSE endpoint.
 */
export default function ProtocolPlaygroundPage() {
  const [prompt, setPrompt] = React.useState(
    "Search the docs for the block protocol spec and summarise it.",
  );
  const [model, setModel] = React.useState<Model>(MODELS[0]);
  const [block, setBlock] = React.useState<PaneState>(EMPTY_PANE);
  const [fn, setFn] = React.useState<PaneState>(EMPTY_PANE);
  const abortRef = React.useRef<AbortController | null>(null);

  const running = block.running || fn.running;
  const bothDone = block.done && fn.done;

  const runPane = React.useCallback(
    async (
      variant: ProtocolVariant,
      signal: AbortSignal,
      setter: React.Dispatch<React.SetStateAction<PaneState>>,
    ) => {
      setter({ tokens: [], done: false, running: true });
      try {
        for await (const tok of STREAM_FNS[variant](prompt, { signal })) {
          if (signal.aborted) return;
          setter((prev) => ({
            ...prev,
            tokens: [...prev.tokens, tok],
          }));
        }
        if (!signal.aborted) {
          setter((prev) => ({ ...prev, done: true, running: false }));
        }
      } catch {
        if (!signal.aborted) {
          setter((prev) => ({ ...prev, done: true, running: false }));
        }
      }
    },
    [prompt],
  );

  const runBoth = React.useCallback(() => {
    abortRef.current?.abort();
    const ac = new AbortController();
    abortRef.current = ac;
    setBlock({ tokens: [], done: false, running: true });
    setFn({ tokens: [], done: false, running: true });
    void Promise.all([
      runPane("block", ac.signal, setBlock),
      runPane("function-call", ac.signal, setFn),
    ]);
  }, [runPane]);

  React.useEffect(() => {
    return () => abortRef.current?.abort();
  }, []);

  const blockText = React.useMemo(() => block.tokens.join(""), [block.tokens]);
  const fnText = React.useMemo(() => fn.tokens.join(""), [fn.tokens]);

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-wrap items-center justify-between gap-4">
        <div className="space-y-1">
          <h1 className="text-2xl font-semibold tracking-tight">
            Protocol Playground · 协议对比
          </h1>
          <p className="text-sm text-muted-foreground">
            Block-format tool calls vs. OpenAI function-calls, same prompt,
            streamed side-by-side.
          </p>
        </div>

        <div className="flex items-center gap-3">
          <label className="flex items-center gap-2 text-xs text-muted-foreground">
            <span className="uppercase tracking-wider">Model</span>
            <select
              value={model}
              onChange={(e) => setModel(e.target.value as Model)}
              className="rounded-md border border-border bg-card px-2 py-1 font-mono text-xs text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
              data-testid="model-picker"
              aria-label="Select model"
            >
              {MODELS.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
          </label>

          <button
            type="button"
            onClick={runBoth}
            disabled={running}
            data-testid="run-button"
            className={cn(
              "inline-flex items-center gap-2 rounded-md border border-border bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground shadow-1 transition-colors",
              "hover:bg-primary/90 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
              "disabled:opacity-60",
            )}
          >
            {running ? "Streaming…" : "Run both"}
          </button>
        </div>
      </header>

      <label className="flex flex-col gap-1.5">
        <span className="text-xs uppercase tracking-wider text-muted-foreground">
          Prompt
        </span>
        <textarea
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          rows={3}
          data-testid="prompt-input"
          className="w-full resize-none rounded-md border border-border bg-card/60 px-3 py-2 font-mono text-sm text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
          aria-label="Prompt"
          placeholder="Ask the model something that should trigger a tool call…"
        />
      </label>

      <div className="rounded-lg border border-border bg-card/40">
        <SplitPane
          ariaLabel="Resize panes"
          left={
            <PaneShell
              title="Block protocol"
              subtitle="<<<[TOOL_REQUEST]>>> · 「始」「末」"
              dotVariant={
                block.running ? "ok" : block.done ? "muted" : "muted"
              }
              dotPulse={block.running}
              testId="pane-block"
            >
              <TokenStream
                tokens={block.tokens}
                done={block.done || !block.running}
                peerText={fnText}
                diffReady={bothDone}
                data-testid="stream-block"
              />
            </PaneShell>
          }
          right={
            <PaneShell
              title="Function-call"
              subtitle="openai.tool_calls"
              dotVariant={fn.running ? "ok" : fn.done ? "muted" : "muted"}
              dotPulse={fn.running}
              testId="pane-function-call"
              border="left"
            >
              <TokenStream
                tokens={fn.tokens}
                done={fn.done || !fn.running}
                peerText={blockText}
                diffReady={bothDone}
                data-testid="stream-function-call"
              />
            </PaneShell>
          }
        />
      </div>
    </div>
  );
}

interface PaneShellProps {
  title: string;
  subtitle: string;
  dotVariant: "ok" | "muted";
  dotPulse: boolean;
  testId: string;
  border?: "left";
  children: React.ReactNode;
}

function PaneShell({
  title,
  subtitle,
  dotVariant,
  dotPulse,
  testId,
  border,
  children,
}: PaneShellProps) {
  return (
    <section
      data-testid={testId}
      className={cn(
        "flex h-full min-h-[360px] flex-col",
        border === "left" && "border-l border-border",
      )}
    >
      <header className="flex items-center justify-between border-b border-border px-4 py-2">
        <div className="flex items-center gap-2">
          <LiveDot
            variant={dotVariant}
            pulse={dotPulse}
            label={dotPulse ? "streaming" : "idle"}
          />
          <h2 className="text-sm font-semibold text-foreground">{title}</h2>
          <span className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
            {subtitle}
          </span>
        </div>
      </header>
      <div className="flex-1 overflow-auto p-3">{children}</div>
    </section>
  );
}
