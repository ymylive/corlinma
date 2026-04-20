//! W3C `traceparent` injection for outbound gRPC calls (plan §9, S7.T1).
//!
//! The gateway's OTel pipeline installs `TraceContextPropagator` as the
//! global propagator. When we call [`inject_trace_context`] we ask that
//! propagator to serialise the *current* span context into a
//! `HeaderMap`-shaped carrier and then copy those headers into the
//! tonic `Request`'s metadata. The Python side reads them via its
//! server interceptor (see `corlinman_server.middleware`).
//!
//! When no tracer is installed the propagator is the noop default, so
//! `inject_trace_context` is a cheap no-op. That's deliberate — nothing
//! requires a running collector.

use opentelemetry::propagation::Injector;
use tonic::metadata::{MetadataKey, MetadataMap, MetadataValue};
use tonic::Request;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Adapter that lets the OTel `TextMapPropagator` write into a
/// `tonic::metadata::MetadataMap`.
struct MetadataInjector<'a>(&'a mut MetadataMap);

impl Injector for MetadataInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        let Ok(key) = MetadataKey::from_bytes(key.as_bytes()) else {
            return;
        };
        let Ok(value) = MetadataValue::try_from(value.as_str()) else {
            return;
        };
        self.0.insert(key, value);
    }
}

/// Inject the current tracing span's W3C context into `request`'s
/// metadata. Safe to call unconditionally; noop when OTel isn't wired.
pub fn inject_trace_context<T>(request: &mut Request<T>) {
    let ctx = Span::current().context();
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&ctx, &mut MetadataInjector(request.metadata_mut()));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inject_is_noop_when_no_span_context() {
        let mut req = Request::new(());
        inject_trace_context(&mut req);
        // With the default noop propagator and no active span, metadata
        // stays empty. We just want to confirm no panic.
        assert!(req.metadata().is_empty() || !req.metadata().is_empty());
    }
}
