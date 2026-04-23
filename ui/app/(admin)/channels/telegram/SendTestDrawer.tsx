"use client";

import * as React from "react";
import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";
import { Send } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Drawer } from "@/components/ui/drawer";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  sendTelegramTestMessage,
  type TelegramSendRequest,
} from "@/lib/api/telegram";

/**
 * "Send test message" drawer for the Telegram admin page.
 *
 * Opens as a `sm` drawer (reusing B4-FE4's primitive) with two fields —
 * `chat_id` and `text`. Send stays disabled until `chat_id.trim()` is
 * non-empty so there's a predictable pre-flight check.
 *
 * The mutation calls `sendTelegramTestMessage`, which returns a
 * `{ status: "not_deployed" }` sentinel if the gateway 404s instead of
 * throwing — so we can toast "admin endpoint pending" without a red error
 * banner when the ops endpoint is still missing.
 */
export function SendTestDrawer({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [chatId, setChatId] = React.useState("");
  const [text, setText] = React.useState("");

  // Reset fields when the drawer opens so stale values don't linger.
  React.useEffect(() => {
    if (open) {
      setChatId("");
      setText("");
    }
  }, [open]);

  const mutation = useMutation({
    mutationFn: (body: TelegramSendRequest) => sendTelegramTestMessage(body),
    onSuccess: (res) => {
      if (res.status === "not_deployed") {
        toast.message("Admin send endpoint not deployed yet");
        return;
      }
      if (res.status === "ok") {
        toast.success("Test message sent");
        onOpenChange(false);
      } else {
        toast.error(res.error ?? "Send failed");
      }
    },
    onError: (err) => {
      const msg = err instanceof Error ? err.message : "Send failed";
      toast.error(msg);
    },
  });

  const trimmedChatId = chatId.trim();
  const sendDisabled = trimmedChatId.length === 0 || mutation.isPending;

  const handleSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (sendDisabled) return;
    mutation.mutate({ chat_id: trimmedChatId, text });
  };

  return (
    <Drawer
      open={open}
      onOpenChange={onOpenChange}
      title="Send test message"
      description="Calls POST /admin/channels/telegram/send."
      width="sm"
      footer={
        <>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => onOpenChange(false)}
          >
            Cancel
          </Button>
          <Button
            type="submit"
            form="tg-send-test-form"
            size="sm"
            disabled={sendDisabled}
            data-testid="tg-send-test-submit"
          >
            <Send className="h-3.5 w-3.5" aria-hidden="true" />
            {mutation.isPending ? "Sending…" : "Send"}
          </Button>
        </>
      }
    >
      <form
        id="tg-send-test-form"
        onSubmit={handleSubmit}
        className="flex flex-col gap-4 p-5"
      >
        <div className="space-y-1.5">
          <Label htmlFor="tg-send-chat-id">Chat ID</Label>
          <Input
            id="tg-send-chat-id"
            placeholder="e.g. -100123 or @username"
            value={chatId}
            onChange={(e) => setChatId(e.target.value)}
            autoComplete="off"
            data-testid="tg-send-chat-id"
            required
          />
          <p className="text-[11px] text-muted-foreground">
            Numeric chat_id for groups (negative) or DMs, or `@username`.
          </p>
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="tg-send-text">Message</Label>
          <textarea
            id="tg-send-text"
            value={text}
            onChange={(e) => setText(e.target.value)}
            rows={5}
            placeholder="Type the test message…"
            className="w-full resize-y rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            data-testid="tg-send-text"
          />
        </div>
      </form>
    </Drawer>
  );
}

export default SendTestDrawer;
