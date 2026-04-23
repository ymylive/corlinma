//! B4 integration chain — tests 1-4.
//!
//! These exercise the primitives landed in Batch 4 (Telegram webhook,
//! Channel trait registry, WsTool + FileFetcher, ApprovalGate on the
//! HookBus) by wiring each subsystem against a real `HookBus` in-process
//! and asserting hook event ordering / shape on each transition.
//!
//! Everything here is loopback + test-time only. No network, no sleeps
//! besides `tokio::time::sleep` for bounded spin waits (< 2s) when the
//! underlying primitive doesn't expose a deterministic wakeup hook.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use corlinman_channels::channel::{spawn_all, ChannelContext, ChannelRegistry};
use corlinman_channels::telegram::media::{MediaError, TelegramHttp};
use corlinman_channels::telegram::types::{File as TgFile, Update};
use corlinman_channels::telegram::webhook::{process_update, WebhookCtx};
use corlinman_core::config::{
    ChannelsConfig, Config, SecretRef, TelegramChannelConfig, TelegramRateLimit,
};
use corlinman_gateway::middleware::approval::{ApprovalDecision, ApprovalGate};
use corlinman_gateway_api::{ChatEventStream, ChatService, InternalChatRequest};
use corlinman_hooks::{HookBus, HookEvent, HookPriority};
use corlinman_wstool::{
    file_server_advert, file_server_handler, DiskFileServer, FileFetcher, WsToolConfig,
    WsToolRunner, WsToolServer, FILE_FETCHER_TOOL,
};
use futures::stream::{self, Stream};
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tokio::time::{timeout, Instant};
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Shared fake TelegramHttp — mirrors the one inside
// `corlinman_channels::telegram::webhook::tests` but is kept local because the
// module-private fake isn't re-exported (nor should it be; tests own their
// own doubles). See `telegram::webhook::tests::FakeHttp` for the canonical.
// ---------------------------------------------------------------------------

struct FakeHttp {
    file_path: Option<String>,
    bytes: Vec<u8>,
}

impl FakeHttp {
    fn new(path: &str, bytes: Vec<u8>) -> Self {
        Self {
            file_path: Some(path.into()),
            bytes,
        }
    }
}

#[async_trait]
impl TelegramHttp for FakeHttp {
    async fn get_file(&self, file_id: &str) -> Result<TgFile, MediaError> {
        Ok(TgFile {
            file_id: file_id.into(),
            file_unique_id: Some(format!("u_{file_id}")),
            file_size: Some(self.bytes.len() as i64),
            file_path: self.file_path.clone(),
        })
    }

    async fn download_stream(
        &self,
        _file_path: &str,
    ) -> Result<Box<dyn Stream<Item = Result<Bytes, MediaError>> + Send + Unpin>, MediaError> {
        let bytes = Bytes::copy_from_slice(&self.bytes);
        Ok(Box::new(Box::pin(stream::iter(vec![Ok::<_, MediaError>(
            bytes,
        )]))))
    }
}

fn voice_update() -> Update {
    // Matches the shape used in
    // `corlinman_channels::telegram::webhook::tests::update_private_voice`.
    serde_json::from_value(json!({
        "update_id": 42,
        "message": {
            "message_id": 5,
            "from": { "id": 7, "is_bot": false },
            "chat": { "id": 7, "type": "private" },
            "date": 0,
            "voice": { "file_id": "V42", "duration": 3 }
        }
    }))
    .unwrap()
}

// ---------------------------------------------------------------------------
// Test 1 — Telegram voice triggers both MessageReceived + MessageTranscribed.
// ---------------------------------------------------------------------------

/// Pins the B4-BE1 contract: a voice update on the webhook path emits
/// `MessageReceived` first, followed by `MessageTranscribed`, both
/// carrying the per-webhook session_key. We call `process_update`
/// directly (the public entry on the `webhook` module) rather than
/// booting axum, which is the boundary the other subsystems cross.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn telegram_voice_triggers_transcribed_and_received_hooks() {
    let td = TempDir::new().unwrap();
    let http = FakeHttp::new("voice/a.oga", b"ogg-bytes".to_vec());
    let bus = HookBus::new(32);
    let mut sub = bus.subscribe(HookPriority::Normal);

    let ctx = WebhookCtx {
        bot_id: 999,
        bot_username: Some("corlinman_bot"),
        data_dir: td.path(),
        http: &http,
        hooks: Some(&bus),
    };

    let out = process_update(&ctx, voice_update())
        .await
        .expect("webhook ok")
        .expect("processed some");
    assert_eq!(out.media_kind, "voice");

    // Order is guaranteed by `process_update`: MessageReceived, then
    // MessageTranscribed (see `telegram::webhook::process_update`).
    let first = timeout(Duration::from_secs(1), sub.recv())
        .await
        .expect("first event within 1s")
        .expect("channel alive");
    match first {
        HookEvent::MessageReceived {
            channel,
            session_key,
            ..
        } => {
            assert_eq!(channel, "telegram");
            assert_eq!(session_key, "telegram:7:7");
        }
        other => panic!("expected MessageReceived, got {}", other.kind()),
    }

    let second = timeout(Duration::from_secs(1), sub.recv())
        .await
        .expect("second event within 1s")
        .expect("channel alive");
    match second {
        HookEvent::MessageTranscribed {
            session_key,
            transcript,
            media_type,
            media_path,
        } => {
            assert_eq!(session_key, "telegram:7:7");
            // Real STT lands later — current stub emits empty string.
            assert_eq!(transcript, "");
            assert_eq!(media_type, "voice");
            assert!(!media_path.is_empty(), "media_path must be populated");
        }
        other => panic!("expected MessageTranscribed, got {}", other.kind()),
    }
}

// ---------------------------------------------------------------------------
// Test 2 — WsTool + FileFetcher roundtrip emits ToolCalled with ok=true.
// ---------------------------------------------------------------------------

/// Pins the B4-BE3 + B4-BE4 contract:
/// 1. A runner advertising `__file_fetcher__/read` can serve a file from
///    `DiskFileServer` when addressed via `ws-tool://<runner>/<path>`.
/// 2. The response bytes + sha256 round-trip exactly.
/// 3. The `HookEvent::ToolCalled` fan-out from the server side records
///    `ok=true` with the expected `tool` + `runner_id`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ws_tool_plus_filefetcher_roundtrip() {
    let tmp_root = TempDir::new().unwrap();
    let payload = b"hello tail-wind".to_vec();
    std::fs::write(tmp_root.path().join("hello.txt"), &payload).unwrap();

    let bus = Arc::new(HookBus::new(64));
    let mut sub = bus.subscribe(HookPriority::Normal);

    let cfg = WsToolConfig::loopback("test-token");
    let server = Arc::new(WsToolServer::new(cfg, bus.clone()));
    let addr = server.bind().await.expect("bind loopback");
    let ws_url = format!("ws://{addr}");

    // Runner: serve the reserved tool backed by DiskFileServer.
    let disk = DiskFileServer::new(tmp_root.path().to_path_buf(), 100 * 1024 * 1024);
    let handler = file_server_handler(disk);
    let runner = WsToolRunner::connect(
        &ws_url,
        "test-token",
        "test-runner",
        vec![file_server_advert()],
    )
    .await
    .expect("runner connect");
    let _serve = tokio::spawn(async move {
        let _ = runner.serve_with(handler).await;
    });

    // Wait for the tool advert to land — bounded spin, no sleeps.
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if server.advertised_tools().contains_key(FILE_FETCHER_TOOL) {
            break;
        }
        if Instant::now() > deadline {
            panic!("runner never advertised FILE_FETCHER_TOOL");
        }
        tokio::task::yield_now().await;
    }

    let fetcher = FileFetcher::new(None, reqwest::Client::new(), 100 * 1024 * 1024)
        .with_ws_server(server.state());
    let blob = fetcher
        .fetch("ws-tool://test-runner/hello.txt")
        .await
        .expect("ws-tool fetch");

    assert_eq!(blob.data.as_ref(), payload.as_slice(), "bytes must match");
    let mut h = Sha256::new();
    h.update(&payload);
    let expected_sha = format!("{:x}", h.finalize());
    assert_eq!(blob.sha256, expected_sha, "sha256 must match");
    assert_eq!(blob.total_bytes, payload.len() as u64);

    // Drain hook events until we land on the ToolCalled for this invocation
    // (the bus may surface other fan-out events first if any were emitted).
    let tool_evt = timeout(Duration::from_secs(2), async move {
        loop {
            match sub.recv().await {
                Ok(HookEvent::ToolCalled {
                    tool,
                    runner_id,
                    ok,
                    error_code,
                    ..
                }) => {
                    return (tool, runner_id, ok, error_code);
                }
                Ok(_) => continue,
                Err(e) => panic!("bus closed: {e:?}"),
            }
        }
    })
    .await
    .expect("ToolCalled within 2s");

    assert_eq!(tool_evt.0, FILE_FETCHER_TOOL);
    assert_eq!(tool_evt.1, "test-runner");
    assert!(tool_evt.2, "ok flag must be true on success");
    assert!(tool_evt.3.is_none(), "error_code is None on success");

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// Test 3 — ApprovalGate emits Requested then Decided on the bus.
// ---------------------------------------------------------------------------

/// Pins the B4-BE6 contract: with a `HookBus` attached, a `Prompt` rule
/// resolved by an operator fires `ApprovalRequested` **before**
/// `ApprovalDecided` with the expected field shapes (id carried through,
/// decision == "allow", decider preserved).
///
/// We also cover the deterministic timeout branch (`prompt_wait`'s
/// `Timeout` future) by using a tiny default_timeout so the wait fires
/// within the test's 5s budget.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn approval_path_emits_paired_hook_events() {
    use corlinman_core::config::{ApprovalMode, ApprovalRule};
    use corlinman_vector::SqliteStore;

    // ---- Shared scaffolding ----
    let tmp = TempDir::new().unwrap();
    let store = SqliteStore::open(&tmp.path().join("kb.sqlite"))
        .await
        .expect("open sqlite");
    corlinman_vector::migration::ensure_schema(&store)
        .await
        .expect("ensure_schema");

    let rules = vec![ApprovalRule {
        plugin: "plugin.x".into(),
        tool: Some("tool.y".into()),
        mode: ApprovalMode::Prompt,
        allow_session_keys: vec![],
    }];
    let bus = Arc::new(HookBus::new(32));
    let gate =
        ApprovalGate::new(rules, Arc::new(store), Duration::from_secs(5)).with_bus(bus.clone());

    // ---- Path A: allow via resolve() ----
    let mut sub = bus.subscribe(HookPriority::Normal);

    let gate_a = gate.clone();
    let handle = tokio::spawn(async move {
        gate_a
            .check(
                "session-xyz",
                "plugin.x",
                "tool.y",
                br#"{"arg":1}"#,
                CancellationToken::new(),
            )
            .await
    });

    // First bus event MUST be ApprovalRequested with the matching triple.
    let first = timeout(Duration::from_secs(2), sub.recv())
        .await
        .expect("Requested within 2s")
        .expect("bus alive");
    let id = match first {
        HookEvent::ApprovalRequested {
            id,
            session_key,
            plugin,
            tool,
            args_preview,
            timeout_at_ms,
        } => {
            assert_eq!(session_key, "session-xyz");
            assert_eq!(plugin, "plugin.x");
            assert_eq!(tool, "tool.y");
            assert!(
                args_preview.contains("\"arg\":1"),
                "args_preview should surface the raw JSON text, got {args_preview:?}"
            );
            assert!(timeout_at_ms > 0, "timeout_at_ms must be populated");
            // Contract surprise to record in report: id is a plain UUID
            // (hyphenated v4) — not namespaced or prefixed. Callers that
            // want a namespaced id must wrap it outside the gate.
            assert!(
                id.len() == 36 && id.chars().filter(|c| *c == '-').count() == 4,
                "id must look like a UUID v4 ({id})"
            );
            id
        }
        other => panic!("expected ApprovalRequested, got {}", other.kind()),
    };

    gate.resolve(&id, ApprovalDecision::Approved)
        .await
        .expect("resolve");

    let second = timeout(Duration::from_secs(2), sub.recv())
        .await
        .expect("Decided within 2s")
        .expect("bus alive");
    match second {
        HookEvent::ApprovalDecided {
            id: ev_id,
            decision,
            decider,
            ..
        } => {
            assert_eq!(ev_id, id, "Decided must carry the same id as Requested");
            // Per hooks/src/event.rs — decision is "allow" / "deny" / "timeout".
            assert_eq!(decision, "allow");
            // `resolve()` today doesn't thread a decider through; this is
            // documented behaviour, not a missing test assertion.
            assert!(decider.is_none(), "resolve() leaves decider=None");
        }
        other => panic!("expected ApprovalDecided, got {}", other.kind()),
    }

    let result = handle.await.expect("join").expect("gate ok");
    assert_eq!(result, ApprovalDecision::Approved);

    // ---- Path B: timeout branch ----
    // Fresh bus subscription so we ignore the events from path A.
    let mut sub2 = bus.subscribe(HookPriority::Normal);

    // Re-open the sqlite and build a gate with a tiny timeout. A fresh
    // store is used so the timeout path lands on an empty table.
    let tmp2 = TempDir::new().unwrap();
    let store2 = SqliteStore::open(&tmp2.path().join("kb.sqlite"))
        .await
        .expect("open sqlite 2");
    corlinman_vector::migration::ensure_schema(&store2)
        .await
        .expect("ensure_schema 2");
    let gate_timeout = ApprovalGate::new(
        vec![ApprovalRule {
            plugin: "plugin.x".into(),
            tool: Some("tool.y".into()),
            mode: ApprovalMode::Prompt,
            allow_session_keys: vec![],
        }],
        Arc::new(store2),
        Duration::from_millis(80),
    )
    .with_bus(bus.clone());

    let decision = gate_timeout
        .check(
            "session-t",
            "plugin.x",
            "tool.y",
            b"{}",
            CancellationToken::new(),
        )
        .await
        .expect("gate ok");
    assert_eq!(decision, ApprovalDecision::Timeout);

    // We expect Requested then Decided { decision: "timeout" }.
    let first_t = timeout(Duration::from_secs(2), sub2.recv())
        .await
        .expect("Requested within 2s")
        .expect("bus alive");
    assert!(
        matches!(first_t, HookEvent::ApprovalRequested { .. }),
        "first event must be Requested, got {}",
        first_t.kind()
    );
    let second_t = timeout(Duration::from_secs(2), sub2.recv())
        .await
        .expect("Decided within 2s")
        .expect("bus alive");
    match second_t {
        HookEvent::ApprovalDecided { decision, .. } => {
            assert_eq!(decision, "timeout");
        }
        other => panic!("expected ApprovalDecided(timeout), got {}", other.kind()),
    }
}

// ---------------------------------------------------------------------------
// Test 4 — ChannelRegistry::builtin() respects channels.telegram.enabled.
// ---------------------------------------------------------------------------

/// Pins the B4-BE2 enabled-flag contract: flipping
/// `channels.telegram.enabled` changes the count of handles returned by
/// `spawn_all`. The config also needs a bot_token so the adapter's run
/// body can reach its inner logic; we provide a Literal SecretRef.
///
/// We cancel the spawned tasks immediately so the test does not wait on
/// their outcomes (the adapter would otherwise attempt to dial
/// `api.telegram.org`; cancel aborts before that resolves).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn channel_registry_telegram_respects_enabled_flag() {
    #[derive(Clone)]
    struct NoopChat;
    #[async_trait]
    impl ChatService for NoopChat {
        async fn run(
            &self,
            _req: InternalChatRequest,
            _cancel: CancellationToken,
        ) -> ChatEventStream {
            Box::pin(stream::empty())
        }
    }

    fn ctx_with(cfg: Config) -> ChannelContext {
        ChannelContext {
            config: Arc::new(cfg),
            chat_service: Arc::new(NoopChat),
            model: "test-model".into(),
            rate_limit_hook: None,
            hook_bus: None,
        }
    }

    // --- Disabled case ---
    let cfg_off = Config {
        channels: ChannelsConfig {
            qq: None,
            telegram: Some(TelegramChannelConfig {
                enabled: false,
                bot_token: Some(SecretRef::Literal {
                    value: "1:fake".into(),
                }),
                allowed_chat_ids: vec![],
                keyword_filter: vec![],
                require_mention_in_groups: false,
                rate_limit: TelegramRateLimit::default(),
            }),
        },
        ..Config::default()
    };
    let cancel_off = CancellationToken::new();
    let handles_off = spawn_all(
        &ChannelRegistry::builtin(),
        ctx_with(cfg_off),
        cancel_off.clone(),
    );
    assert_eq!(
        handles_off.len(),
        0,
        "telegram disabled + qq absent → zero handles"
    );
    cancel_off.cancel();
    for h in handles_off {
        let _ = h.await;
    }

    // --- Enabled case ---
    let cfg_on = Config {
        channels: ChannelsConfig {
            qq: None,
            telegram: Some(TelegramChannelConfig {
                enabled: true,
                bot_token: Some(SecretRef::Literal {
                    value: "1:fake".into(),
                }),
                allowed_chat_ids: vec![],
                keyword_filter: vec![],
                require_mention_in_groups: false,
                rate_limit: TelegramRateLimit::default(),
            }),
        },
        ..Config::default()
    };
    let cancel_on = CancellationToken::new();
    let handles_on = spawn_all(
        &ChannelRegistry::builtin(),
        ctx_with(cfg_on),
        cancel_on.clone(),
    );
    assert_eq!(handles_on.len(), 1, "telegram enabled → one handle spawned");
    // Abort immediately: the adapter will attempt to dial Telegram
    // otherwise. Cancel short-circuits that reliably within the test
    // window.
    cancel_on.cancel();
    for h in handles_on {
        // Give the task up to 1s to unwind cleanly; if it doesn't, drop
        // the handle (task is aborted on drop of the JoinHandle for
        // abortable futures — here cancel ensures it exits).
        let _ = tokio::time::timeout(Duration::from_secs(1), h).await;
    }

    // Silence an unused-import warning on the `PathBuf` helper used by
    // other tests in the same binary; keeps the linter happy when this
    // test is run in isolation via `-- --exact`.
    let _ = PathBuf::new();
}
