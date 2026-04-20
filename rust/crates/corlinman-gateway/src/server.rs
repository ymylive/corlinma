//! axum Router construction + HTTP server bootstrap.
//!
//! Later milestones fold the tonic gRPC server (VectorService + PluginBridge)
//! into this same entry point; this first revision only wires axum.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use corlinman_agent_client::client::{connect_channel, resolve_endpoint, AgentClient};
use corlinman_plugins::{roots_from_env_var, Origin, PluginRegistry, SearchRoot};
use tokio::net::TcpListener;

use crate::metrics;
use crate::middleware::trace;
use crate::routes;
use crate::routes::chat::{grpc::GrpcBackend, ChatBackend, ChatState};

/// Build the top-level axum router with the default (stub) chat route.
///
/// Returns 501 for `/v1/chat/completions` — use [`build_router_with_backend`]
/// to wire the real gRPC backend.
pub fn build_router() -> Router {
    metrics::init();
    trace::layer(routes::router())
}

/// Build the router with a concrete [`ChatBackend`]. Used both by `main` and
/// by integration tests that want a running handler.
///
/// Uses the M2 placeholder tool executor — suitable for tests that don't
/// care about plugin execution. Production boot goes through
/// [`build_router_for_runtime`] which loads a real [`PluginRegistry`].
pub fn build_router_with_backend(backend: Arc<dyn ChatBackend>) -> Router {
    metrics::init();
    let state = ChatState::new(backend);
    trace::layer(routes::router_with_chat_state(state))
}

/// Build the router with a backend and a plugin registry so the chat route
/// dispatches `ToolCall` frames to real plugin processes.
pub fn build_router_with_backend_and_registry(
    backend: Arc<dyn ChatBackend>,
    registry: Arc<PluginRegistry>,
) -> Router {
    metrics::init();
    let state = ChatState::with_registry(backend, registry);
    trace::layer(routes::router_with_chat_state(state))
}

/// Connect to the Python gRPC agent server; falls back to the stub router
/// when the agent isn't reachable (so `/health` stays up even if Python died).
pub async fn build_router_for_runtime() -> Router {
    build_runtime().await.0
}

/// Same as [`build_router_for_runtime`] but also returns the shared
/// [`ChatBackend`] when the agent was reachable, so callers (e.g. the QQ
/// channel task in `main`) can drive the chat pipeline without HTTP.
pub async fn build_runtime() -> (Router, Option<Arc<dyn ChatBackend>>) {
    let registry = Arc::new(load_plugin_registry());
    tracing::info!(
        plugin_count = registry.len(),
        diagnostic_count = registry.diagnostics().len(),
        "plugin registry loaded",
    );
    let endpoint = resolve_endpoint();
    match connect_channel(&endpoint).await {
        Ok(channel) => {
            tracing::info!(endpoint = %endpoint, "agent client connected");
            let client = AgentClient::new(channel);
            let backend: Arc<dyn ChatBackend> = Arc::new(GrpcBackend::new(client));
            (
                build_router_with_backend_and_registry(backend.clone(), registry),
                Some(backend),
            )
        }
        Err(err) => {
            tracing::warn!(
                endpoint = %endpoint,
                error = %err,
                "agent client unreachable; /v1/chat/completions will 501",
            );
            (build_router(), None)
        }
    }
}

/// Discover plugins from, in priority order:
///   1. `$CORLINMAN_DATA_DIR/plugins/` (user-installed),
///   2. each colon-separated entry in `$CORLINMAN_PLUGIN_EXTRA_DIRS`,
///   3. each colon-separated entry in `$CORLINMAN_PLUGIN_DIRS` (matches the CLI).
///
/// Missing directories are silently ignored so a fresh install boots cleanly.
fn load_plugin_registry() -> PluginRegistry {
    let mut roots: Vec<SearchRoot> = Vec::new();
    if let Ok(data_dir) = std::env::var("CORLINMAN_DATA_DIR") {
        let path = std::path::PathBuf::from(data_dir).join("plugins");
        roots.push(SearchRoot::new(path, Origin::Config));
    }
    roots.extend(roots_from_env_var(
        "CORLINMAN_PLUGIN_EXTRA_DIRS",
        Origin::Config,
    ));
    roots.extend(roots_from_env_var("CORLINMAN_PLUGIN_DIRS", Origin::Config));
    PluginRegistry::from_roots(roots)
}

/// Bind `addr` and serve until `shutdown` resolves.
pub async fn run<F>(addr: SocketAddr, shutdown: F) -> anyhow::Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let router = build_router_for_runtime().await;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "gateway listening");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

/// Variant of [`run`] that accepts a prebuilt router (produced by
/// [`build_runtime`]). Used when `main` needs the backend handle for
/// side-by-side channel tasks.
pub async fn run_with_router<F>(addr: SocketAddr, router: Router, shutdown: F) -> anyhow::Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "gateway listening");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}
