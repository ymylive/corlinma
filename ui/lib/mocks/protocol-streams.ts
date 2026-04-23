/**
 * Mock SSE streams for the Protocol Playground (B3-FE1).
 *
 * Two async generators producing token-by-token output:
 *   - `streamBlockProtocol(prompt)` — our `<<<[TOOL_REQUEST]>>>` +
 *     「始」「末」 block format (per-agent opt-in).
 *   - `streamFunctionCall(prompt)` — OpenAI-style `tool_calls` JSON
 *     rendered as tokens.
 *
 * The displayed syntax is illustrative — no parsing, no gateway round-trip.
 *
 * TODO(B3-BE1/BE2): replace with real SSE hits against
 *   `GET /admin/playground/protocol?variant=block` and
 *   `GET /admin/playground/protocol?variant=function-call` once the gateway
 *   bridge lands. Preserve the token-level granularity — the UI reveals one
 *   SSE event at a time and depends on small chunks for smooth animation.
 */

export type ProtocolVariant = "block" | "function-call";

export interface StreamOptions {
  /** Delay between tokens in ms. Default 25. Pass 0 for instant (tests). */
  tokenDelayMs?: number;
  /** AbortSignal — generator exits on abort. */
  signal?: AbortSignal;
}

function tokenize(text: string): string[] {
  // Keep whitespace + separators attached so concatenation reconstructs the
  // exact string. Matches runs of word chars, single non-word chars, or
  // whitespace runs.
  return text.match(/\s+|[^\s]+/g) ?? [];
}

function delay(ms: number, signal?: AbortSignal): Promise<void> {
  if (ms <= 0) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const t = setTimeout(resolve, ms);
    if (signal) {
      const onAbort = () => {
        clearTimeout(t);
        reject(new DOMException("aborted", "AbortError"));
      };
      if (signal.aborted) onAbort();
      else signal.addEventListener("abort", onAbort, { once: true });
    }
  });
}

function buildBlockText(prompt: string): string {
  const query = prompt.trim() || "corlinman protocol spec";
  return [
    "I'll look this up for you.",
    "",
    "<<<[TOOL_REQUEST]>>>",
    `tool_name:「始」web.search「末」`,
    `query:「始」${query}「末」`,
    `top_k:「始」3「末」`,
    "<<<[END_TOOL_REQUEST]>>>",
    "",
    "Results received. Summarising the top hit inline —",
    "the block-protocol form keeps tool calls legible in plain chat",
    "logs and survives model providers that don't expose a native",
    "function-calling channel.",
  ].join("\n");
}

function buildFunctionCallText(prompt: string): string {
  const query = prompt.trim() || "corlinman protocol spec";
  const payload = {
    role: "assistant",
    content: "I'll look this up for you.",
    tool_calls: [
      {
        id: "call_01",
        type: "function",
        function: {
          name: "web.search",
          arguments: JSON.stringify({ query, top_k: 3 }),
        },
      },
    ],
  };
  return [
    JSON.stringify(payload, null, 2),
    "",
    "Results received. Summarising the top hit inline —",
    "the function-call form lets the provider enforce the schema",
    "and return structured arguments the runtime can dispatch",
    "without regex parsing.",
  ].join("\n");
}

async function* streamText(
  text: string,
  opts: StreamOptions,
): AsyncGenerator<string, void, unknown> {
  const tokens = tokenize(text);
  const delayMs = opts.tokenDelayMs ?? 25;
  for (const tok of tokens) {
    if (opts.signal?.aborted) return;
    yield tok;
    await delay(delayMs, opts.signal);
  }
}

/** Stream the block-protocol variant token-by-token. */
export function streamBlockProtocol(
  prompt: string,
  opts: StreamOptions = {},
): AsyncGenerator<string, void, unknown> {
  return streamText(buildBlockText(prompt), opts);
}

/** Stream the OpenAI function-call variant token-by-token. */
export function streamFunctionCall(
  prompt: string,
  opts: StreamOptions = {},
): AsyncGenerator<string, void, unknown> {
  return streamText(buildFunctionCallText(prompt), opts);
}
