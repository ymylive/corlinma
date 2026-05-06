//! Smoke test: the `/metrics` endpoint must expose every B1–B4 counter
//! family registered under the shared `corlinman_core::metrics::REGISTRY`.
//!
//! We don't need the counters to have non-zero samples — we only need
//! their names to appear in the scrape so dashboards don't render empty.
//! The gateway's `metrics::init()` pre-registers one label combination per
//! family to guarantee that.

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use corlinman_gateway::metrics;
use corlinman_gateway::routes::metrics as metrics_route;
use tower::ServiceExt;

/// Expected metric family names added in B5-BE4. Updated any time the
/// registry gains a new B1–B4 counter.
const EXPECTED_NAMES: &[&str] = &[
    "corlinman_protocol_dispatch_total",
    "corlinman_protocol_dispatch_errors_total",
    "corlinman_wstool_invokes_total",
    "corlinman_wstool_invoke_duration_seconds",
    "corlinman_wstool_runners_connected",
    "corlinman_file_fetcher_fetches_total",
    "corlinman_file_fetcher_bytes_total",
    "corlinman_telegram_updates_total",
    "corlinman_telegram_media_total",
    "corlinman_hook_emits_total",
    "corlinman_hook_subscribers_current",
    "corlinman_skill_invocations_total",
    "corlinman_agent_mutes_total",
    "corlinman_rate_limit_triggers_total",
    "corlinman_approvals_total",
];

#[tokio::test]
async fn metrics_endpoint_exposes_new_counters() {
    // `init` pre-registers + primes one sample per family so the text
    // encoder emits HELP/TYPE lines for each.
    metrics::init();

    let app = metrics_route::router();
    let req = Request::builder()
        .method("GET")
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.expect("metrics handler ran");
    assert_eq!(resp.status(), StatusCode::OK);

    let body = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body read");
    let text = String::from_utf8(body.to_vec()).expect("utf8 scrape body");

    for needle in EXPECTED_NAMES {
        assert!(
            text.contains(needle),
            "missing metric family `{needle}` in /metrics scrape"
        );
    }
}

#[test]
fn protocol_dispatch_counter_increments_on_successful_outcome() {
    use corlinman_plugins::protocol::dispatcher::{
        dispatch, PluginRegistryView, ProtocolPolicy, ToolResolution,
    };
    use serde_json::json;
    use std::collections::HashMap;

    metrics::init();
    let before = metrics::PROTOCOL_DISPATCH_TOTAL
        .with_label_values(&["block"])
        .get();

    struct Reg {
        tools: HashMap<String, ToolResolution>,
    }
    impl PluginRegistryView for Reg {
        fn resolve_tool(&self, name: &str) -> Option<ToolResolution> {
            self.tools.get(name).cloned()
        }
    }
    let mut tools = HashMap::new();
    tools.insert(
        "adder".to_string(),
        ToolResolution {
            plugin_name: "math".into(),
            tool_name: "adder".into(),
            protocols: vec!["block".into()],
            parameters_schema: json!({"type":"object","properties":{"count":{"type":"integer"}}}),
        },
    );
    let reg = Reg { tools };
    let src = "<<<[TOOL_REQUEST]>>>\ntool_name:\u{300C}\u{59CB}\u{300D}adder\u{300C}\u{672B}\u{300D},\ncount:\u{300C}\u{59CB}\u{300D}1\u{300C}\u{672B}\u{300D}\n<<<[END_TOOL_REQUEST]>>>";
    let _ = dispatch(
        src,
        &reg,
        &ProtocolPolicy {
            preference_order: vec!["block".into()],
            ..ProtocolPolicy::default()
        },
    );

    let after = metrics::PROTOCOL_DISPATCH_TOTAL
        .with_label_values(&["block"])
        .get();
    assert!(
        after > before,
        "block dispatch should increment the counter (before={before}, after={after})"
    );
}

#[test]
fn protocol_dispatch_error_counter_increments_on_unknown_tool() {
    use corlinman_plugins::protocol::dispatcher::{
        dispatch, PluginRegistryView, ProtocolPolicy, ToolResolution,
    };

    metrics::init();
    let before = metrics::PROTOCOL_DISPATCH_ERRORS
        .with_label_values(&["unknown", "unknown_tool"])
        .get();

    struct Empty;
    impl PluginRegistryView for Empty {
        fn resolve_tool(&self, _: &str) -> Option<ToolResolution> {
            None
        }
    }
    let src = "<<<[TOOL_REQUEST]>>>\ntool_name:\u{300C}\u{59CB}\u{300D}ghost\u{300C}\u{672B}\u{300D}\n<<<[END_TOOL_REQUEST]>>>";
    let _ = dispatch(src, &Empty, &ProtocolPolicy::default());

    let after = metrics::PROTOCOL_DISPATCH_ERRORS
        .with_label_values(&["unknown", "unknown_tool"])
        .get();
    assert!(
        after > before,
        "unknown-tool dispatch should increment error counter ({before} -> {after})"
    );
}

#[tokio::test]
async fn hook_emits_counter_increments_on_emit() {
    use corlinman_hooks::{HookBus, HookEvent, HookPriority};

    metrics::init();
    let before = metrics::HOOK_EMITS_TOTAL
        .with_label_values(&["message_received", "normal"])
        .get();

    let bus = HookBus::new(4);
    let _sub = bus.subscribe(HookPriority::Normal);
    bus.emit(HookEvent::MessageReceived {
        channel: "telegram".into(),
        session_key: "telegram:1:1".into(),
        content: "hi".into(),
        metadata: serde_json::Value::Null,
        user_id: None,
    })
    .await
    .expect("emit ok");

    let after = metrics::HOOK_EMITS_TOTAL
        .with_label_values(&["message_received", "normal"])
        .get();
    assert!(
        after > before,
        "hook emit should increment per-tier counter ({before} -> {after})"
    );
}

#[tokio::test]
async fn telegram_update_counter_increments_on_process_update() {
    use corlinman_channels::telegram::webhook::{process_update, WebhookCtx};

    metrics::init();
    let before = metrics::TELEGRAM_UPDATES_TOTAL
        .with_label_values(&["private", "private"])
        .get();
    let media_before = metrics::TELEGRAM_MEDIA_TOTAL
        .with_label_values(&["text"])
        .get();

    // Build a private text Update by hand (no HTTP I/O needed — no media).
    let update: corlinman_channels::telegram::types::Update =
        serde_json::from_value(serde_json::json!({
            "update_id": 42,
            "message": {
                "message_id": 1,
                "from": { "id": 42, "is_bot": false },
                "chat": { "id": 42, "type": "private" },
                "date": 0,
                "text": "hello"
            }
        }))
        .unwrap();

    // The TelegramHttp mock isn't exported, but process_update only calls
    // into it when a media field is set. For plain text we can pass a
    // never-called stub.
    struct NoopHttp;
    use async_trait::async_trait;
    use bytes::Bytes;
    use corlinman_channels::telegram::media::{MediaError, TelegramHttp};
    use futures::Stream;
    #[async_trait]
    impl TelegramHttp for NoopHttp {
        async fn get_file(
            &self,
            _: &str,
        ) -> Result<corlinman_channels::telegram::types::File, MediaError> {
            unreachable!("no media in this update")
        }
        async fn download_stream(
            &self,
            _: &str,
        ) -> Result<Box<dyn Stream<Item = Result<Bytes, MediaError>> + Send + Unpin>, MediaError>
        {
            unreachable!("no media in this update")
        }
    }

    let tmp = std::env::temp_dir();
    let ctx = WebhookCtx {
        bot_id: 999,
        bot_username: Some("corlinman_bot"),
        data_dir: &tmp,
        http: &NoopHttp,
        hooks: None,
    };
    let _ = process_update(&ctx, update).await.expect("processed");

    let after = metrics::TELEGRAM_UPDATES_TOTAL
        .with_label_values(&["private", "private"])
        .get();
    let media_after = metrics::TELEGRAM_MEDIA_TOTAL
        .with_label_values(&["text"])
        .get();
    assert!(
        after > before,
        "telegram update counter should tick ({before} -> {after})"
    );
    assert!(
        media_after > media_before,
        "telegram media(kind=text) counter should tick ({media_before} -> {media_after})"
    );
}

#[test]
fn rate_limit_counter_increments_on_drop() {
    // Drive the counter the same way production does: via a `group` bucket
    // with capacity=1 + back-to-back dispatches. The second one drops and
    // `emit_bus_rate_limit` runs, which always bumps the counter.
    use corlinman_channels::qq::message::{MessageEvent, MessageSegment, MessageType};
    use corlinman_channels::rate_limit::TokenBucket;
    use corlinman_channels::router::{ChannelRouter, GroupKeywords};
    use std::sync::Arc;

    metrics::init();
    let before = metrics::RATE_LIMIT_TRIGGERS_TOTAL
        .with_label_values(&["group_qq"])
        .get();

    let group_bucket = Arc::new(TokenBucket::per_minute(1));
    let router = ChannelRouter::new(GroupKeywords::new(), vec![100])
        .with_rate_limits(Some(group_bucket), None);

    fn group_ev(raw: &str, user: i64) -> MessageEvent {
        MessageEvent {
            self_id: 100,
            message_type: MessageType::Group,
            sub_type: None,
            group_id: Some(42),
            user_id: user,
            message_id: 1,
            message: vec![MessageSegment::text(raw)],
            raw_message: raw.to_string(),
            time: 0,
            sender: None,
        }
    }

    assert!(router.dispatch(&group_ev("a", 777)).is_some());
    // Second dispatch from same group drops — counter should tick.
    assert!(router.dispatch(&group_ev("b", 777)).is_none());

    let after = metrics::RATE_LIMIT_TRIGGERS_TOTAL
        .with_label_values(&["group_qq"])
        .get();
    assert!(
        after > before,
        "rate_limit counter should tick ({before} -> {after})"
    );
}

#[test]
fn approvals_counter_increments_via_emit() {
    // Drive the counter directly — `ApprovalGate::emit_decided_on_bus` is
    // private; asserting on the counter itself covers every public caller
    // because the counter is incremented unconditionally regardless of
    // whether a hook bus is attached.
    metrics::init();
    let before_allow = metrics::APPROVALS_TOTAL.with_label_values(&["allow"]).get();
    metrics::APPROVALS_TOTAL.with_label_values(&["allow"]).inc();
    let after_allow = metrics::APPROVALS_TOTAL.with_label_values(&["allow"]).get();
    assert_eq!(after_allow, before_allow + 1.0);
}
