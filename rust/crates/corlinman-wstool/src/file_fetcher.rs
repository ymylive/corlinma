//! Multi-scheme blob fetcher shared by gateway-side tooling that needs to
//! read files which may live on the gateway itself, on an arbitrary
//! HTTP(S) origin, or on a remote WsTool runner.
//!
//! # Schemes
//!
//! - `file:///<path>` — read from the local filesystem, rejecting any
//!   path that escapes the configured `local_root` (via `..` components
//!   or symlink resolution).
//! - `http://…` / `https://…` — fetched with `reqwest`, subject to the
//!   `max_bytes` cap.
//! - `ws-tool://<runner_id>/<path>` — dispatched through an existing
//!   [`crate::server::WsToolServer`] by invoking the reserved tool name
//!   `__file_fetcher__/read`. Runners that wish to serve this URI family
//!   advertise the tool and register a [`FileServer`] handler (typically
//!   via [`file_server_handler`]).
//!
//! # Design choices
//!
//! * Framing — the `ws-tool://` transport reuses the existing
//!   [`crate::message::WsToolMessage::Invoke`] text frames and base64-in-
//!   JSON rather than introducing a new binary framing on the
//!   WebSocket. B4-BE3's multiplexing, cancel, heartbeat, and hook-bus
//!   emission apply unchanged; a future workstream can upgrade to
//!   tungstenite binary frames without churning this module's API.
//! * Reserved tool name — `__file_fetcher__/read` is namespaced by the
//!   `__…__` convention so it can coexist with runner-defined tools
//!   without collision and is easy to filter out of tool advertisements
//!   shown to humans.
//! * URI scope — we only parse three schemes. `ws-tool://` parsing is
//!   intentionally minimal: `authority` is the runner id (no userinfo,
//!   no port), and the `path` (minus the leading `/`) is forwarded to
//!   [`FileServer::open`] unchanged so runners can apply their own
//!   virtual layout.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use bytes::Bytes;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::error::ToolError;
use crate::message::ToolAdvert;
use crate::runner::{ProgressSink, ToolHandler};
use crate::server::{invoke_once, ServerState};

/// Reserved tool name dispatched over the WsTool transport to serve
/// `ws-tool://` URIs. See the module-level docstring.
pub const FILE_FETCHER_TOOL: &str = "__file_fetcher__/read";

/// Default per-fetch size cap (100 MiB).
pub const DEFAULT_MAX_BYTES: u64 = 100 * 1024 * 1024;

const WS_TOOL_INVOKE_TIMEOUT_MS: u64 = 30_000;

/// A successfully fetched blob with content metadata.
#[derive(Debug, Clone)]
pub struct FetchedBlob {
    pub data: Bytes,
    pub mime: Option<String>,
    /// Lowercase hex-encoded SHA-256 of [`Self::data`].
    pub sha256: String,
    pub total_bytes: u64,
}

/// All error modes produced by [`FileFetcher::fetch`] and the runner-side
/// [`FileServer`] helpers.
#[derive(Debug, thiserror::Error)]
pub enum FileFetcherError {
    #[error("unsupported uri scheme: {0}")]
    UnsupportedScheme(String),
    #[error("invalid uri: {0}")]
    InvalidUri(String),
    #[error("local_root not configured")]
    LocalRootMissing,
    #[error("path escapes local_root: {0}")]
    PathTraversal(String),
    #[error("runner not connected: {0}")]
    UnknownRunner(String),
    #[error("size limit exceeded: {got} > {limit}")]
    SizeLimit { got: u64, limit: u64 },
    #[error("hash mismatch (expected {expected}, got {got})")]
    HashMismatch { expected: String, got: String },
    #[error("http {status}")]
    HttpStatus { status: u16 },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("transport: {0}")]
    Transport(String),
}

/// Gateway-side multi-scheme fetcher. Cheap to construct; clone the
/// `reqwest::Client` and reuse across sites for connection pooling.
pub struct FileFetcher {
    local_root: Option<PathBuf>,
    http_client: reqwest::Client,
    max_bytes: u64,
    ws_server: Option<Arc<ServerState>>,
}

impl FileFetcher {
    pub fn new(local_root: Option<PathBuf>, http_client: reqwest::Client, max_bytes: u64) -> Self {
        Self {
            local_root,
            http_client,
            max_bytes,
            ws_server: None,
        }
    }

    /// Attach a `WsToolServer` state handle so this fetcher can resolve
    /// `ws-tool://` URIs. See [`crate::server::WsToolServer::state`].
    pub fn with_ws_server(mut self, state: Arc<ServerState>) -> Self {
        self.ws_server = Some(state);
        self
    }

    /// Retrieve a blob by URI. See the module docstring for supported
    /// schemes and security invariants.
    pub async fn fetch(&self, uri: &str) -> Result<FetchedBlob, FileFetcherError> {
        let scheme = uri_scheme(uri);
        let span = tracing::info_span!(
            "file_fetch",
            uri_scheme = scheme,
            total_bytes = tracing::field::Empty,
            ok = tracing::field::Empty,
        );
        let _enter = span.enter();

        let result = if let Some(rest) = strip_scheme(uri, "file://") {
            self.fetch_file(rest).await
        } else if uri.starts_with("http://") || uri.starts_with("https://") {
            self.fetch_http(uri).await
        } else if let Some(rest) = strip_scheme(uri, "ws-tool://") {
            self.fetch_ws_tool(rest).await
        } else {
            let scheme = uri.split(':').next().unwrap_or("").to_string();
            Err(FileFetcherError::UnsupportedScheme(scheme))
        };

        let ok = result.is_ok();
        span.record("ok", ok);
        corlinman_core::metrics::FILE_FETCHER_FETCHES_TOTAL
            .with_label_values(&[scheme, if ok { "true" } else { "false" }])
            .inc();
        if let Ok(blob) = &result {
            span.record("total_bytes", blob.total_bytes);
            corlinman_core::metrics::FILE_FETCHER_BYTES_TOTAL
                .with_label_values(&[scheme])
                .inc_by(blob.total_bytes as f64);
        }
        result
    }

    async fn fetch_file(&self, path_part: &str) -> Result<FetchedBlob, FileFetcherError> {
        let root = self
            .local_root
            .as_ref()
            .ok_or(FileFetcherError::LocalRootMissing)?;
        // `file:///abs/path` arrives as `/abs/path`; `file://host/path`
        // would arrive as `host/path` — reject host forms (not this
        // module's concern).
        if !path_part.starts_with('/') {
            return Err(FileFetcherError::InvalidUri(format!(
                "file:// URIs must be absolute, got {path_part}"
            )));
        }
        let candidate = PathBuf::from(path_part);
        read_within_root(root, &candidate, self.max_bytes).await
    }

    async fn fetch_http(&self, uri: &str) -> Result<FetchedBlob, FileFetcherError> {
        let resp = self
            .http_client
            .get(uri)
            .send()
            .await
            .map_err(|e| FileFetcherError::Transport(e.to_string()))?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            return Err(FileFetcherError::HttpStatus { status });
        }
        let mime = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let mut hasher = Sha256::new();
        let mut buf: Vec<u8> = Vec::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| FileFetcherError::Transport(e.to_string()))?;
            let next = buf.len() as u64 + chunk.len() as u64;
            if next > self.max_bytes {
                return Err(FileFetcherError::SizeLimit {
                    got: next,
                    limit: self.max_bytes,
                });
            }
            hasher.update(&chunk);
            buf.extend_from_slice(&chunk);
        }
        let total = buf.len() as u64;
        Ok(FetchedBlob {
            data: Bytes::from(buf),
            mime,
            sha256: hex_lower(&hasher.finalize()),
            total_bytes: total,
        })
    }

    async fn fetch_ws_tool(&self, rest: &str) -> Result<FetchedBlob, FileFetcherError> {
        let state = self
            .ws_server
            .as_ref()
            .ok_or_else(|| FileFetcherError::Transport("ws-tool fetcher not attached".into()))?;
        // authority / path
        let (runner_id, path) = match rest.split_once('/') {
            Some((r, p)) if !r.is_empty() => (r.to_string(), p.to_string()),
            _ => {
                return Err(FileFetcherError::InvalidUri(format!(
                    "ws-tool URI must be ws-tool://<runner>/<path>, got ws-tool://{rest}"
                )))
            }
        };
        // Up-front presence check: the generic tool-index lookup inside
        // `invoke_once` would return `Unsupported` whether the runner is
        // missing or the tool is missing; we want a distinct error when
        // the runner id is unknown so callers can distinguish.
        if !state.runners.contains_key(&runner_id) {
            return Err(FileFetcherError::UnknownRunner(runner_id));
        }

        let args = serde_json::json!({
            "path": path,
            "max_bytes": self.max_bytes,
            "runner_id": runner_id,
        });

        let payload = invoke_once(
            state.clone(),
            FILE_FETCHER_TOOL.to_string(),
            args,
            WS_TOOL_INVOKE_TIMEOUT_MS,
            CancellationToken::new(),
        )
        .await
        .map_err(|e| FileFetcherError::Transport(e.to_string()))?;

        let data_b64 = payload
            .get("data_b64")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FileFetcherError::Transport("runner omitted data_b64".into()))?;
        let mime = payload
            .get("mime")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let remote_sha = payload
            .get("sha256")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let total_bytes = payload
            .get("total_bytes")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let raw = base64::engine::general_purpose::STANDARD
            .decode(data_b64)
            .map_err(|e| FileFetcherError::Transport(format!("base64: {e}")))?;
        let got_len = raw.len() as u64;
        if got_len > self.max_bytes {
            return Err(FileFetcherError::SizeLimit {
                got: got_len,
                limit: self.max_bytes,
            });
        }
        let got_sha = {
            let mut h = Sha256::new();
            h.update(&raw);
            hex_lower(&h.finalize())
        };
        if let Some(expected) = &remote_sha {
            if expected != &got_sha {
                return Err(FileFetcherError::HashMismatch {
                    expected: expected.clone(),
                    got: got_sha,
                });
            }
        }

        Ok(FetchedBlob {
            data: Bytes::from(raw),
            mime,
            sha256: got_sha,
            total_bytes: total_bytes.max(got_len),
        })
    }
}

/// Runner-side abstraction that serves the reserved file-fetcher tool.
/// Implementors decide what the `path` argument means in their virtual
/// layout. [`DiskFileServer`] is the canonical impl.
#[async_trait]
pub trait FileServer: Send + Sync + 'static {
    async fn open(&self, path: &str) -> Result<FileReadInfo, FileFetcherError>;
}

/// Result of a successful [`FileServer::open`].
#[derive(Debug)]
pub struct FileReadInfo {
    pub data: Bytes,
    pub mime: Option<String>,
}

/// Reference [`FileServer`] that reads from a rooted directory with
/// symlink-escape protection.
pub struct DiskFileServer {
    root: PathBuf,
    max_bytes: u64,
}

impl DiskFileServer {
    pub fn new(root: PathBuf, max_bytes: u64) -> Self {
        Self { root, max_bytes }
    }
}

#[async_trait]
impl FileServer for DiskFileServer {
    async fn open(&self, path: &str) -> Result<FileReadInfo, FileFetcherError> {
        let cleaned = path.trim_start_matches('/');
        let candidate = self.root.join(cleaned);
        let blob = read_within_root(&self.root, &candidate, self.max_bytes).await?;
        Ok(FileReadInfo {
            data: blob.data,
            mime: None,
        })
    }
}

/// Advertisement the runner should include in its `Accept` so the
/// gateway's tool index knows it can serve `ws-tool://<this-runner>/…`
/// URIs. Combine with [`file_server_handler`] on the runner side.
pub fn file_server_advert() -> ToolAdvert {
    ToolAdvert {
        name: FILE_FETCHER_TOOL.to_string(),
        description: "FileFetcher read endpoint (base64-over-JSON)".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "max_bytes": {"type": "integer"}
            },
            "required": ["path"]
        }),
    }
}

/// Wrap a [`FileServer`] as a [`ToolHandler`]. Use this in a runner's
/// `serve_with` call when it should answer the reserved tool name;
/// composite handlers that also serve other tools can delegate based on
/// `tool == FILE_FETCHER_TOOL`.
pub fn file_server_handler<F: FileServer>(server: F) -> FileServerHandler<F> {
    FileServerHandler { server }
}

/// Adapter implementing [`ToolHandler`] on top of a [`FileServer`].
pub struct FileServerHandler<F> {
    server: F,
}

#[async_trait]
impl<F: FileServer> ToolHandler for FileServerHandler<F> {
    async fn invoke(
        &self,
        tool: &str,
        args: serde_json::Value,
        _progress: ProgressSink,
        _cancel: CancellationToken,
    ) -> Result<serde_json::Value, ToolError> {
        if tool != FILE_FETCHER_TOOL {
            return Err(ToolError::new(
                "unsupported",
                format!("file_server_handler does not serve {tool}"),
            ));
        }
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::new("invalid_args", "missing string `path`"))?;
        let info = self
            .server
            .open(path)
            .await
            .map_err(|e| ToolError::new("read_failed", e.to_string()))?;
        let mut hasher = Sha256::new();
        hasher.update(&info.data);
        let sha = hex_lower(&hasher.finalize());
        let total = info.data.len() as u64;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&info.data);
        Ok(serde_json::json!({
            "data_b64": b64,
            "mime": info.mime,
            "sha256": sha,
            "total_bytes": total,
        }))
    }
}

// ------------------------------------------------------------------
// Internals
// ------------------------------------------------------------------

fn strip_scheme<'a>(uri: &'a str, scheme: &str) -> Option<&'a str> {
    uri.strip_prefix(scheme)
}

/// Low-cardinality scheme label for metrics. Collapses everything outside
/// the supported three into `"other"` so a malicious operator can't grow
/// the metric's cardinality unbounded.
fn uri_scheme(uri: &str) -> &'static str {
    if uri.starts_with("file://") {
        "file"
    } else if uri.starts_with("http://") {
        "http"
    } else if uri.starts_with("https://") {
        "https"
    } else if uri.starts_with("ws-tool://") {
        "ws-tool"
    } else {
        "other"
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(*b >> 4) as usize] as char);
        out.push(HEX[(*b & 0x0F) as usize] as char);
    }
    out
}

/// Canonicalize both `root` and `candidate`, verify the latter lives
/// under the former, enforce `max_bytes`, and read the file into a
/// hashed blob. Returns `PathTraversal` on any escape.
async fn read_within_root(
    root: &Path,
    candidate: &Path,
    max_bytes: u64,
) -> Result<FetchedBlob, FileFetcherError> {
    let root_canon = tokio::fs::canonicalize(root).await.map_err(|e| {
        FileFetcherError::Io(std::io::Error::new(
            e.kind(),
            format!("canonicalize root {}: {e}", root.display()),
        ))
    })?;
    // Reject `..` segments before touching the filesystem so a missing
    // file doesn't swallow a traversal attempt.
    if candidate
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(FileFetcherError::PathTraversal(
            candidate.display().to_string(),
        ));
    }
    let target_canon = tokio::fs::canonicalize(candidate).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            FileFetcherError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{}: not found", candidate.display()),
            ))
        } else {
            FileFetcherError::Io(e)
        }
    })?;
    if !target_canon.starts_with(&root_canon) {
        return Err(FileFetcherError::PathTraversal(
            target_canon.display().to_string(),
        ));
    }

    let meta = tokio::fs::metadata(&target_canon).await?;
    if meta.len() > max_bytes {
        return Err(FileFetcherError::SizeLimit {
            got: meta.len(),
            limit: max_bytes,
        });
    }
    let data = tokio::fs::read(&target_canon).await?;
    let got_len = data.len() as u64;
    if got_len > max_bytes {
        return Err(FileFetcherError::SizeLimit {
            got: got_len,
            limit: max_bytes,
        });
    }
    let mut hasher = Sha256::new();
    hasher.update(&data);
    Ok(FetchedBlob {
        data: Bytes::from(data),
        mime: None,
        sha256: hex_lower(&hasher.finalize()),
        total_bytes: got_len,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_lower_matches_known_vector() {
        // sha256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let mut h = Sha256::new();
        h.update(b"");
        assert_eq!(
            hex_lower(&h.finalize()),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn strip_scheme_is_exact() {
        assert_eq!(strip_scheme("file:///a/b", "file://"), Some("/a/b"));
        assert_eq!(strip_scheme("ws-tool://r/p", "ws-tool://"), Some("r/p"));
        assert_eq!(strip_scheme("http://x", "file://"), None);
    }
}
