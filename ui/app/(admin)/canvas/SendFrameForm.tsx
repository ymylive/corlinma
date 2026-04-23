"use client";

import * as React from "react";
import { Send } from "lucide-react";

import { cn } from "@/lib/utils";
import { GlassPanel } from "@/components/ui/glass-panel";
import {
  CANVAS_FRAME_KINDS,
  type CanvasFrameKind,
} from "@/lib/mocks/canvas";

export interface SendFrameFormProps {
  /** Disabled when there is no active session. */
  disabled?: boolean;
  sending?: boolean;
  onSubmit: (kind: CanvasFrameKind, payload: Record<string, unknown>) => void;
  labels: {
    title: string;
    kind: string;
    payload: string;
    payloadPlaceholder: string;
    send: string;
    sending: string;
    invalidJson: string;
  };
}

const DEFAULT_PAYLOAD = `{
  "component": "HeroCard",
  "props": { "title": "Hello" }
}`;

/**
 * Small form panel for posting a frame to the canvas host. Validates the JSON
 * payload on submit and surfaces a red inline error when it fails to parse.
 *
 * Tidepool retoken: the surrounding card is now a `<GlassPanel variant="soft">`
 * and inputs use the `tp-*` token palette so the form blends with the rest of
 * the page.
 *
 * Native `<select>` is used for the kind dropdown — keyboard + screen-reader
 * support come for free.
 */
export function SendFrameForm({
  disabled,
  sending,
  onSubmit,
  labels,
}: SendFrameFormProps) {
  const [kind, setKind] = React.useState<CanvasFrameKind>("a2ui_push");
  const [raw, setRaw] = React.useState<string>(DEFAULT_PAYLOAD);
  const [error, setError] = React.useState<string | null>(null);

  const handleSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    setError(null);
    let parsed: unknown;
    try {
      parsed = raw.trim() === "" ? {} : JSON.parse(raw);
    } catch {
      setError(labels.invalidJson);
      return;
    }
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
      setError(labels.invalidJson);
      return;
    }
    onSubmit(kind, parsed as Record<string, unknown>);
  };

  const inactive = Boolean(disabled || sending);

  return (
    <GlassPanel
      as="section"
      variant="soft"
      className="flex flex-col gap-3 p-4"
      data-testid="canvas-send-frame-form"
    >
      <form onSubmit={handleSubmit} className="flex flex-col gap-3">
        <div className="flex items-center justify-between">
          <h3 className="font-mono text-[10.5px] uppercase tracking-[0.1em] text-tp-ink-3">
            {labels.title}
          </h3>
        </div>

        <label className="flex flex-col gap-1">
          <span className="font-mono text-[10.5px] uppercase tracking-[0.08em] text-tp-ink-4">
            {labels.kind}
          </span>
          <select
            value={kind}
            onChange={(e) => setKind(e.target.value as CanvasFrameKind)}
            disabled={inactive}
            data-testid="canvas-send-kind"
            className={cn(
              "h-9 rounded-md border px-2 text-[13px]",
              "bg-tp-glass-inner border-tp-glass-edge text-tp-ink-2",
              "hover:bg-tp-glass-inner-hover",
              "focus:outline-none focus:ring-2 focus:ring-tp-amber/40",
              "disabled:cursor-not-allowed disabled:opacity-60",
            )}
          >
            {CANVAS_FRAME_KINDS.map((k) => (
              <option key={k} value={k}>
                {k}
              </option>
            ))}
          </select>
        </label>

        <label className="flex flex-col gap-1">
          <span className="font-mono text-[10.5px] uppercase tracking-[0.08em] text-tp-ink-4">
            {labels.payload}
          </span>
          <textarea
            value={raw}
            onChange={(e) => setRaw(e.target.value)}
            disabled={inactive}
            placeholder={labels.payloadPlaceholder}
            data-testid="canvas-send-payload"
            rows={6}
            spellCheck={false}
            className={cn(
              "rounded-md border px-2 py-1.5 font-mono text-[11.5px] leading-relaxed",
              "bg-tp-glass-inner border-tp-glass-edge text-tp-ink-2",
              "placeholder:text-tp-ink-4",
              "focus:outline-none focus:ring-2 focus:ring-tp-amber/40",
              "disabled:cursor-not-allowed disabled:opacity-60",
              error && "border-tp-err/60 focus:ring-tp-err/40",
            )}
          />
        </label>

        {error ? (
          <p
            role="alert"
            className="font-mono text-[11.5px] text-tp-err"
            data-testid="canvas-send-error"
          >
            {error}
          </p>
        ) : null}

        <div className="flex items-center justify-end">
          <button
            type="submit"
            disabled={inactive}
            data-testid="canvas-send-submit"
            className={cn(
              "inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5",
              "font-mono text-[11.5px]",
              "bg-tp-amber-soft text-tp-amber border-tp-amber/30",
              "hover:bg-tp-amber-soft hover:border-tp-amber/50",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-tp-amber/40",
              "disabled:cursor-not-allowed disabled:opacity-50",
            )}
          >
            <Send className="h-3 w-3" aria-hidden />
            {sending ? labels.sending : labels.send}
          </button>
        </div>
      </form>
    </GlassPanel>
  );
}

export default SendFrameForm;
