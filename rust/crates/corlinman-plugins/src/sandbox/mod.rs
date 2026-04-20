//! Sandbox strategies for plugin execution.
//!
//! `DockerSandbox` + the `DockerRunner` trait live in [`docker`]; the trait is
//! surfaced here so `runtime::jsonrpc_stdio::execute` can dispatch through a
//! boxed runner (and tests can inject a mock without touching Docker).
//!
//! Byte-size parsing for `manifest.sandbox.memory` lives in [`bytes_parser`].

pub mod bytes_parser;
pub mod docker;

pub use bytes_parser::parse_bytes;
pub use docker::{DockerRunner, DockerSandbox};

use crate::manifest::SandboxConfig;

/// JSON-RPC error code we attach to `PluginOutput::Error` when the container
/// was OOM-killed. Mirrors the taxonomy note in `CorlinmanError::PluginRuntime`
/// and lets a downstream gateway bump `corlinman_plugin_execute_total{status="oom"}`
/// without reparsing the message.
pub const OOM_ERROR_CODE: i64 = -32010;

/// Whether a manifest's `[sandbox]` block actually asks for containerisation.
///
/// `SandboxConfig::default()` is "do nothing"; we treat the sandbox as enabled
/// the moment the author sets *any* meaningful field. This keeps the
/// dispatch in `execute()` honest without demanding an explicit `enable` flag
/// in every plugin manifest.
pub fn is_enabled(sb: &SandboxConfig) -> bool {
    sb.memory.is_some()
        || sb.cpus.is_some()
        || sb.read_only_root
        || !sb.cap_drop.is_empty()
        || sb.network.is_some()
        || !sb.binds.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sandbox_is_disabled() {
        assert!(!is_enabled(&SandboxConfig::default()));
    }

    #[test]
    fn any_field_enables_sandbox() {
        assert!(is_enabled(&SandboxConfig {
            memory: Some("64m".into()),
            ..Default::default()
        }));
        assert!(is_enabled(&SandboxConfig {
            read_only_root: true,
            ..Default::default()
        }));
        assert!(is_enabled(&SandboxConfig {
            cap_drop: vec!["ALL".into()],
            ..Default::default()
        }));
    }
}
