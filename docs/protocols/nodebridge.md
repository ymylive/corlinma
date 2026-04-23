# NodeBridge protocol v1

**Spec version**: `1.0.0-alpha` — advertised in the `Registered` frame.
**Status**: reserved for future device clients. The gateway ships the
stub server (`corlinman-nodebridge`); no native iOS/Android/macOS app is
built from this repo.

## Purpose

NodeBridge lets remote device-class clients (iOS, Android, macOS, Linux,
Electron) register capabilities and receive dispatched jobs. Unlike
`WsTool` — tuned for server-class runners sharing a trust boundary with
the gateway — NodeBridge assumes short-lived, user-owned devices and
reserves signing as a forward-compat hook.

## Transport

JSON text frames over WebSocket at `/nodebridge/connect`, bound to
`config.nodebridge.listen` (default `127.0.0.1:18788`). JSON (not
protobuf) was picked because every mobile stack ships a first-class
JSON codec; the traffic profile (sparse jobs + low-rate telemetry) does
not justify forcing third-party integrators to run `protoc`.

## Versioning

`server_version` in `Registered` is a semver string. Breaking changes to
any frame bump major; additive fields (e.g. new variant, new optional
field) are additive minor. Clients should tolerate unknown fields.

## Messages

| Kind (wire `kind`) | Direction | Summary |
| --- | --- | --- |
| `register` | C→S | First frame. Advertises `node_id`, `node_type`, `capabilities[]`, `auth_token`, `version`, optional `signature`. |
| `registered` | S→C | Ack: `{node_id, server_version, heartbeat_secs}`. |
| `register_rejected` | S→C | `{code, message}` then close. Codes: `unsigned_registration`, `duplicate_node_id`, `protocol_violation`, `bad_frame`. |
| `heartbeat` | C→S | `{node_id, at_ms}`. Missed 3× → disconnect. |
| `ping` / `pong` | either | Liveness. `ping` must be answered by `pong`. |
| `dispatch_job` | S→C | `{job_id, job_kind, params, timeout_ms}`. Client must reply with `job_result`. |
| `job_result` | C→S | `{job_id, ok, payload}`. `ok=false` implies `payload.error`. |
| `telemetry` | C→S | `{node_id, metric, value, tags}`. Forwarded to `HookEvent::Telemetry`. |
| `shutdown` | S→C | `{reason}`, followed by close. |

Wire note: the enum discriminant is `kind`, so `DispatchJob`'s job-kind
field serialises as `job_kind` to avoid the collision.

## Flows

**Registration.** Client dials WS, sends `register`. Server validates:
if `signature == null` and `config.accept_unsigned == false`, replies
`register_rejected {code: "unsigned_registration"}` and closes.
Otherwise inserts the session (`node_id` unique) and replies
`registered`.

**Heartbeat.** Server opens a ticker at `heartbeat_secs` (15 by default)
and emits `ping` each tick. Any inbound frame — `heartbeat`, `pong`,
`job_result`, or `telemetry` — resets the miss counter. Three
consecutive misses → the session is evicted and the socket closed.

**Dispatch.** `dispatch_job` is fanned out to the first session whose
capabilities include `kind`. If none, the server returns
`NodeBridgeError::NoCapableNode` locally without touching the wire. If
no `job_result` arrives before `timeout_ms`, the server reports
`NodeBridgeError::Timeout` locally and clears its pending entry.

## Future work

- **Signing.** `Register.signature` is `Option<String>`. A future minor
  will define a canonicalisation over (`node_id`, `node_type`,
  `capabilities`, `auth_token`, `version`) + device attestation keys.
  `accept_unsigned` flips to `false` by default once signing ships.
- **Multi-tenant auth.** `auth_token` is currently a single shared
  secret. A future version will accept per-tenant tokens + OIDC.
- **Progress streaming.** Long-running jobs may gain a mid-flight
  `JobProgress` frame matching the WsTool pattern.
