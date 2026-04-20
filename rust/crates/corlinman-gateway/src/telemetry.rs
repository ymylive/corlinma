//! OpenTelemetry OTLP exporter wiring for the gateway.
//!
//! # Contract
//!
//! - Activated only when `OTEL_EXPORTER_OTLP_ENDPOINT` is set. Unset → every
//!   function here returns `None` and tracing falls back to the existing
//!   stdout / broadcast layers.
//! - Failing init is warn-and-continue. The gateway must never refuse to
//!   start because a collector is unreachable.
//! - Service name is `corlinman-gateway` unless `OTEL_SERVICE_NAME`
//!   overrides. The service version matches the crate version.
//! - Exporter protocol is gRPC (tonic). The collector endpoint is read
//!   verbatim from `OTEL_EXPORTER_OTLP_ENDPOINT` (e.g.
//!   `http://localhost:4317`).
//!
//! # Layering
//!
//! The caller (`main.rs::init_tracing`) composes a `tracing_subscriber`
//! registry with the JSON stdout layer + log broadcast layer it already
//! builds, plus this module's [`otel_layer`] when enabled. That way the
//! gateway continues to function if OTLP is disabled.
//!
//! # Propagation
//!
//! The global text-map propagator is set to W3C TraceContext so outgoing
//! gRPC requests carry `traceparent`. The tonic client interceptor that
//! injects the header lives in the agent-client crate (see
//! `corlinman_agent_client::trace_propagate`).

use std::env;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::Tracer;
use opentelemetry_sdk::Resource;

/// Try to configure an OTLP exporter from the environment.
///
/// Returns `Some(tracer)` when `OTEL_EXPORTER_OTLP_ENDPOINT` is set and
/// the exporter pipeline initialises cleanly, `None` otherwise. Errors
/// downgrade to a `tracing::warn` and `None` so boot continues.
pub fn try_init_tracer() -> Option<Tracer> {
    let endpoint = env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok()?;
    if endpoint.trim().is_empty() {
        return None;
    }

    // Install the W3C propagator *before* building the pipeline so any
    // async work spawned during init still gets traceparent injection.
    global::set_text_map_propagator(TraceContextPropagator::new());

    let service_name =
        env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "corlinman-gateway".to_string());
    let resource = Resource::new(vec![
        KeyValue::new("service.name", service_name),
        KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
    ]);

    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(endpoint.clone());

    let pipeline = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(opentelemetry_sdk::trace::Config::default().with_resource(resource))
        .install_batch(opentelemetry_sdk::runtime::Tokio);

    match pipeline {
        Ok(provider) => {
            tracing::info!(endpoint = %endpoint, "otlp tracer initialised");
            let tracer = provider.tracer("corlinman-gateway");
            // Install globally so other crates can obtain tracers too.
            global::set_tracer_provider(provider);
            Some(tracer)
        }
        Err(err) => {
            tracing::warn!(endpoint = %endpoint, error = %err, "otlp init failed; continuing");
            None
        }
    }
}

/// Best-effort flush + shutdown of the global tracer provider. Safe to
/// call even when [`try_init_tracer`] was never invoked.
pub fn shutdown() {
    global::shutdown_tracer_provider();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_noop_without_endpoint() {
        // SAFETY: test-local env mutation, acceptable risk.
        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        }
        assert!(try_init_tracer().is_none());
    }

    #[test]
    fn empty_endpoint_is_noop() {
        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "   ");
        }
        assert!(try_init_tracer().is_none());
        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        }
    }
}
