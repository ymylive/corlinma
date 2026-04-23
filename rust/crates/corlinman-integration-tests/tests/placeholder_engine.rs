//! Test 2 — `placeholder_engine_resolves_and_guards`.
//!
//! Wire a `PlaceholderEngine` with a test resolver on the `session` namespace
//! and assert:
//!   - straightforward substitution works (`{{session.user_id}}` -> `alice`),
//!   - a self-referential cycle via resolver-returned placeholders is caught
//!     by either cycle detection or the depth guard — either is acceptable
//!     because both are bounded and terminate.

use std::sync::Arc;

use async_trait::async_trait;
use corlinman_core::placeholder::{
    DynamicResolver, PlaceholderCtx, PlaceholderEngine, PlaceholderError,
};
use corlinman_core::CorlinmanError;

/// Resolver exposing three session-scoped keys. `user_id` is terminal; the
/// two `loop_*` keys point at each other so recursive expansion must bail.
struct SessionResolver;

#[async_trait]
impl DynamicResolver for SessionResolver {
    async fn resolve(&self, key: &str, _ctx: &PlaceholderCtx) -> Result<String, PlaceholderError> {
        match key {
            "user_id" => Ok("alice".to_string()),
            "loop_a" => Ok("{{session.loop_b}}".to_string()),
            "loop_b" => Ok("{{session.loop_a}}".to_string()),
            other => Err(PlaceholderError::Resolver {
                namespace: "session".into(),
                message: format!("unknown key: {other}"),
            }),
        }
    }
}

#[tokio::test]
async fn placeholder_engine_resolves_and_guards() {
    let mut eng = PlaceholderEngine::new();
    eng.register_namespace("session", Arc::new(SessionResolver));

    let ctx = PlaceholderCtx::new("sess-1");

    // Happy path.
    let out = eng.render("hi {{session.user_id}}", &ctx).await.unwrap();
    assert_eq!(out, "hi alice");

    // Cycle: must either detect the cycle or bail on depth, never run forever.
    let err = eng
        .render("echo {{session.loop_a}}", &ctx)
        .await
        .expect_err("recursive resolver must not succeed");
    match err {
        CorlinmanError::Parse { what, message } => {
            assert_eq!(what, "placeholder");
            let is_cycle = message.contains("cycle");
            let is_depth = message.contains("recursion depth");
            assert!(
                is_cycle || is_depth,
                "expected cycle or depth error, got: {message}"
            );
        }
        other => panic!("expected CorlinmanError::Parse, got {other:?}"),
    }
}

#[tokio::test]
async fn placeholder_engine_unknown_token_passes_through() {
    // Cross-check: unknown keys in a non-registered namespace must be left
    // verbatim rather than erroring. This anchors the "typo-friendly" contract
    // documented on the engine.
    let eng = PlaceholderEngine::new();
    let ctx = PlaceholderCtx::new("sess-x");
    let out = eng
        .render("raw: {{nope.missing}}", &ctx)
        .await
        .expect("unknown tokens must not error");
    assert_eq!(out, "raw: {{nope.missing}}");
}
