//! Tracing middleware: inject request_id / traceparent; open subsystem span.
//!
//! # M7 slice
//!
//! The span / traceparent wiring still lands in a later milestone; what's
//! implemented now is the `corlinman_http_requests_total{route,status}`
//! counter — recorded in [`record_metrics`] and composed as an axum
//! middleware via [`layer`].
//
// TODO: read `traceparent` header (W3C), generate if absent; bind into
//       `tracing::Span` fields {request_id, subsystem, route, method}.
// TODO: propagate outbound via gRPC metadata so Python structlog sees the same trace id.

use axum::{
    extract::{MatchedPath, Request},
    middleware::{from_fn, Next},
    response::Response,
    Router,
};

use crate::metrics::HTTP_REQUESTS;

/// Attach HTTP request metrics to every route on `router`.
pub fn layer(router: Router) -> Router {
    router.layer(from_fn(record_metrics))
}

/// Record one `corlinman_http_requests_total` sample per response.
///
/// `route` uses axum's matched path (`/v1/chat/completions`, `/metrics`, ...)
/// so we keep cardinality bounded; unmatched requests fall back to the raw
/// URI path.
pub async fn record_metrics(req: Request, next: Next) -> Response {
    let route: String = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());
    let response = next.run(req).await;
    let status = response.status().as_u16().to_string();
    HTTP_REQUESTS
        .with_label_values(&[route.as_str(), status.as_str()])
        .inc();
    response
}
