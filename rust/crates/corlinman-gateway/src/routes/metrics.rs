//! `GET /metrics` — Prometheus scrape endpoint.
//!
//! Output is the standard text-exposition `v0.0.4` format so any Prometheus
//! scraper can consume it directly. Metric definitions + the registry live
//! in [`crate::metrics`].

use axum::{http::header, response::IntoResponse, routing::get, Router};

use crate::metrics;

pub fn router() -> Router {
    Router::new().route("/metrics", get(handler))
}

async fn handler() -> impl IntoResponse {
    let body = metrics::encode();
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn metrics_endpoint_returns_prometheus_body() {
        // Force a metric to be present in the registry.
        metrics::init();
        metrics::HTTP_REQUESTS
            .with_label_values(&["/metrics", "200"])
            .inc();

        let app = router();
        let req = Request::builder()
            .method("GET")
            .uri("/metrics")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        assert!(
            ct.starts_with("text/plain"),
            "unexpected content-type: {ct}"
        );
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            text.contains("corlinman_http_requests_total"),
            "missing metric family: {text}"
        );
    }
}
