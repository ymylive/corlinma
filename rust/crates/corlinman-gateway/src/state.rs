//! `AppState` — cloneable bundle of shared handles.
//!
//! Currently holds the plugin registry so handlers (notably the chat route)
//! can dispatch `ServerFrame::ToolCall` frames to real plugin runtimes via
//! [`corlinman_plugins::PluginRegistry`]. Later milestones will extend this
//! with live config, the agent client, vector store, approval queue, and a
//! broadcast event bus (plan §14 R10).
//
// TODO: hold `config: Arc<ArcSwap<CorlinmanConfig>>` for lock-free hot reload;
//       every handler calls `state.config.load()` at entry.
// TODO: include `agent: corlinman_agent_client::AgentClient`,
//       `vector: corlinman_vector::Store`, `approvals: ApprovalQueue`, and a
//       broadcast `events: tokio::sync::broadcast::Sender<Event>`.

use std::sync::Arc;

use corlinman_plugins::PluginRegistry;

/// Process-wide shared handles. Cheap to clone — every field is `Arc`-wrapped.
#[derive(Clone)]
pub struct AppState {
    /// Discovered plugin manifests. Populated once at boot; later milestones
    /// will hot-reload via `notify`.
    pub plugin_registry: Arc<PluginRegistry>,
}

impl AppState {
    /// Build an `AppState` with the supplied registry. Callers wire this in
    /// from `main.rs` after discovery runs.
    pub fn new(plugin_registry: Arc<PluginRegistry>) -> Self {
        Self { plugin_registry }
    }

    /// Convenience constructor for tests / stubs that don't need any plugins.
    pub fn empty() -> Self {
        Self {
            plugin_registry: Arc::new(PluginRegistry::default()),
        }
    }
}
