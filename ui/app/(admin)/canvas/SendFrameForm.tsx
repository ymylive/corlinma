"use client";

import * as React from "react";
import { Send } from "lucide-react";

import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
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
 * Native `<select>` is used for the kind dropdown — the task calls for it
 * explicitly (simpler a11y semantics, keyboard + screen-reader support come
 * for free).
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

  return (
    <form
      onSubmit={handleSubmit}
      className="flex flex-col gap-3 rounded-lg border border-border bg-panel p-4"
      data-testid="canvas-send-frame-form"
    >
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-foreground">
          {labels.title}
        </h3>
      </div>

      <label className="flex flex-col gap-1">
        <span className="text-xs text-muted-foreground">{labels.kind}</span>
        <select
          value={kind}
          onChange={(e) => setKind(e.target.value as CanvasFrameKind)}
          disabled={disabled || sending}
          data-testid="canvas-send-kind"
          className="h-9 rounded-md border border-input bg-background px-2 text-sm"
        >
          {CANVAS_FRAME_KINDS.map((k) => (
            <option key={k} value={k}>
              {k}
            </option>
          ))}
        </select>
      </label>

      <label className="flex flex-col gap-1">
        <span className="text-xs text-muted-foreground">{labels.payload}</span>
        <textarea
          value={raw}
          onChange={(e) => setRaw(e.target.value)}
          disabled={disabled || sending}
          placeholder={labels.payloadPlaceholder}
          data-testid="canvas-send-payload"
          rows={5}
          className={cn(
            "rounded-md border border-input bg-background px-2 py-1.5 font-mono text-xs",
            error && "border-err/60 focus-visible:ring-err",
          )}
          spellCheck={false}
        />
      </label>

      {error ? (
        <p
          role="alert"
          className="text-xs text-err"
          data-testid="canvas-send-error"
        >
          {error}
        </p>
      ) : null}

      <div className="flex items-center justify-end">
        <Button
          type="submit"
          disabled={disabled || sending}
          data-testid="canvas-send-submit"
          size="sm"
        >
          <Send className="h-3.5 w-3.5" />
          {sending ? labels.sending : labels.send}
        </Button>
      </div>
    </form>
  );
}

export default SendFrameForm;
