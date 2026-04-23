//! Outbound Telegram Bot API calls: `sendMessage`, `sendPhoto`, `sendVoice`.
//!
//! `sendMessage` lives in the existing long-poll service; this module adds
//! the multipart variants so the webhook adapter can ship images/voice back
//! into a chat. We hand-roll the multipart boundary to avoid flipping on
//! `reqwest`'s `multipart` feature (small crate, small deps — the cost of
//! one boundary-writer function is worth a cleaner dep graph).

use std::path::Path;

use serde::Deserialize;
use tokio::fs;

/// Envelope returned by every send* endpoint. `result.message_id` is the
/// only field corlinman currently tracks (for session-store round-trips).
#[derive(Debug, Clone, Deserialize)]
struct SentEnvelope {
    ok: bool,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    result: Option<SentMessage>,
}

#[derive(Debug, Clone, Deserialize)]
struct SentMessage {
    message_id: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("telegram api error: {0}")]
    Api(String),
    #[error("http error: {0}")]
    Http(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Thin client over the bot HTTPS surface, scoped to the outbound path.
pub struct TelegramSender {
    pub client: reqwest::Client,
    pub token: String,
    pub base: String,
}

impl TelegramSender {
    pub fn new(client: reqwest::Client, token: String) -> Self {
        Self {
            client,
            token,
            base: "https://api.telegram.org".into(),
        }
    }

    fn endpoint(&self, method: &str) -> String {
        format!("{}/bot{}/{}", self.base, self.token, method)
    }

    /// POST /sendMessage with `chat_id` + `text`. Returns the Telegram
    /// `message_id` of the reply so the session store can anchor the turn.
    pub async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
        reply_to_message_id: Option<i64>,
    ) -> Result<i64, SendError> {
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "text": text,
        });
        if let Some(rid) = reply_to_message_id {
            body["reply_to_message_id"] = rid.into();
        }
        let resp = self
            .client
            .post(self.endpoint("sendMessage"))
            .json(&body)
            .send()
            .await
            .map_err(|e| SendError::Http(e.to_string()))?;
        parse_envelope(resp).await
    }

    /// POST /sendPhoto as multipart/form-data from a local file path.
    /// Falls back to the simple URL form when `source` is a URL string
    /// (no upload — Telegram fetches the image itself).
    pub async fn send_photo(
        &self,
        chat_id: i64,
        source: PhotoSource<'_>,
        caption: Option<&str>,
    ) -> Result<i64, SendError> {
        match source {
            PhotoSource::Url(url) => {
                let mut body = serde_json::json!({
                    "chat_id": chat_id,
                    "photo": url,
                });
                if let Some(c) = caption {
                    body["caption"] = c.into();
                }
                let resp = self
                    .client
                    .post(self.endpoint("sendPhoto"))
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| SendError::Http(e.to_string()))?;
                parse_envelope(resp).await
            }
            PhotoSource::Path(path) => {
                let bytes = fs::read(path).await?;
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("photo.bin");
                let body =
                    build_multipart(chat_id, "photo", filename, &bytes, caption, "image/jpeg");
                self.post_multipart("sendPhoto", body.body, &body.boundary)
                    .await
            }
        }
    }

    /// POST /sendVoice as multipart/form-data from a local OGG path.
    pub async fn send_voice(
        &self,
        chat_id: i64,
        path: &Path,
        caption: Option<&str>,
    ) -> Result<i64, SendError> {
        let bytes = fs::read(path).await?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("voice.ogg");
        let body = build_multipart(chat_id, "voice", filename, &bytes, caption, "audio/ogg");
        self.post_multipart("sendVoice", body.body, &body.boundary)
            .await
    }

    async fn post_multipart(
        &self,
        method: &str,
        body: Vec<u8>,
        boundary: &str,
    ) -> Result<i64, SendError> {
        let resp = self
            .client
            .post(self.endpoint(method))
            .header(
                reqwest::header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(body)
            .send()
            .await
            .map_err(|e| SendError::Http(e.to_string()))?;
        parse_envelope(resp).await
    }
}

/// Photo source variants — either a local file (uploaded as multipart)
/// or a URL (Telegram fetches it server-side).
pub enum PhotoSource<'a> {
    Path(&'a Path),
    Url(&'a str),
}

async fn parse_envelope(resp: reqwest::Response) -> Result<i64, SendError> {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(SendError::Http(format!("{status}: {text}")));
    }
    let env: SentEnvelope =
        serde_json::from_str(&text).map_err(|e| SendError::Http(e.to_string()))?;
    if !env.ok {
        return Err(SendError::Api(env.description.unwrap_or_default()));
    }
    env.result
        .map(|m| m.message_id)
        .ok_or_else(|| SendError::Api("response missing result.message_id".into()))
}

/// Output of [`build_multipart`] — body bytes + the boundary string used in
/// the `Content-Type` header.
struct Multipart {
    body: Vec<u8>,
    boundary: String,
}

/// Assemble a minimal `multipart/form-data` body with `chat_id` + file
/// part + optional caption. The boundary is a fixed, high-entropy string
/// the caller passes via the Content-Type header.
///
/// Layout:
/// ```text
/// --BOUNDARY\r\n
/// Content-Disposition: form-data; name="chat_id"\r\n\r\n
/// 12345\r\n
/// --BOUNDARY\r\n
/// Content-Disposition: form-data; name="photo"; filename="..."\r\n
/// Content-Type: image/jpeg\r\n\r\n
/// <bytes>\r\n
/// --BOUNDARY--\r\n
/// ```
fn build_multipart(
    chat_id: i64,
    file_field: &str,
    filename: &str,
    bytes: &[u8],
    caption: Option<&str>,
    content_type: &str,
) -> Multipart {
    let boundary = format!("corlinman-tg-{}", uuid::Uuid::new_v4().simple());
    let mut body: Vec<u8> = Vec::with_capacity(bytes.len() + 256);
    let dash = b"--";
    let crlf = b"\r\n";

    // chat_id text part
    body.extend_from_slice(dash);
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(crlf);
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"chat_id\"");
    body.extend_from_slice(crlf);
    body.extend_from_slice(crlf);
    body.extend_from_slice(chat_id.to_string().as_bytes());
    body.extend_from_slice(crlf);

    // caption text part (optional)
    if let Some(cap) = caption {
        body.extend_from_slice(dash);
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(crlf);
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"caption\"");
        body.extend_from_slice(crlf);
        body.extend_from_slice(crlf);
        body.extend_from_slice(cap.as_bytes());
        body.extend_from_slice(crlf);
    }

    // file part
    body.extend_from_slice(dash);
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(crlf);
    let header = format!(
        "Content-Disposition: form-data; name=\"{field}\"; filename=\"{fn_}\"\r\nContent-Type: {ct}\r\n\r\n",
        field = file_field,
        fn_ = filename,
        ct = content_type,
    );
    body.extend_from_slice(header.as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(crlf);

    // closing boundary
    body.extend_from_slice(dash);
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(dash);
    body.extend_from_slice(crlf);

    Multipart { body, boundary }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multipart_includes_chat_id_filename_and_bytes() {
        let mp = build_multipart(
            42,
            "photo",
            "cat.jpg",
            b"\x89PNG\r\n",
            Some("hello"),
            "image/jpeg",
        );
        let s = String::from_utf8_lossy(&mp.body);
        assert!(s.contains("name=\"chat_id\""));
        assert!(s.contains("42"));
        assert!(s.contains("name=\"photo\""));
        assert!(s.contains("filename=\"cat.jpg\""));
        assert!(s.contains("name=\"caption\""));
        assert!(s.contains("hello"));
        assert!(mp.body.windows(5).any(|w| w == b"\x89PNG\r"));
        // Closing delimiter must be present.
        let closer = format!("--{}--", mp.boundary);
        assert!(s.contains(&closer));
    }

    #[test]
    fn multipart_boundary_is_unique_per_call() {
        let a = build_multipart(1, "photo", "a", b"x", None, "image/jpeg");
        let b = build_multipart(1, "photo", "a", b"x", None, "image/jpeg");
        assert_ne!(a.boundary, b.boundary);
    }
}
