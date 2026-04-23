//! `POST /channels/telegram/webhook` — Telegram Bot webhook endpoint.
//!
//! Telegram calls this route with an `Update` JSON body whenever
//! `setWebhook` has been registered at `[telegram.webhook].public_url`.
//! The request is authenticated by the `X-Telegram-Bot-Api-Secret-Token`
//! header (compared against `[telegram.webhook].secret_token` in constant
//! time). Mismatch → 401; success → 200 with an empty body, then the
//! handler spawns the agent reply asynchronously so Telegram's delivery
//! retry window isn't blocked on upstream latency.
//!
//! State is a [`Arc<TelegramWebhookState>`] holding the HTTP client, bot
//! identity, hook bus, and secret. The state is constructed once at gateway
//! boot (see `main.rs`) and cloned into the route via axum's `State`
//! extractor. Hot-swapping the secret requires a gateway restart — the
//! admin tooltip (README note) surfaces that constraint.

use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use corlinman_channels::telegram::media::TelegramHttp;
use corlinman_channels::telegram::types::Update;
use corlinman_channels::telegram::webhook::{process_update, verify_secret, WebhookCtx};
use corlinman_hooks::HookBus;
use serde_json::{json, Value};

/// Header Telegram echoes back so the handler can authenticate inbound
/// requests. Reference: core.telegram.org/bots/api#setwebhook.
pub const SECRET_HEADER: &str = "x-telegram-bot-api-secret-token";

/// Shared state for the webhook route.
pub struct TelegramWebhookState {
    /// Expected secret (matches `[telegram.webhook].secret_token`). Empty
    /// string means the check is disabled — useful for local dev tunnels
    /// that strip headers. The route handler logs a warning in that case.
    pub secret_token: String,
    pub bot_id: i64,
    pub bot_username: Option<String>,
    pub data_dir: PathBuf,
    pub http: Arc<dyn TelegramHttp>,
    pub hooks: Option<HookBus>,
}

/// Axum handler for `POST /channels/telegram/webhook`.
///
/// Returns:
/// - 401 if the secret header doesn't match.
/// - 400 if the body isn't decodable as `Update`.
/// - 200 with `{"ok": true}` otherwise (non-message updates return 200
///   and are silently dropped so Telegram doesn't retry).
pub async fn telegram_webhook(
    State(state): State<Arc<TelegramWebhookState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let got = headers.get(SECRET_HEADER).and_then(|v| v.to_str().ok());
    if !verify_secret(&state.secret_token, got) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "unauthorized",
                "message": "X-Telegram-Bot-Api-Secret-Token mismatch",
            })),
        )
            .into_response();
    }

    let update: Update = match serde_json::from_value(body) {
        Ok(u) => u,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "invalid_update",
                    "message": err.to_string(),
                })),
            )
                .into_response();
        }
    };

    let ctx = WebhookCtx {
        bot_id: state.bot_id,
        bot_username: state.bot_username.as_deref(),
        data_dir: &state.data_dir,
        http: state.http.as_ref(),
        hooks: state.hooks.as_ref(),
    };

    match process_update(&ctx, update).await {
        Ok(_) => (StatusCode::OK, Json(json!({"ok": true}))).into_response(),
        Err(err) => {
            tracing::warn!(
                target: "corlinman.gateway.channels.telegram",
                error = %err,
                "webhook processing failed"
            );
            // Return 200 anyway — Telegram would otherwise retry the same
            // update indefinitely, which turns a transient media-download
            // hiccup into a thundering herd.
            (StatusCode::OK, Json(json!({"ok": false}))).into_response()
        }
    }
}

/// Build the webhook sub-router. Callers in `server.rs` / `main.rs` wire
/// this in after constructing [`TelegramWebhookState`] from config.
pub fn router_with_state(state: Arc<TelegramWebhookState>) -> Router {
    Router::new()
        .route("/channels/telegram/webhook", post(telegram_webhook))
        .with_state(state)
}
