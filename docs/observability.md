# Observability

This document catalogues the tracing spans and Prometheus metrics exposed
by the corlinman platform after B5-BE4. It is intended as a quick
reference when debugging production issues or extending dashboards.

## Scrape endpoint

Prometheus scrapes `GET /metrics` on the gateway (default port `6005`).
The endpoint emits text-exposition v0.0.4 and is served by
`corlinman-gateway::routes::metrics`. Metric definitions live in
`corlinman-core::metrics` and are re-exported by `corlinman-gateway::metrics`
so every subcrate registers into the same `prometheus::Registry`.

## Metric families

### Existing families (pre-B5-BE4)

| Metric | Type | Labels |
|---|---|---|
| `corlinman_http_requests_total` | counter | `route`, `status` |
| `corlinman_chat_stream_duration_seconds` | histogram | `model`, `finish` |
| `corlinman_plugin_execute_total` | counter | `plugin`, `status` |
| `corlinman_plugin_execute_duration_seconds` | histogram | `plugin` |
| `corlinman_backoff_retries_total` | counter | `reason` |
| `corlinman_agent_grpc_inflight` | gauge | — |
| `corlinman_channels_rate_limited_total` | counter | `channel`, `reason` |
| `corlinman_vector_query_duration_seconds` | histogram | `stage` |

### B1–B4 additions

| Metric | Type | Labels |
|---|---|---|
| `corlinman_protocol_dispatch_total` | counter | `protocol` |
| `corlinman_protocol_dispatch_errors_total` | counter | `protocol`, `code` |
| `corlinman_wstool_invokes_total` | counter | `tool`, `ok` |
| `corlinman_wstool_invoke_duration_seconds` | histogram | `tool` |
| `corlinman_wstool_runners_connected` | gauge | — |
| `corlinman_file_fetcher_fetches_total` | counter | `scheme`, `ok` |
| `corlinman_file_fetcher_bytes_total` | counter | `scheme` |
| `corlinman_telegram_updates_total` | counter | `chat_type`, `mention_reason` |
| `corlinman_telegram_media_total` | counter | `kind` |
| `corlinman_hook_emits_total` | counter | `event_kind`, `priority` |
| `corlinman_hook_subscribers_current` | gauge | `priority` |
| `corlinman_skill_invocations_total` | counter | `skill_name` |
| `corlinman_agent_mutes_total` | counter | `expanded_agent` |
| `corlinman_rate_limit_triggers_total` | counter | `limit_type` |
| `corlinman_approvals_total` | counter | `decision` |

Label cardinality is kept bounded:
- `protocol` ∈ `{block, openai_function, unknown}`
- `code` ∈ `{unknown_tool, protocol_not_advertised, parse, coercion}`
- `ok` ∈ `{true, false}`
- `scheme` ∈ `{file, http, https, ws-tool, other}`
- `priority` ∈ `{critical, normal, low}`
- `kind` ∈ `{photo, voice, document, text}`
- `decision` ∈ `{allow, deny, timeout}`
- `mention_reason` ∈ `{private, group_addressed, group_ignored}`
- `limit_type` ∈ `{<reason>_<channel>}` — keep channels bounded

## Tracing spans

| Span | Crate::module | Fields |
|---|---|---|
| `hook_emit` | `corlinman-hooks::bus` | `event_kind`, `session_key`, `priority_tier_count` |
| `placeholder_render` | `corlinman-core::placeholder` | `template_len`, `depth_used`, `unresolved_count` |
| `protocol_dispatch` | `corlinman-plugins::protocol::dispatcher` | `outcomes_count`, `block_count`, `fc_count` |
| `block_parse` | `corlinman-plugins::protocol::block` | `envelope_count`, `error_count` |
| `wstool_invoke` | `corlinman-wstool::runtime` | `tool`, `runner_id`, `duration_ms`, `ok` |
| `file_fetch` | `corlinman-wstool::file_fetcher` | `uri_scheme`, `total_bytes`, `ok` |
| `telegram_webhook` | `corlinman-channels::telegram::webhook` | `chat_type`, `mention_reason`, `media_kind` |
| `epa_backfill` (structlog event) | `corlinman_agent.rag.epa_backfill` | `chunks_processed`, `basis_axes`, `wall_clock_s`, `chunks_skipped`, `namespaces_touched`, `namespace`, `status` |

All Rust spans are emitted via `tracing`. The gateway forwards them to an
OTLP collector when `CORLINMAN_OTEL_ENDPOINT` is set (see
`corlinman-gateway::telemetry`).

## Common queries

### PromQL

- Protocol dispatch QPS:
  ```promql
  sum by(protocol) (rate(corlinman_protocol_dispatch_total[1m]))
  ```
- WsTool p99 latency (global):
  ```promql
  histogram_quantile(0.99, sum by(le) (rate(corlinman_wstool_invoke_duration_seconds_bucket[5m])))
  ```
- FileFetcher bytes/s by scheme:
  ```promql
  sum by(scheme) (rate(corlinman_file_fetcher_bytes_total[1m]))
  ```
- Approval allow ratio:
  ```promql
  sum(rate(corlinman_approvals_total{decision="allow"}[5m]))
  / sum(rate(corlinman_approvals_total[5m]))
  ```
- Dispatch error breakdown:
  ```promql
  sum by(code) (rate(corlinman_protocol_dispatch_errors_total[5m]))
  ```
- Hook emit rate by kind:
  ```promql
  sum by(event_kind) (rate(corlinman_hook_emits_total[1m]))
  ```
- Rate-limit drops by limit_type:
  ```promql
  sum by(limit_type) (rate(corlinman_rate_limit_triggers_total[5m]))
  ```

### Tracing filters

With `tracing-subscriber` + `EnvFilter` the gateway honours `RUST_LOG`:

- Focus on a single span family:
  ```bash
  RUST_LOG="info,corlinman_plugins::protocol=debug"
  ```
- Drill into WsTool timing:
  ```bash
  RUST_LOG="info,corlinman_wstool::runtime=debug"
  ```
- Quiet everything except hook emits:
  ```bash
  RUST_LOG="warn,corlinman_hooks::bus=info"
  ```

With an OTLP collector attached, the same field names (`tool`,
`runner_id`, `duration_ms`, `event_kind`, ...) are queryable in Tempo /
Jaeger as span attributes.

## Pointing an OTel collector at the gateway

The gateway initialises a tracer provider when `CORLINMAN_OTEL_ENDPOINT`
is set. Minimal setup with `docker-compose`:

```yaml
# ops/docker-compose.observability.yml (excerpt)
services:
  otel-collector:
    image: otel/opentelemetry-collector-contrib:latest
    command: ["--config=/etc/otelcol/config.yaml"]
    volumes:
      - ./otel-collector.yaml:/etc/otelcol/config.yaml
    ports:
      - "4317:4317"   # OTLP gRPC (tracing)
      - "4318:4318"   # OTLP HTTP

  corlinman-gateway:
    environment:
      CORLINMAN_OTEL_ENDPOINT: "http://otel-collector:4317"
```

Collector config routes traces to Tempo/Jaeger and leaves metrics to
Prometheus (which scrapes `/metrics` directly — no OTLP metric export is
configured).

## Grafana dashboard

`ops/dashboards/corlinman.json` — import into Grafana 10+, wire the
`DS_PROMETHEUS` input to a Prometheus datasource that scrapes the
gateway. Dashboard UID: `corlinman-gateway`.
