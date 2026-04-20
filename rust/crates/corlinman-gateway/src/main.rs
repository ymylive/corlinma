//! corlinman-gateway binary entry point.
//!
//! Boot sequence:
//!   1. install tracing_subscriber (JSON to stdout, `RUST_LOG` respected)
//!   2. resolve listen address (`PORT` env override, default 6005)
//!   3. build the axum router + shared `ChatBackend` handle
//!   4. optionally load `CORLINMAN_CONFIG` and, if `[channels.qq].enabled`,
//!      spawn the QQ channel task bound to the same backend
//!   5. serve axum with graceful-shutdown wired to SIGTERM/SIGINT
//!   6. on signal, cancel child tasks + `std::process::exit(143)`

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use corlinman_core::config::Config;
use corlinman_gateway::routes::chat::ChatBackend;
use corlinman_gateway::services::ChatService as GatewayChatService;
use corlinman_gateway::{server, shutdown};
use corlinman_gateway_api::ChatService as ChatServiceTrait;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() {
    init_tracing();

    let addr = resolve_addr();
    tracing::info!(%addr, "starting corlinman-gateway");

    // Root cancellation token. Cancels gRPC/channels/axum on shutdown.
    let root = CancellationToken::new();

    // Build router + keep a handle on the shared backend.
    let (router, backend) = server::build_runtime().await;

    // Optionally launch channel adapters.
    let mut channel_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    if let Some(backend) = backend.as_ref() {
        match load_config() {
            Ok(Some(cfg)) => {
                if let Some(handle) = maybe_spawn_qq_channel(&cfg, backend.clone(), root.clone()) {
                    channel_handles.push(handle);
                }
            }
            Ok(None) => {
                tracing::debug!("no CORLINMAN_CONFIG / config.toml found; channels disabled");
            }
            Err(err) => {
                tracing::warn!(error = %err, "config load failed; channels disabled");
            }
        }
    }

    let server_cancel = root.clone();
    let server_handle = tokio::spawn(async move {
        let shutdown_fut = {
            let token = server_cancel.clone();
            async move { token.cancelled().await }
        };
        if let Err(err) = server::run_with_router(addr, router, shutdown_fut).await {
            tracing::error!(error = %err, "gateway server crashed");
        }
    });

    let reason = shutdown::wait_for_signal().await;
    tracing::info!(?reason, "shutdown signal received, draining");
    root.cancel();

    if let Err(err) = server_handle.await {
        tracing::warn!(error = %err, "server task join failed");
    }
    for h in channel_handles {
        let _ = h.await;
    }

    std::process::exit(shutdown::EXIT_CODE_ON_SIGNAL);
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().json().with_current_span(false))
        .init();
}

fn resolve_addr() -> SocketAddr {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(6005);
    SocketAddr::from(([127, 0, 0, 1], port))
}

/// Load config from `CORLINMAN_CONFIG` if set; otherwise return `Ok(None)` so
/// the gateway can run without a config file (e.g. dev / tests).
fn load_config() -> anyhow::Result<Option<Config>> {
    let Some(path) = std::env::var("CORLINMAN_CONFIG").ok().map(PathBuf::from) else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let cfg = Config::load_from_path(&path)?;
    Ok(Some(cfg))
}

/// If `[channels.qq].enabled` is true, spawn the channel loop and return its
/// join handle. Otherwise returns `None`.
fn maybe_spawn_qq_channel(
    cfg: &Config,
    backend: Arc<dyn ChatBackend>,
    root: CancellationToken,
) -> Option<tokio::task::JoinHandle<()>> {
    let qq_cfg = cfg.channels.qq.as_ref()?;
    if !qq_cfg.enabled {
        return None;
    }
    let model = cfg.models.default.clone();
    let svc: Arc<dyn ChatServiceTrait> = Arc::new(GatewayChatService::new(backend));
    let params = corlinman_channels::service::QqChannelParams {
        config: qq_cfg.clone(),
        model,
        chat_service: svc,
    };
    let cancel = root.child_token();
    Some(tokio::spawn(async move {
        if let Err(err) = corlinman_channels::service::run_qq_channel(params, cancel).await {
            tracing::error!(error = %err, "qq channel task exited with error");
        }
    }))
}
