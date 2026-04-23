//! Telegram webhook handler + signature validation.
//!
//! Wire:
//! ```text
//! Telegram ── POST /channels/telegram/webhook ──► gateway
//!                    X-Telegram-Bot-Api-Secret-Token: <configured>
//!                    body = Update JSON
//! ```
//!
//! Responsibility split:
//! - `verify_secret`: constant-time compare of the incoming header to the
//!   configured secret. Mismatch → caller returns 401.
//! - `process_update`: drives the full pipeline (media download → hook
//!   emission → session key build) from a decoded [`Update`]. Kept as a
//!   free function so the gateway route handler stays thin and unit tests
//!   don't need to spin up axum.
//!
//! All I/O goes through the [`TelegramHttp`] trait (for media downloads)
//! and the [`HookBus`] so production wiring and tests share one code path.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use corlinman_hooks::{HookBus, HookEvent};
use serde_json::json;

use super::media::{download_to_media_dir, DownloadedMedia, MediaError, TelegramHttp};
use super::types::{classify, session_key_for, Message, MessageRoute, Update};

/// Constant-time secret comparison. Telegram echoes back the configured
/// `secret_token` in the `X-Telegram-Bot-Api-Secret-Token` request header;
/// a mismatch (or absence, when a secret is configured) → 401.
///
/// When `configured` is empty string the check is disabled (useful for
/// local dev with a tunnel that strips headers) — callers should log a
/// warning at startup in that case.
pub fn verify_secret(configured: &str, got: Option<&str>) -> bool {
    if configured.is_empty() {
        return true;
    }
    let got = got.unwrap_or("");
    if got.len() != configured.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (a, b) in configured.as_bytes().iter().zip(got.as_bytes().iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Runtime context passed to [`process_update`] so the function remains
/// trait-object friendly and easy to mock in tests.
pub struct WebhookCtx<'a> {
    pub bot_id: i64,
    pub bot_username: Option<&'a str>,
    pub data_dir: &'a Path,
    pub http: &'a (dyn TelegramHttp + Send + Sync),
    pub hooks: Option<&'a HookBus>,
}

/// Outcome of processing one `Update`. Returned so the gateway route can
/// decide whether to trigger an agent reply without re-parsing.
#[derive(Debug, Clone)]
pub struct ProcessedUpdate {
    pub update_id: i64,
    pub session_key: String,
    pub route: MessageRoute,
    pub content: String,
    /// Populated when the message carried a media attachment that was
    /// successfully downloaded. `None` for plain-text messages or when
    /// the download failed (error is logged; the update still flows).
    pub media: Option<DownloadedMedia>,
    /// `photo` | `voice` | `document` | `text`. Preserves the media kind
    /// across the hook emission for downstream consumers.
    pub media_kind: &'static str,
}

/// Drive the full pipeline for one webhook `Update`. Non-message updates
/// (edited_message, callback_query, channel_post, ...) are quietly ignored
/// — returning `Ok(None)` so the route still responds 200 and Telegram
/// doesn't re-deliver them.
pub async fn process_update(
    ctx: &WebhookCtx<'_>,
    update: Update,
) -> Result<Option<ProcessedUpdate>, WebhookError> {
    use tracing::Instrument;

    let Some(msg) = update.message else {
        return Ok(None);
    };

    let route = classify(&msg, ctx.bot_id, ctx.bot_username);
    let session_key = session_key_for(&msg);

    let span = tracing::info_span!(
        "telegram_webhook",
        chat_type = msg.chat.chat_type.as_str(),
        mention_reason = mention_reason_label(route),
        media_kind = tracing::field::Empty,
    );
    // Keep the span active across all awaits by instrumenting the body.
    corlinman_core::metrics::TELEGRAM_UPDATES_TOTAL
        .with_label_values(&[msg.chat.chat_type.as_str(), mention_reason_label(route)])
        .inc();
    process_update_body(ctx, msg, route, session_key, update.update_id)
        .instrument(span)
        .await
}

async fn process_update_body(
    ctx: &WebhookCtx<'_>,
    msg: Message,
    route: MessageRoute,
    session_key: String,
    update_id: i64,
) -> Result<Option<ProcessedUpdate>, WebhookError> {
    // Pick the first media attachment present; photo/voice/document are
    // modelled as optional fields on Message so at most one is present in
    // practice for user-authored messages. Process in that order to match
    // Telegram's own precedence.
    let (media_kind, file_id, fallback_ext): (&'static str, Option<String>, &str) =
        if let Some(photo) = msg.largest_photo() {
            ("photo", Some(photo.file_id.clone()), "jpg")
        } else if let Some(voice) = msg.voice.as_ref() {
            ("voice", Some(voice.file_id.clone()), "ogg")
        } else if let Some(doc) = msg.document.as_ref() {
            ("document", Some(doc.file_id.clone()), "bin")
        } else {
            ("text", None, "")
        };

    tracing::Span::current().record("media_kind", media_kind);
    corlinman_core::metrics::TELEGRAM_MEDIA_TOTAL
        .with_label_values(&[media_kind])
        .inc();

    let media = if let Some(fid) = file_id.as_deref() {
        match download_to_media_dir(ctx.http, fid, ctx.data_dir, fallback_ext).await {
            Ok(m) => Some(m),
            Err(err) => {
                tracing::warn!(
                    target: "corlinman.channels.telegram.webhook",
                    error = %err,
                    file_id = fid,
                    "telegram media download failed"
                );
                None
            }
        }
    } else {
        None
    };

    let content = msg.text.clone().unwrap_or_default();

    // Fire hooks.
    if let Some(bus) = ctx.hooks {
        let meta = build_metadata(&msg, route, media.as_ref(), media_kind);
        let _ = bus
            .emit(HookEvent::MessageReceived {
                channel: "telegram".to_string(),
                session_key: session_key.clone(),
                content: content.clone(),
                metadata: meta,
            })
            .await;

        if media_kind == "voice" {
            let media_path = media
                .as_ref()
                .map(|m| m.path.to_string_lossy().into_owned())
                .unwrap_or_default();
            let _ = bus
                .emit(HookEvent::MessageTranscribed {
                    session_key: session_key.clone(),
                    // Real STT lands in a later batch — stub for now so the
                    // hook shape is wired and subscribers can build against it.
                    transcript: String::new(),
                    media_path,
                    media_type: "voice".to_string(),
                })
                .await;
        }
    }

    Ok(Some(ProcessedUpdate {
        update_id,
        session_key,
        route,
        content,
        media,
        media_kind,
    }))
}

/// Low-cardinality mention-reason label for metrics + span fields. Keeps
/// the dimension fixed at three values regardless of future `MessageRoute`
/// variants.
fn mention_reason_label(route: MessageRoute) -> &'static str {
    match route {
        MessageRoute::Private => "private",
        MessageRoute::GroupAddressed => "group_addressed",
        MessageRoute::GroupIgnored => "group_ignored",
    }
}

/// Build the JSON metadata payload attached to `HookEvent::MessageReceived`.
fn build_metadata(
    msg: &Message,
    route: MessageRoute,
    media: Option<&DownloadedMedia>,
    media_kind: &'static str,
) -> serde_json::Value {
    let is_group = route.is_group();
    let mentions_bot = matches!(route, MessageRoute::GroupAddressed);
    let mut meta = json!({
        "is_group": is_group,
        "chat_type": msg.chat.chat_type,
        "mentions_bot": mentions_bot,
        "media_kind": media_kind,
    });
    if is_group {
        meta["group_id"] = msg.chat.id.to_string().into();
    }
    if let Some(m) = media {
        meta["media_path"] = m.path.to_string_lossy().to_string().into();
        meta["media_bytes"] = (m.bytes as i64).into();
    }
    meta
}

#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("media error: {0}")]
    Media(#[from] MediaError),
    #[error("decode error: {0}")]
    Decode(String),
}

/// Compute the default data-dir-scoped media directory.
/// Kept public so the gateway boot path can `mkdir -p` it eagerly and
/// surface permission errors before the first webhook arrives.
pub fn default_media_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("media").join("telegram")
}

/// Owned variant of [`WebhookCtx`] used when passing the handler into an
/// `Arc` for axum state. The gateway glue crate constructs this once at
/// boot; tests use the `&'a` variant directly.
pub struct WebhookContext {
    pub bot_id: i64,
    pub bot_username: Option<String>,
    pub data_dir: PathBuf,
    pub http: Arc<dyn TelegramHttp>,
    pub hooks: Option<HookBus>,
    pub secret_token: String,
}

// ============================================================================
// Tests — see also `tests` submodule inside `media.rs` for the FakeHttp.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bytes::Bytes;
    use futures_util::stream::Stream;
    use std::sync::Mutex;

    /// Locally-scoped fake (the one in `media::tests` is private).
    struct FakeHttp {
        file_path: Option<String>,
        bytes: Vec<u8>,
        get_file_calls: Mutex<Vec<String>>,
    }

    impl FakeHttp {
        fn new(path: &str, bytes: Vec<u8>) -> Self {
            Self {
                file_path: Some(path.into()),
                bytes,
                get_file_calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.get_file_calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl TelegramHttp for FakeHttp {
        async fn get_file(&self, file_id: &str) -> Result<super::super::types::File, MediaError> {
            self.get_file_calls
                .lock()
                .unwrap()
                .push(file_id.to_string());
            Ok(super::super::types::File {
                file_id: file_id.into(),
                file_unique_id: Some(format!("u_{file_id}")),
                file_size: Some(self.bytes.len() as i64),
                file_path: self.file_path.clone(),
            })
        }

        async fn download_stream(
            &self,
            _file_path: &str,
        ) -> Result<Box<dyn Stream<Item = Result<Bytes, MediaError>> + Send + Unpin>, MediaError>
        {
            use futures_util::stream;
            let bytes = Bytes::copy_from_slice(&self.bytes);
            Ok(Box::new(Box::pin(stream::iter(vec![Ok::<_, MediaError>(
                bytes,
            )]))))
        }
    }

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let mut p = std::env::temp_dir();
            p.push(format!("tg-wh-{}", uuid::Uuid::new_v4().simple()));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn update_private_text(text: &str) -> Update {
        serde_json::from_value(json!({
            "update_id": 10,
            "message": {
                "message_id": 1,
                "from": { "id": 42, "is_bot": false, "username": "alice" },
                "chat": { "id": 42, "type": "private" },
                "date": 0,
                "text": text,
            }
        }))
        .unwrap()
    }

    fn update_group_plain(text: &str) -> Update {
        serde_json::from_value(json!({
            "update_id": 11,
            "message": {
                "message_id": 2,
                "from": { "id": 77, "is_bot": false },
                "chat": { "id": -100, "type": "supergroup", "title": "room" },
                "date": 0,
                "text": text,
            }
        }))
        .unwrap()
    }

    fn update_group_mention(text: &str) -> Update {
        serde_json::from_value(json!({
            "update_id": 12,
            "message": {
                "message_id": 3,
                "from": { "id": 77, "is_bot": false },
                "chat": { "id": -100, "type": "supergroup" },
                "date": 0,
                "text": text,
                "entities": [{ "type": "mention", "offset": 0, "length": 14 }]
            }
        }))
        .unwrap()
    }

    fn update_group_reply_to_bot() -> Update {
        serde_json::from_value(json!({
            "update_id": 13,
            "message": {
                "message_id": 4,
                "from": { "id": 77, "is_bot": false },
                "chat": { "id": -100, "type": "supergroup" },
                "date": 0,
                "text": "yes please",
                "reply_to_message": {
                    "message_id": 99,
                    "from": { "id": 999, "is_bot": true, "username": "corlinman_bot" },
                    "chat": { "id": -100, "type": "supergroup" },
                    "date": 0,
                    "text": "Need anything?"
                }
            }
        }))
        .unwrap()
    }

    fn update_private_voice() -> Update {
        serde_json::from_value(json!({
            "update_id": 14,
            "message": {
                "message_id": 5,
                "from": { "id": 42, "is_bot": false },
                "chat": { "id": 42, "type": "private" },
                "date": 0,
                "voice": { "file_id": "V123", "duration": 3 }
            }
        }))
        .unwrap()
    }

    fn update_private_photo() -> Update {
        serde_json::from_value(json!({
            "update_id": 15,
            "message": {
                "message_id": 6,
                "from": { "id": 42, "is_bot": false },
                "chat": { "id": 42, "type": "private" },
                "date": 0,
                "photo": [
                    { "file_id": "P_SMALL", "width": 90, "height": 90, "file_size": 500 },
                    { "file_id": "P_MED", "width": 320, "height": 320, "file_size": 5000 },
                    { "file_id": "P_BIG", "width": 1280, "height": 1280, "file_size": 50000 }
                ]
            }
        }))
        .unwrap()
    }

    // ------------------------------------------------------------------------
    // Tests
    // ------------------------------------------------------------------------

    #[test]
    fn webhook_signature_valid_accepts_update() {
        assert!(verify_secret("sekret", Some("sekret")));
    }

    #[test]
    fn webhook_signature_invalid_returns_401() {
        assert!(!verify_secret("sekret", Some("sekret2")));
        assert!(!verify_secret("sekret", Some("")));
        assert!(!verify_secret("sekret", None));
        // Empty config disables the check — documented behaviour.
        assert!(verify_secret("", None));
    }

    #[tokio::test]
    async fn private_chat_triggers_response() {
        let td = TempDir::new();
        let http = FakeHttp::new("x/y.txt", vec![]);
        let ctx = WebhookCtx {
            bot_id: 999,
            bot_username: Some("corlinman_bot"),
            data_dir: &td.0,
            http: &http,
            hooks: None,
        };
        let out = process_update(&ctx, update_private_text("hi"))
            .await
            .unwrap()
            .expect("processed");
        assert_eq!(out.route, MessageRoute::Private);
        assert!(out.route.should_respond());
    }

    #[tokio::test]
    async fn group_without_mention_emits_received_but_not_respond() {
        let td = TempDir::new();
        let http = FakeHttp::new("x/y.txt", vec![]);
        let bus = HookBus::new(16);
        let mut sub = bus.subscribe(corlinman_hooks::HookPriority::Normal);
        let ctx = WebhookCtx {
            bot_id: 999,
            bot_username: Some("corlinman_bot"),
            data_dir: &td.0,
            http: &http,
            hooks: Some(&bus),
        };
        let out = process_update(&ctx, update_group_plain("random chatter"))
            .await
            .unwrap()
            .expect("processed");
        assert_eq!(out.route, MessageRoute::GroupIgnored);
        assert!(!out.route.should_respond());
        // Hook still fired.
        let ev = sub.recv().await.unwrap();
        assert_eq!(ev.kind(), "message_received");
    }

    #[tokio::test]
    async fn group_with_at_mention_triggers_response() {
        let td = TempDir::new();
        let http = FakeHttp::new("x/y.txt", vec![]);
        let ctx = WebhookCtx {
            bot_id: 999,
            bot_username: Some("corlinman_bot"),
            data_dir: &td.0,
            http: &http,
            hooks: None,
        };
        let out = process_update(&ctx, update_group_mention("@corlinman_bot hello"))
            .await
            .unwrap()
            .expect("processed");
        assert_eq!(out.route, MessageRoute::GroupAddressed);
    }

    #[tokio::test]
    async fn group_with_reply_to_bot_triggers_response() {
        let td = TempDir::new();
        let http = FakeHttp::new("x/y.txt", vec![]);
        let ctx = WebhookCtx {
            bot_id: 999,
            bot_username: Some("corlinman_bot"),
            data_dir: &td.0,
            http: &http,
            hooks: None,
        };
        let out = process_update(&ctx, update_group_reply_to_bot())
            .await
            .unwrap()
            .expect("processed");
        assert_eq!(out.route, MessageRoute::GroupAddressed);
    }

    #[tokio::test]
    async fn voice_message_emits_transcribed_hook_with_empty_transcript() {
        let td = TempDir::new();
        let http = FakeHttp::new("voice/a.oga", b"ogg-bytes".to_vec());
        let bus = HookBus::new(16);
        let mut sub = bus.subscribe(corlinman_hooks::HookPriority::Normal);
        let ctx = WebhookCtx {
            bot_id: 999,
            bot_username: Some("corlinman_bot"),
            data_dir: &td.0,
            http: &http,
            hooks: Some(&bus),
        };
        let out = process_update(&ctx, update_private_voice())
            .await
            .unwrap()
            .expect("processed");
        assert_eq!(out.media_kind, "voice");
        assert!(out.media.is_some());

        // First hook: MessageReceived.
        let first = sub.recv().await.unwrap();
        assert_eq!(first.kind(), "message_received");
        // Second hook: MessageTranscribed with empty transcript.
        let second = sub.recv().await.unwrap();
        match second {
            HookEvent::MessageTranscribed {
                transcript,
                media_type,
                media_path,
                ..
            } => {
                assert_eq!(transcript, "");
                assert_eq!(media_type, "voice");
                assert!(!media_path.is_empty());
            }
            other => panic!("expected MessageTranscribed, got {:?}", other.kind()),
        }
    }

    #[tokio::test]
    async fn photo_largest_file_id_selected_for_download() {
        let td = TempDir::new();
        let http = FakeHttp::new("photos/p.jpg", b"fake-jpg".to_vec());
        let ctx = WebhookCtx {
            bot_id: 999,
            bot_username: Some("corlinman_bot"),
            data_dir: &td.0,
            http: &http,
            hooks: None,
        };
        let _ = process_update(&ctx, update_private_photo())
            .await
            .unwrap()
            .expect("processed");
        let calls = http.calls();
        assert_eq!(calls, vec!["P_BIG"], "largest photo file_id must be chosen");
    }

    #[test]
    fn session_key_format_private_vs_group() {
        // Private.
        let priv_msg: Message = serde_json::from_value(json!({
            "message_id": 1,
            "from": { "id": 42, "is_bot": false },
            "chat": { "id": 42, "type": "private" },
            "date": 0,
            "text": "hi"
        }))
        .unwrap();
        assert_eq!(session_key_for(&priv_msg), "telegram:42:42");

        // Group — all users map to the same session.
        let g1: Message = serde_json::from_value(json!({
            "message_id": 1,
            "from": { "id": 1, "is_bot": false },
            "chat": { "id": -100, "type": "supergroup" },
            "date": 0,
            "text": "hi"
        }))
        .unwrap();
        let g2: Message = serde_json::from_value(json!({
            "message_id": 2,
            "from": { "id": 2, "is_bot": false },
            "chat": { "id": -100, "type": "supergroup" },
            "date": 0,
            "text": "hi"
        }))
        .unwrap();
        assert_eq!(session_key_for(&g1), session_key_for(&g2));
        assert_eq!(session_key_for(&g1), "telegram:-100:group");
    }

    #[tokio::test]
    async fn media_download_streams_to_disk() {
        let td = TempDir::new();
        let http = FakeHttp::new("photos/cat.jpg", b"JPGDATA".to_vec());
        let ctx = WebhookCtx {
            bot_id: 999,
            bot_username: None,
            data_dir: &td.0,
            http: &http,
            hooks: None,
        };
        let out = process_update(&ctx, update_private_photo())
            .await
            .unwrap()
            .expect("processed");
        let media = out.media.expect("media present");
        assert!(media.path.exists(), "file must be written to disk");
        let contents = std::fs::read(&media.path).unwrap();
        assert_eq!(contents, b"JPGDATA");
        assert!(media.path.starts_with(&td.0));
    }

    #[test]
    fn non_message_update_returns_none() {
        // Synthetic: update_id only, no message (edited_message etc.).
        let u: Update = serde_json::from_value(json!({ "update_id": 99 })).unwrap();
        // Can't easily run process_update without a runtime; just confirm
        // the decode produced a `None` message so `process_update` takes
        // the early-out branch.
        assert!(u.message.is_none());
    }
}
