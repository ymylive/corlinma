# corlinman-channels — ops notes

## Telegram: webhook vs long-poll

The Telegram adapter runs in one of two modes, picked by
`[telegram.webhook].public_url`:

| `public_url`                                      | Transport  | Notes                                                                  |
| ------------------------------------------------- | ---------- | ---------------------------------------------------------------------- |
| empty string (`""`) or whitespace only            | long-poll  | Development default. Gateway calls `getUpdates` every 25 s.            |
| HTTPS URL (e.g. `https://bot.example.com/tg/wh`)  | webhook    | Telegram POSTs to `POST /channels/telegram/webhook` on the gateway.    |

### Webhook mode

- The gateway validates every request against the
  `X-Telegram-Bot-Api-Secret-Token` header. A mismatch returns 401.
  Setting `[telegram.webhook].secret_token = ""` disables the check
  (don't do this in production; it's only for local tunnels that strip
  headers).
- Media downloads (photo / voice / document) are streamed to
  `$data_dir/media/telegram/<unique>.<ext>`; each file is capped at
  20 MB (the Telegram bot-API hard limit).
- Voice notes emit `HookEvent::MessageTranscribed` with an empty
  transcript until STT lands in a follow-up batch.
- Group routing: the agent only replies when the bot is addressed
  (`@mention`, `reply_to_message.from == bot`). Un-addressed group
  messages still emit `MessageReceived` so analytics subscribers see
  them, but no agent turn is spawned.

### Hot-swap caveat

`public_url` is read **once at gateway boot**. Changing it in the admin
UI or editing the config file at runtime has no effect on the current
process — the operator must restart the gateway for the change to take
hold. This is by design: swapping transport modes while updates are
in-flight would either double-deliver or drop updates, and untangling
that is outside the scope of B4-BE1.

The admin UI should surface this as a tooltip next to the field, e.g.

> "Changing the public URL requires a gateway restart. Leave empty to
> fall back to long-poll."

(Tracking: admin-UI copy owned by B4-FE1.)

### Size / rate limits

- Telegram's bot API refuses file downloads larger than **20 MB**; the
  adapter surfaces this as `MediaError::TooLarge` and the route still
  returns 200 so Telegram stops retrying.
- `sendPhoto` and `sendVoice` use hand-rolled multipart boundaries —
  we don't enable reqwest's `multipart` feature. Replies that need
  richer file uploads (sticker / animation) will either extend the
  hand-rolled encoder or opt into the reqwest feature when the
  trade-off flips.
- No explicit outbound rate limit is enforced. Telegram itself rate-
  limits bots at ~30 msg/s globally and 1 msg/s per chat; stay under.
