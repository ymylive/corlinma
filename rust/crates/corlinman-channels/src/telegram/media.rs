//! Telegram file-download orchestration.
//!
//! Telegram's attachment API is a two-step dance:
//! 1. `GET /bot<token>/getFile?file_id=...` → `{file_path: "voice/xxx.ogg"}`
//! 2. `GET /file/bot<token>/<file_path>` → the raw bytes.
//!
//! Step 2's endpoint refuses files > 20MB. We surface that as a typed
//! [`MediaError::TooLarge`] so the caller can fall back to a polite "this
//! attachment is too large for the bot API" reply instead of crashing.
//!
//! The HTTP surface is captured behind the [`TelegramHttp`] trait so tests
//! can inject a fake that returns canned `File` envelopes and byte streams
//! without touching the network.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::Stream;
use tokio::fs;
use tokio::io::AsyncWriteExt;

use super::types::File;

/// Maximum download size we permit (Telegram bot API cap is 20MB;
/// we apply the same ceiling defensively).
pub const MAX_DOWNLOAD_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("telegram api error: {0}")]
    Api(String),
    #[error("getFile returned no file_path (file likely too large for bot API)")]
    NoFilePath,
    #[error("download exceeded {MAX_DOWNLOAD_BYTES} byte cap")]
    TooLarge,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("http error: {0}")]
    Http(String),
}

/// Narrow HTTP surface the media helper depends on. Production wiring
/// constructs [`ReqwestHttp`]; tests use [`FakeHttp`].
#[async_trait]
pub trait TelegramHttp: Send + Sync {
    /// Resolve a `file_id` to the `File` envelope (`file_path` + size).
    async fn get_file(&self, file_id: &str) -> Result<File, MediaError>;

    /// Stream the file bytes. We return a boxed `Stream` rather than a
    /// `Vec<u8>` so large voice notes never force a full-buffer allocation
    /// during the download.
    async fn download_stream(
        &self,
        file_path: &str,
    ) -> Result<Box<dyn Stream<Item = Result<Bytes, MediaError>> + Send + Unpin>, MediaError>;
}

/// Reqwest-backed [`TelegramHttp`]. Shares the `reqwest::Client` with the
/// webhook handler; the base URL is overridable purely to keep the shape
/// ready for a sandbox instance (not used in tests — those pick the fake).
pub struct ReqwestHttp {
    pub client: reqwest::Client,
    pub token: String,
    pub base: String,
}

impl ReqwestHttp {
    pub fn new(client: reqwest::Client, token: String) -> Self {
        Self {
            client,
            token,
            base: "https://api.telegram.org".into(),
        }
    }
}

#[async_trait]
impl TelegramHttp for ReqwestHttp {
    async fn get_file(&self, file_id: &str) -> Result<File, MediaError> {
        #[derive(serde::Deserialize)]
        struct Env {
            ok: bool,
            #[serde(default)]
            description: Option<String>,
            #[serde(default)]
            result: Option<File>,
        }
        let url = format!("{}/bot{}/getFile", self.base, self.token);
        let resp = self
            .client
            .get(&url)
            .query(&[("file_id", file_id)])
            .send()
            .await
            .map_err(|e| MediaError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(MediaError::Http(format!("getFile HTTP {}", resp.status())));
        }
        let env: Env = resp
            .json()
            .await
            .map_err(|e| MediaError::Http(e.to_string()))?;
        if !env.ok {
            return Err(MediaError::Api(env.description.unwrap_or_default()));
        }
        env.result
            .ok_or_else(|| MediaError::Api("no result".into()))
    }

    async fn download_stream(
        &self,
        file_path: &str,
    ) -> Result<Box<dyn Stream<Item = Result<Bytes, MediaError>> + Send + Unpin>, MediaError> {
        use futures_util::StreamExt;
        let url = format!("{}/file/bot{}/{}", self.base, self.token, file_path);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| MediaError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(MediaError::Http(format!("download HTTP {}", resp.status())));
        }
        let stream = resp
            .bytes_stream()
            .map(|r| r.map_err(|e| MediaError::Http(e.to_string())));
        Ok(Box::new(Box::pin(stream)))
    }
}

/// Downloaded-media metadata returned to the caller.
#[derive(Debug, Clone)]
pub struct DownloadedMedia {
    /// Absolute filesystem path to the persisted file.
    pub path: PathBuf,
    /// Total bytes written.
    pub bytes: u64,
    /// The `file_id` that was downloaded (useful for logging).
    pub file_id: String,
}

/// Resolve + stream a Telegram attachment to
/// `<data_dir>/media/telegram/<unique>.<ext>`.
///
/// - `file_id`: the Telegram handle (photo/voice/document).
/// - `data_dir`: the gateway's configured `server.data_dir`.
/// - `fallback_ext`: extension to use when `file_path` has none (e.g.
///   `.ogg` for voice, `.bin` otherwise).
///
/// The file name uses `file_unique_id` when Telegram returned one
/// (idempotent across downloads of the same asset) and falls back to
/// `file_id`. This means a retry of the same webhook won't duplicate the
/// blob — the second write overwrites the first with identical bytes.
pub async fn download_to_media_dir<H: TelegramHttp + ?Sized>(
    http: &H,
    file_id: &str,
    data_dir: &Path,
    fallback_ext: &str,
) -> Result<DownloadedMedia, MediaError> {
    let file = http.get_file(file_id).await?;
    let file_path = file.file_path.ok_or(MediaError::NoFilePath)?;

    // Derive extension from the resolved `file_path` (the Telegram server
    // stores "photos/file_7.jpg", "voice/file_3.oga", etc.).
    let ext = Path::new(&file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or(fallback_ext.trim_start_matches('.'));

    let unique = file
        .file_unique_id
        .clone()
        .unwrap_or_else(|| file_id.to_string());
    // Sanitize: replace path separators & control chars with '_'.
    let safe: String = unique
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();

    let target_dir = data_dir.join("media").join("telegram");
    fs::create_dir_all(&target_dir).await?;
    let target = target_dir.join(format!("{safe}.{ext}"));

    // Stream to disk with a running byte counter so we can reject oversize
    // payloads mid-flight (Telegram's 20MB cap is usually enforced server
    // side but the trait allows any future source).
    let mut stream = http.download_stream(&file_path).await?;
    let mut file_out = fs::File::create(&target).await?;
    let mut written: u64 = 0;
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        written = written.saturating_add(chunk.len() as u64);
        if written > MAX_DOWNLOAD_BYTES {
            // Best-effort cleanup; ignore the error since TooLarge is the
            // real story.
            let _ = fs::remove_file(&target).await;
            return Err(MediaError::TooLarge);
        }
        file_out.write_all(&chunk).await?;
    }
    file_out.flush().await?;
    Ok(DownloadedMedia {
        path: target,
        bytes: written,
        file_id: file_id.to_string(),
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;
    use std::sync::{Arc, Mutex};

    /// Minimal in-memory `TelegramHttp`. `scripted_file_path` is what
    /// `get_file` will report; `scripted_bytes` is streamed back by
    /// `download_stream`.
    pub struct FakeHttp {
        pub scripted_file_path: Option<String>,
        pub scripted_bytes: Vec<u8>,
        pub get_file_calls: Mutex<Vec<String>>,
        pub download_calls: Mutex<Vec<String>>,
    }

    impl FakeHttp {
        pub fn new(file_path: &str, bytes: Vec<u8>) -> Self {
            Self {
                scripted_file_path: Some(file_path.into()),
                scripted_bytes: bytes,
                get_file_calls: Mutex::new(Vec::new()),
                download_calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl TelegramHttp for FakeHttp {
        async fn get_file(&self, file_id: &str) -> Result<File, MediaError> {
            self.get_file_calls
                .lock()
                .unwrap()
                .push(file_id.to_string());
            Ok(File {
                file_id: file_id.into(),
                file_unique_id: Some(format!("uniq_{file_id}")),
                file_size: Some(self.scripted_bytes.len() as i64),
                file_path: self.scripted_file_path.clone(),
            })
        }

        async fn download_stream(
            &self,
            file_path: &str,
        ) -> Result<Box<dyn Stream<Item = Result<Bytes, MediaError>> + Send + Unpin>, MediaError>
        {
            self.download_calls
                .lock()
                .unwrap()
                .push(file_path.to_string());
            let bytes = Bytes::copy_from_slice(&self.scripted_bytes);
            let s = stream::iter(vec![Ok::<_, MediaError>(bytes)]);
            Ok(Box::new(Box::pin(s)))
        }
    }

    #[tokio::test]
    async fn media_download_streams_to_disk() {
        let tmp = tempdir();
        let bytes = b"fake-ogg-bytes".to_vec();
        let http = Arc::new(FakeHttp::new("voice/file_7.oga", bytes.clone()));
        let got = download_to_media_dir(&*http, "FILE123", tmp.path(), "bin")
            .await
            .expect("download ok");
        assert_eq!(got.bytes, bytes.len() as u64);
        assert!(got.path.starts_with(tmp.path()));
        assert!(got.path.extension().and_then(|e| e.to_str()) == Some("oga"));
        let on_disk = std::fs::read(&got.path).unwrap();
        assert_eq!(on_disk, bytes);
    }

    #[tokio::test]
    async fn missing_file_path_errors_out() {
        let tmp = tempdir();
        let http = FakeHttp {
            scripted_file_path: None,
            scripted_bytes: vec![],
            get_file_calls: Mutex::new(Vec::new()),
            download_calls: Mutex::new(Vec::new()),
        };
        let err = download_to_media_dir(&http, "FILE", tmp.path(), "bin")
            .await
            .unwrap_err();
        assert!(matches!(err, MediaError::NoFilePath));
    }

    /// Minimal in-process tempdir helper — we don't want to grow a
    /// `tempfile` dev-dep just for this module, and the gateway crate has
    /// one already.
    fn tempdir() -> TempDir {
        TempDir::new()
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let mut p = std::env::temp_dir();
            let unique = format!(
                "corlinman-tg-media-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4().simple()
            );
            p.push(unique);
            std::fs::create_dir_all(&p).unwrap();
            Self { path: p }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
