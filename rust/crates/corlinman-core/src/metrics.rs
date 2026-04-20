//! Process-wide Prometheus registry shared by every corlinman-* crate.
//!
//! Lives here (the leaf crate every other crate depends on) so subsystems
//! like the plugin runtime, agent-client retry loop, and vector searcher
//! can observe into the same registry the gateway's `/metrics` handler
//! encodes. Before S7 these handles lived in `corlinman-gateway::metrics`
//! but that created a reverse-dependency problem for the three call-site
//! crates — hence the move. `corlinman-gateway::metrics` re-exports every
//! symbol declared here so existing metric names stay unchanged.
//!
//! Naming + labels are documented next to each metric. Keep it
//! low-cardinality — per-request ids belong in tracing spans.

use once_cell::sync::Lazy;
use prometheus::{CounterVec, HistogramOpts, HistogramVec, IntGauge, Opts, Registry};

/// Process-wide registry. All `Lazy` metrics below register into this.
pub static REGISTRY: Lazy<Registry> = Lazy::new(Registry::new);

/// `corlinman_http_requests_total{route, status}`.
pub static HTTP_REQUESTS: Lazy<CounterVec> = Lazy::new(|| {
    let cv = CounterVec::new(
        Opts::new("corlinman_http_requests_total", "Total HTTP requests"),
        &["route", "status"],
    )
    .expect("valid metric");
    REGISTRY
        .register(Box::new(cv.clone()))
        .expect("register http_requests");
    cv
});

/// `corlinman_chat_stream_duration_seconds{model, finish}`.
pub static CHAT_STREAM_DURATION: Lazy<HistogramVec> = Lazy::new(|| {
    let opts = HistogramOpts::new(
        "corlinman_chat_stream_duration_seconds",
        "End-to-end SSE chat stream duration",
    )
    .buckets(vec![
        0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0,
    ]);
    let hv = HistogramVec::new(opts, &["model", "finish"]).expect("valid metric");
    REGISTRY
        .register(Box::new(hv.clone()))
        .expect("register chat_stream_duration");
    hv
});

/// `corlinman_plugin_execute_total{plugin, status}`.
///
/// `status ∈ {"ok", "error", "timeout", "oom", "denied", "cancelled"}`.
pub static PLUGIN_EXECUTE_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    let cv = CounterVec::new(
        Opts::new("corlinman_plugin_execute_total", "Plugin tool invocations"),
        &["plugin", "status"],
    )
    .expect("valid metric");
    REGISTRY
        .register(Box::new(cv.clone()))
        .expect("register plugin_execute_total");
    cv
});

/// `corlinman_plugin_execute_duration_seconds{plugin}`.
pub static PLUGIN_EXECUTE_DURATION: Lazy<HistogramVec> = Lazy::new(|| {
    let opts = HistogramOpts::new(
        "corlinman_plugin_execute_duration_seconds",
        "Plugin tool invocation wall time",
    )
    .buckets(vec![
        0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0,
    ]);
    let hv = HistogramVec::new(opts, &["plugin"]).expect("valid metric");
    REGISTRY
        .register(Box::new(hv.clone()))
        .expect("register plugin_execute_duration");
    hv
});

/// `corlinman_backoff_retries_total{reason}`.
pub static BACKOFF_RETRIES: Lazy<CounterVec> = Lazy::new(|| {
    let cv = CounterVec::new(
        Opts::new(
            "corlinman_backoff_retries_total",
            "Retries performed by the agent-client backoff scheduler",
        ),
        &["reason"],
    )
    .expect("valid metric");
    REGISTRY
        .register(Box::new(cv.clone()))
        .expect("register backoff_retries");
    cv
});

/// `corlinman_agent_grpc_inflight` — active `Agent.Chat` streams.
pub static AGENT_GRPC_INFLIGHT: Lazy<IntGauge> = Lazy::new(|| {
    let g = IntGauge::new(
        "corlinman_agent_grpc_inflight",
        "In-flight Agent.Chat gRPC bidi streams",
    )
    .expect("valid metric");
    REGISTRY
        .register(Box::new(g.clone()))
        .expect("register agent_grpc_inflight");
    g
});

/// `corlinman_channels_rate_limited_total{channel, reason}`.
pub static CHANNELS_RATE_LIMITED: Lazy<CounterVec> = Lazy::new(|| {
    let cv = CounterVec::new(
        Opts::new(
            "corlinman_channels_rate_limited_total",
            "Inbound channel messages silently dropped by a rate-limit check",
        ),
        &["channel", "reason"],
    )
    .expect("valid metric");
    REGISTRY
        .register(Box::new(cv.clone()))
        .expect("register channels_rate_limited");
    cv
});

/// `corlinman_vector_query_duration_seconds{stage}` —
/// `stage ∈ {"hnsw", "bm25", "fuse", "rerank"}`.
pub static VECTOR_QUERY_DURATION: Lazy<HistogramVec> = Lazy::new(|| {
    let opts = HistogramOpts::new(
        "corlinman_vector_query_duration_seconds",
        "Hybrid vector query timing per stage",
    )
    .buckets(vec![
        0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5,
    ]);
    let hv = HistogramVec::new(opts, &["stage"]).expect("valid metric");
    REGISTRY
        .register(Box::new(hv.clone()))
        .expect("register vector_query_duration");
    hv
});

/// Encode the registry in Prometheus text-exposition v0.0.4 format.
pub fn encode() -> Vec<u8> {
    use prometheus::Encoder;
    let mut buf = Vec::new();
    let encoder = prometheus::TextEncoder::new();
    let _ = encoder.encode(&REGISTRY.gather(), &mut buf);
    buf
}

/// Eagerly touch every `Lazy` so names appear in `/metrics` even before
/// the first data point. See the gateway's wrapper `init()` for sentinel
/// label wiring.
pub fn init() {
    Lazy::force(&HTTP_REQUESTS);
    Lazy::force(&CHAT_STREAM_DURATION);
    Lazy::force(&PLUGIN_EXECUTE_TOTAL);
    Lazy::force(&PLUGIN_EXECUTE_DURATION);
    Lazy::force(&BACKOFF_RETRIES);
    Lazy::force(&AGENT_GRPC_INFLIGHT);
    Lazy::force(&CHANNELS_RATE_LIMITED);
    Lazy::force(&VECTOR_QUERY_DURATION);
}
