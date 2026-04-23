//! Telegram Bot channel adapter.
//!
//! Two transport modes share this crate:
//!
//! - **Long-poll** (`service::run_telegram_channel`) — `getUpdates` over
//!   HTTPS. Preserved as the default for local dev / environments where
//!   no public HTTPS endpoint is reachable.
//! - **Webhook** (`webhook::process_update`) — Telegram POSTs `Update`
//!   payloads to the gateway's `POST /channels/telegram/webhook` route.
//!   Enabled when `[telegram.webhook].public_url` is a non-empty HTTPS
//!   URL; the gateway boot path calls `setWebhook` on startup and
//!   `deleteWebhook` on shutdown.
//!
//! Submodule map:
//! - [`message`]: Bot API wire types (Update/Message/User/Chat/PhotoSize/
//!   Voice/Document) + `ChannelBinding` builder. Long-poll specific
//!   `SendMessageParams` also lives here for historical reasons.
//! - [`types`]: re-exports `message` + adds webhook-only types
//!   (`File`, `MessageRoute`, `classify`, `session_key_for`).
//! - [`media`]: `TelegramHttp` trait + `download_to_media_dir` helper.
//!   Production uses `ReqwestHttp`; tests substitute a `FakeHttp`.
//! - [`send`]: `TelegramSender` — `sendMessage` / `sendPhoto` /
//!   `sendVoice`. Hand-rolled multipart (no extra cargo features).
//! - [`webhook`]: signature verification + `process_update` pipeline
//!   (classify → download media → emit hooks → return `ProcessedUpdate`).
//! - [`service`]: long-poll driver, unchanged for backwards compatibility.
//!
//! # Why not teloxide
//!
//! teloxide is a fine framework but pulls a dispatcher / state-machine
//! stack we don't use — corlinman only needs `getUpdates` in and
//! `sendMessage` + media out. A lean `reqwest` adapter keeps the dep
//! graph predictable and avoids compile-time bloat.

pub mod media;
pub mod message;
pub mod send;
pub mod service;
pub mod types;
pub mod webhook;

pub use message::*;
pub use service::*;
