//! Webhook-mode companion types.
//!
//! The core `Update` / `Message` / `User` / `Chat` / `PhotoSize` / `Voice` /
//! `Document` types live in [`super::message`] — this module just re-exports
//! them (so callers can `use corlinman_channels::telegram::types::*`) and
//! adds the webhook-specific wire shapes (getFile envelope + classification
//! helpers) without bloating `message.rs`.

use serde::Deserialize;

pub use super::message::{
    binding_from_message, is_mentioning_bot, Chat, Document, Message, MessageEntity, PhotoSize,
    SendMessageParams, Update, User, Voice,
};

/// Result of `GET /bot<token>/getFile?file_id=...`. Telegram returns
/// `{ok: true, result: {file_id, file_unique_id, file_size, file_path}}`.
/// Only `file_path` is load-bearing for the download step.
#[derive(Debug, Clone, Deserialize)]
pub struct File {
    #[serde(default)]
    pub file_id: String,
    #[serde(default)]
    pub file_unique_id: Option<String>,
    #[serde(default)]
    pub file_size: Option<i64>,
    /// Relative path under `api.telegram.org/file/bot<token>/`; absent for
    /// files larger than 20MB (bot API download cap).
    #[serde(default)]
    pub file_path: Option<String>,
}

/// Classification of an inbound message into one of three routing buckets.
/// The webhook handler uses this to decide whether to trigger an agent
/// response or simply emit `MessageReceived` for hook subscribers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRoute {
    /// Private 1:1 DM. Always respond.
    Private,
    /// Group / supergroup where the bot is addressed (via `@mention` or a
    /// `reply_to_message.from` that is the bot). Respond.
    GroupAddressed,
    /// Group / supergroup where the bot is **not** addressed. Emit
    /// `MessageReceived` (so hook consumers can still log/analyze) but do
    /// not engage the agent.
    GroupIgnored,
}

impl MessageRoute {
    /// True when the agent should be invoked to produce a reply.
    pub fn should_respond(self) -> bool {
        matches!(self, Self::Private | Self::GroupAddressed)
    }

    /// True when the source chat is a group / supergroup (used when shaping
    /// the `MessageReceived.metadata.is_group` flag).
    pub fn is_group(self) -> bool {
        matches!(self, Self::GroupAddressed | Self::GroupIgnored)
    }
}

/// Decide how to route an inbound message based on chat type + whether the
/// bot is addressed. Pure function so the webhook handler stays thin and
/// the unit tests can exercise each branch without a mock HTTP client.
pub fn classify(msg: &Message, bot_id: i64, bot_username: Option<&str>) -> MessageRoute {
    if msg.chat.is_private() {
        return MessageRoute::Private;
    }

    // Entity-based @mention (covers both `Mention` by username and
    // `TextMention` by user.id).
    if is_mentioning_bot(msg, bot_id, bot_username) {
        return MessageRoute::GroupAddressed;
    }

    // Fallback: case-insensitive substring "@<bot_username>" in the text.
    // `is_mentioning_bot` relies on Telegram entity offsets; forwarded
    // messages / HTML-mode edits sometimes strip entities so the literal
    // mention text is all we have.
    if let (Some(text), Some(uname)) = (msg.text.as_deref(), bot_username) {
        let needle = format!("@{}", uname.to_ascii_lowercase());
        if text.to_ascii_lowercase().contains(&needle) {
            return MessageRoute::GroupAddressed;
        }
    }

    // Reply-to-bot: user replied directly to one of the bot's messages.
    if let Some(reply) = &msg.reply_to_message {
        if let Some(from) = &reply.from {
            // Match by id when known (reliable) or by username (the task
            // spec explicitly requires the username path).
            if from.id == bot_id {
                return MessageRoute::GroupAddressed;
            }
            if let (Some(bu), Some(ru)) = (bot_username, from.username.as_deref()) {
                if ru.eq_ignore_ascii_case(bu) {
                    return MessageRoute::GroupAddressed;
                }
            }
        }
    }

    MessageRoute::GroupIgnored
}

/// Build the session key used by the agent runtime.
///
/// - Private chats: `telegram:<chat_id>:<user_id>` — one conversation per
///   (bot, peer). When `from` is absent (anonymous posts) we fall back to
///   `chat_id` so the key remains stable per-chat.
/// - Group / supergroup / channel: `telegram:<chat_id>:group` — the agent
///   sees the group as a single shared conversation regardless of who
///   typed the latest message.
pub fn session_key_for(msg: &Message) -> String {
    if msg.chat.is_private() {
        let user_id = msg.from.as_ref().map(|u| u.id).unwrap_or(msg.chat.id);
        format!("telegram:{}:{}", msg.chat.id, user_id)
    } else {
        format!("telegram:{}:group", msg.chat.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn private(text: &str, user_id: i64) -> Message {
        serde_json::from_value(json!({
            "message_id": 1,
            "from": { "id": user_id, "is_bot": false },
            "chat": { "id": user_id, "type": "private" },
            "date": 0,
            "text": text,
        }))
        .unwrap()
    }

    fn group_plain(text: &str) -> Message {
        serde_json::from_value(json!({
            "message_id": 1,
            "from": { "id": 77, "is_bot": false },
            "chat": { "id": -100, "type": "supergroup" },
            "date": 0,
            "text": text,
        }))
        .unwrap()
    }

    fn group_mention_entity(text: &str) -> Message {
        serde_json::from_value(json!({
            "message_id": 1,
            "from": { "id": 77, "is_bot": false },
            "chat": { "id": -100, "type": "supergroup" },
            "date": 0,
            "text": text,
            "entities": [{ "type": "mention", "offset": 0, "length": 14 }]
        }))
        .unwrap()
    }

    #[test]
    fn classify_private_always_responds() {
        let m = private("hi", 42);
        assert_eq!(classify(&m, 999, Some("bot")), MessageRoute::Private);
    }

    #[test]
    fn classify_group_without_mention_is_ignored() {
        let m = group_plain("hello world");
        assert_eq!(
            classify(&m, 999, Some("corlinman_bot")),
            MessageRoute::GroupIgnored
        );
    }

    #[test]
    fn classify_group_entity_mention_is_addressed() {
        let m = group_mention_entity("@corlinman_bot hello");
        assert_eq!(
            classify(&m, 999, Some("corlinman_bot")),
            MessageRoute::GroupAddressed
        );
    }

    #[test]
    fn classify_group_substring_mention_fallback() {
        // No entity, but the text still contains the @handle (e.g. a
        // forward that stripped entities).
        let m = group_plain("hey @CorlinMan_Bot please help");
        assert_eq!(
            classify(&m, 999, Some("corlinman_bot")),
            MessageRoute::GroupAddressed
        );
    }

    #[test]
    fn classify_reply_to_bot_is_addressed() {
        let raw = json!({
            "message_id": 2,
            "from": { "id": 77, "is_bot": false },
            "chat": { "id": -100, "type": "supergroup" },
            "date": 0,
            "text": "yes please",
            "reply_to_message": {
                "message_id": 1,
                "from": { "id": 999, "is_bot": true, "username": "corlinman_bot" },
                "chat": { "id": -100, "type": "supergroup" },
                "date": 0,
                "text": "Need anything?"
            }
        });
        let m: Message = serde_json::from_value(raw).unwrap();
        assert_eq!(
            classify(&m, 999, Some("corlinman_bot")),
            MessageRoute::GroupAddressed
        );
    }

    #[test]
    fn session_key_private_uses_user_id() {
        let m = private("hi", 42);
        assert_eq!(session_key_for(&m), "telegram:42:42");
    }

    #[test]
    fn session_key_group_uses_group_suffix() {
        let m = group_plain("hello");
        assert_eq!(session_key_for(&m), "telegram:-100:group");
    }

    #[test]
    fn message_route_helpers() {
        assert!(MessageRoute::Private.should_respond());
        assert!(MessageRoute::GroupAddressed.should_respond());
        assert!(!MessageRoute::GroupIgnored.should_respond());
        assert!(!MessageRoute::Private.is_group());
        assert!(MessageRoute::GroupAddressed.is_group());
        assert!(MessageRoute::GroupIgnored.is_group());
    }
}
