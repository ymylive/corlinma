//! Integration tests for the multi-scheme [`FileFetcher`].
//!
//! The `ws-tool://` suite dials a real runner (via the shared `common`
//! harness) that serves the reserved `__file_fetcher__/read` tool
//! through [`DiskFileServer`] — so this file also exercises the
//! runner-side adapter end-to-end.

mod common;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;

use common::Harness;
use corlinman_wstool::{
    file_server_advert, file_server_handler, DiskFileServer, FileFetcher, FileFetcherError,
    WsToolRunner,
};

fn hex_of(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    let mut s = String::with_capacity(out.len() * 2);
    for b in out {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn fresh_root() -> tempdir_lite::Dir {
    tempdir_lite::Dir::new()
}

async fn spawn_http_server(
    body: Vec<u8>,
    content_type: &'static str,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let handler_body = body.clone();
    let app = Router::new().route(
        "/blob",
        get(move || {
            let b = handler_body.clone();
            async move { ([(axum::http::header::CONTENT_TYPE, content_type)], b) }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind http");
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app.into_make_service()).await;
    });
    (addr, handle)
}

#[tokio::test]
async fn fetch_file_uri_roundtrips_small_file_with_hash_match() {
    let dir = fresh_root();
    let payload = b"golden pillow contents".to_vec();
    let file_path = dir.path().join("hello.txt");
    std::fs::write(&file_path, &payload).unwrap();

    let fetcher = FileFetcher::new(
        Some(dir.path().to_path_buf()),
        reqwest::Client::new(),
        4 * 1024,
    );
    let uri = format!("file://{}", file_path.to_string_lossy());
    let blob = fetcher.fetch(&uri).await.expect("fetch file");
    assert_eq!(blob.data.as_ref(), payload.as_slice());
    assert_eq!(blob.total_bytes, payload.len() as u64);
    assert_eq!(blob.sha256, hex_of(&payload));
}

#[tokio::test]
async fn fetch_file_uri_rejects_path_traversal() {
    let dir = fresh_root();
    let inside = dir.path().join("ok.txt");
    std::fs::write(&inside, b"ok").unwrap();

    let fetcher = FileFetcher::new(
        Some(dir.path().to_path_buf()),
        reqwest::Client::new(),
        4 * 1024,
    );
    // Build a URI that contains `..` — must be rejected even if the
    // resolved path would exist.
    let traversal_uri = format!("file://{}/../ok.txt", dir.path().to_string_lossy());
    let err = fetcher
        .fetch(&traversal_uri)
        .await
        .expect_err("must reject");
    assert!(
        matches!(err, FileFetcherError::PathTraversal(_)),
        "expected PathTraversal, got {err:?}"
    );
}

#[tokio::test]
async fn fetch_http_uri_via_local_axum_server() {
    let payload: Vec<u8> = (0u8..=255).cycle().take(8 * 1024).collect();
    let (addr, _srv) = spawn_http_server(payload.clone(), "application/octet-stream").await;

    let fetcher = FileFetcher::new(None, reqwest::Client::new(), 1024 * 1024);
    let blob = fetcher
        .fetch(&format!("http://{addr}/blob"))
        .await
        .expect("http fetch");
    assert_eq!(blob.data.as_ref(), payload.as_slice());
    assert_eq!(blob.total_bytes, payload.len() as u64);
    assert_eq!(blob.sha256, hex_of(&payload));
    assert_eq!(blob.mime.as_deref(), Some("application/octet-stream"));
}

#[tokio::test]
async fn fetch_size_exceeds_limit_errors() {
    let dir = fresh_root();
    let path = dir.path().join("big.bin");
    std::fs::write(&path, vec![0u8; 2_048]).unwrap();

    let fetcher = FileFetcher::new(
        Some(dir.path().to_path_buf()),
        reqwest::Client::new(),
        /* max_bytes = */ 1_024,
    );
    let uri = format!("file://{}", path.to_string_lossy());
    let err = fetcher.fetch(&uri).await.expect_err("must reject");
    match err {
        FileFetcherError::SizeLimit { got, limit } => {
            assert_eq!(got, 2_048);
            assert_eq!(limit, 1_024);
        }
        other => panic!("expected SizeLimit, got {other:?}"),
    }
}

#[tokio::test]
async fn fetch_ws_tool_uri_from_runner_roundtrips() {
    let h = Harness::new().await;
    let dir = fresh_root();
    let payload = b"blob served over ws-tool".to_vec();
    std::fs::write(dir.path().join("doc.txt"), &payload).unwrap();

    let disk = DiskFileServer::new(dir.path().to_path_buf(), 16 * 1024);
    let handler = file_server_handler(disk);

    let runner = WsToolRunner::connect(
        &h.ws_url,
        &h.token,
        "file-runner",
        vec![file_server_advert()],
    )
    .await
    .expect("runner connect");
    let _serve = tokio::spawn(async move {
        let _ = runner.serve_with(handler).await;
    });

    // Wait for the advertised tool to land.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        if h.server
            .advertised_tools()
            .contains_key(corlinman_wstool::FILE_FETCHER_TOOL)
        {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("file_fetcher tool never registered");
        }
        tokio::task::yield_now().await;
    }

    let fetcher =
        FileFetcher::new(None, reqwest::Client::new(), 16 * 1024).with_ws_server(h.server.state());
    let blob = fetcher
        .fetch("ws-tool://file-runner/doc.txt")
        .await
        .expect("ws-tool fetch");
    assert_eq!(blob.data.as_ref(), payload.as_slice());
    assert_eq!(blob.total_bytes, payload.len() as u64);
    assert_eq!(blob.sha256, hex_of(&payload));
}

#[tokio::test]
async fn fetch_unknown_runner_errors() {
    let h = Harness::new().await;
    let fetcher =
        FileFetcher::new(None, reqwest::Client::new(), 1024).with_ws_server(h.server.state());
    let err = fetcher
        .fetch("ws-tool://nope/some/path")
        .await
        .expect_err("must reject");
    match err {
        FileFetcherError::UnknownRunner(id) => assert_eq!(id, "nope"),
        other => panic!("expected UnknownRunner, got {other:?}"),
    }
}

#[tokio::test]
async fn fetch_unsupported_scheme_errors() {
    let fetcher = FileFetcher::new(None, reqwest::Client::new(), 1024);
    let err = fetcher
        .fetch("ftp://example.com/x")
        .await
        .expect_err("must reject");
    match err {
        FileFetcherError::UnsupportedScheme(s) => assert_eq!(s, "ftp"),
        other => panic!("expected UnsupportedScheme, got {other:?}"),
    }
}

#[tokio::test]
#[ignore = "50MB loopback perf; opt-in via `cargo test -- --ignored`"]
async fn fetch_file_uri_50mb_under_2s() {
    let dir = fresh_root();
    let mut payload = vec![0u8; 50 * 1024 * 1024];
    // Vary the bytes so sha256 actually does work per-byte.
    for (i, b) in payload.iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(31);
    }
    let path = dir.path().join("big.bin");
    std::fs::write(&path, &payload).unwrap();

    let fetcher = FileFetcher::new(
        Some(dir.path().to_path_buf()),
        reqwest::Client::new(),
        128 * 1024 * 1024,
    );
    let uri = format!("file://{}", path.to_string_lossy());
    let started = std::time::Instant::now();
    let blob = fetcher.fetch(&uri).await.expect("fetch big");
    let elapsed = started.elapsed();
    assert_eq!(blob.total_bytes, 50 * 1024 * 1024);
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "50MB fetch took {elapsed:?}, budget 2s"
    );
    // silence unused warning for Arc imports when this test is ignored
    let _ = Arc::new(0u8);
    let _ = PathBuf::new();
}

// ---------------------------------------------------------------
// tempdir_lite — tiny scoped tempdir so we don't add `tempfile` as
// a dev-dep. Auto-removes on drop (best-effort).
// ---------------------------------------------------------------
mod tempdir_lite {
    use std::path::{Path, PathBuf};

    pub struct Dir {
        path: PathBuf,
    }

    impl Dir {
        pub fn new() -> Self {
            let base = std::env::temp_dir();
            let unique = format!("corlinman-wstool-ff-{}-{}", std::process::id(), next_seq());
            let path = base.join(unique);
            std::fs::create_dir_all(&path).expect("create tempdir");
            Self { path }
        }

        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn next_seq() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        N.fetch_add(1, Ordering::Relaxed)
    }
}
