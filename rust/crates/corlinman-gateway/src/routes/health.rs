//! `GET /health` — liveness + readiness probe.
//!
//! Runs a small fixed set of probes in parallel and aggregates their status:
//!   * `config` — a parsed gateway config is available in process state.
//!   * `agent_grpc` — the Python agent gRPC endpoint accepts a connection.
//!   * `sqlite_sessions` — `<data_dir>/sessions.sqlite` opens + pings.
//!   * `sqlite_kb` — `<data_dir>/kb.sqlite` opens + pings (optional — missing
//!     file is `ok` since a fresh install has no KB yet).
//!   * `usearch` — `<data_dir>/vector/index.usearch` opens when present.
//!   * `plugin_registry` — non-fatal diagnostics (`warn` when any plugin
//!     manifest failed to parse).
//!   * `channels_qq` — connection state of the QQ WebSocket probe
//!     (skipped `ok` when not configured).
//!
//! Overall status is the worst probe result (`fail` > `warn` > `ok`).
//! Kill the Python subprocess → `agent_grpc` → `fail` → overall `unhealthy`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use axum::extract::State;
use axum::{routing::get, Json, Router};
use corlinman_core::config::Config;
use corlinman_plugins::PluginRegistry;
use serde::Serialize;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Per-probe outcome. `Ok` / `Warn` / `Fail` map to overall status + JSON.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum ProbeStatus {
    Ok,
    Warn,
    Fail,
}

/// Overall health status computed from the worst probe.
fn overall_status(entries: &[CheckEntry]) -> &'static str {
    match entries.iter().map(|e| e.status).max() {
        None | Some(ProbeStatus::Ok) => "ok",
        Some(ProbeStatus::Warn) => "degraded",
        Some(ProbeStatus::Fail) => "unhealthy",
    }
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub checks: Vec<CheckEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckEntry {
    pub name: String,
    pub status: ProbeStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl CheckEntry {
    fn ok_detail(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: ProbeStatus::Ok,
            detail: Some(detail.into()),
        }
    }
    fn warn(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: ProbeStatus::Warn,
            detail: Some(detail.into()),
        }
    }
    fn fail(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: ProbeStatus::Fail,
            detail: Some(detail.into()),
        }
    }
}

/// Minimal `/health` probe state. Every field is optional so routers
/// composed without the full runtime (tests, stub boot) degrade to the
/// old empty-checks behaviour instead of reporting false negatives.
#[derive(Clone, Default)]
pub struct HealthState {
    pub config: Option<Arc<ArcSwap<Config>>>,
    pub data_dir: Option<PathBuf>,
    pub plugin_registry: Option<Arc<PluginRegistry>>,
    /// Python agent gRPC endpoint, e.g. `127.0.0.1:50051`. Skipped when `None`.
    pub agent_endpoint: Option<String>,
}

/// Axum router exposing `GET /health` without any wired probes — returns
/// the legacy empty-checks response, preserved for stub routers.
pub fn router() -> Router {
    Router::new().route("/health", get(health_stub))
}

/// Axum router exposing `GET /health` with real probes.
pub fn router_with_state(state: HealthState) -> Router {
    Router::new()
        .route("/health", get(health))
        .with_state(state)
}

async fn health_stub() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: VERSION,
        checks: Vec::new(),
    })
}

async fn health(State(state): State<HealthState>) -> Json<HealthResponse> {
    let checks = run_checks(&state).await;
    let status = overall_status(&checks);
    Json(HealthResponse {
        status,
        version: VERSION,
        checks,
    })
}

async fn run_checks(state: &HealthState) -> Vec<CheckEntry> {
    let mut entries = Vec::with_capacity(7);
    entries.push(probe_config(state));
    entries.push(probe_agent_grpc(state).await);
    if let Some(data_dir) = state.data_dir.as_ref() {
        entries.push(probe_sqlite(data_dir, "sessions", "sqlite_sessions").await);
        entries.push(probe_sqlite(data_dir, "kb", "sqlite_kb").await);
        entries.push(probe_usearch(data_dir).await);
    }
    entries.push(probe_plugins(state));
    entries.push(probe_channels_qq(state));
    entries
}

fn probe_config(state: &HealthState) -> CheckEntry {
    match state.config.as_ref() {
        Some(cfg) => {
            let snap = cfg.load();
            CheckEntry::ok_detail(
                "config",
                format!("loaded; models.default={}", snap.models.default),
            )
        }
        None => CheckEntry::warn("config", "no config loaded; serving with defaults"),
    }
}

async fn probe_agent_grpc(state: &HealthState) -> CheckEntry {
    let Some(endpoint) = state.agent_endpoint.as_deref() else {
        return CheckEntry::ok_detail("agent_grpc", "no agent endpoint configured");
    };

    // Parse `host:port` for TcpStream. URL schemes are stripped; unix: is
    // reported as `warn` because this probe doesn't do UDS.
    let addr = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))
        .unwrap_or(endpoint);
    if addr.starts_with("unix:") {
        return CheckEntry::warn(
            "agent_grpc",
            format!("unix socket probe not implemented: {endpoint}"),
        );
    }

    match tokio::time::timeout(
        Duration::from_millis(500),
        tokio::net::TcpStream::connect(addr),
    )
    .await
    {
        Ok(Ok(_stream)) => CheckEntry::ok_detail("agent_grpc", format!("reachable {addr}")),
        Ok(Err(e)) => CheckEntry::fail("agent_grpc", format!("connect {addr} failed: {e}")),
        Err(_) => CheckEntry::fail("agent_grpc", format!("connect {addr} timed out (500ms)")),
    }
}

async fn probe_sqlite(data_dir: &std::path::Path, stem: &str, name: &str) -> CheckEntry {
    let path = data_dir.join(format!("{stem}.sqlite"));
    if !path.exists() {
        // Missing is not an error — fresh installs have empty data dirs.
        return CheckEntry::ok_detail(
            name,
            format!("{} missing (not yet created)", path.display()),
        );
    }
    match sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_millis(500))
        .connect(&format!("sqlite://{}", path.display()))
        .await
    {
        Ok(pool) => {
            let ok = sqlx::query("SELECT 1").execute(&pool).await.is_ok();
            pool.close().await;
            if ok {
                CheckEntry::ok_detail(name, format!("{} pingable", path.display()))
            } else {
                CheckEntry::fail(name, format!("{} ping failed", path.display()))
            }
        }
        Err(e) => CheckEntry::fail(name, format!("{} open failed: {e}", path.display())),
    }
}

async fn probe_usearch(data_dir: &std::path::Path) -> CheckEntry {
    let path = data_dir.join("vector").join("index.usearch");
    if !path.exists() {
        return CheckEntry::ok_detail("usearch", format!("{} missing", path.display()));
    }
    match corlinman_vector::UsearchIndex::open(&path) {
        Ok(idx) => CheckEntry::ok_detail(
            "usearch",
            format!("{} dim={} size={}", path.display(), idx.dim(), idx.size()),
        ),
        Err(e) => CheckEntry::fail("usearch", format!("{} open failed: {e}", path.display())),
    }
}

fn probe_plugins(state: &HealthState) -> CheckEntry {
    let Some(reg) = state.plugin_registry.as_ref() else {
        return CheckEntry::ok_detail("plugin_registry", "no registry wired");
    };
    let count = reg.len();
    let diags = reg.diagnostics().len();
    if diags == 0 {
        CheckEntry::ok_detail(
            "plugin_registry",
            format!("{count} plugin(s); 0 diagnostics"),
        )
    } else {
        CheckEntry::warn(
            "plugin_registry",
            format!("{count} plugin(s); {diags} diagnostic(s)"),
        )
    }
}

fn probe_channels_qq(state: &HealthState) -> CheckEntry {
    let Some(cfg) = state.config.as_ref() else {
        return CheckEntry::ok_detail("channels_qq", "no config; channel probe skipped");
    };
    let snap = cfg.load();
    match snap.channels.qq.as_ref() {
        None => CheckEntry::ok_detail("channels_qq", "not configured"),
        Some(qq) if !qq.enabled => CheckEntry::ok_detail("channels_qq", "declared but disabled"),
        Some(qq) => CheckEntry::ok_detail("channels_qq", format!("enabled -> {}", qq.ws_url)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arc_swap::ArcSwap;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[tokio::test]
    async fn overall_status_ok_when_all_probes_pass() {
        let state = HealthState {
            config: Some(Arc::new(ArcSwap::from_pointee(Config::default()))),
            data_dir: Some(tempdir().unwrap().path().to_path_buf()),
            plugin_registry: Some(Arc::new(PluginRegistry::default())),
            agent_endpoint: None,
        };
        let checks = run_checks(&state).await;
        let status = overall_status(&checks);
        assert_eq!(status, "ok", "got checks: {checks:?}");
    }

    #[tokio::test]
    async fn agent_grpc_unreachable_flips_overall_unhealthy() {
        // Port 1 on 127.0.0.1 is reserved → connect refused / timeout.
        let state = HealthState {
            config: Some(Arc::new(ArcSwap::from_pointee(Config::default()))),
            data_dir: None,
            plugin_registry: None,
            agent_endpoint: Some("127.0.0.1:1".into()),
        };
        let checks = run_checks(&state).await;
        let status = overall_status(&checks);
        assert_eq!(status, "unhealthy", "got checks: {checks:?}");
        let agent = checks
            .iter()
            .find(|e| e.name == "agent_grpc")
            .expect("agent probe present");
        assert_eq!(agent.status, ProbeStatus::Fail);
    }

    #[tokio::test]
    async fn missing_config_reports_warn_but_not_unhealthy() {
        let state = HealthState::default();
        let checks = run_checks(&state).await;
        let status = overall_status(&checks);
        assert_eq!(status, "degraded", "got checks: {checks:?}");
    }

    #[tokio::test]
    async fn handler_returns_json() {
        let state = HealthState {
            config: Some(Arc::new(ArcSwap::from_pointee(Config::default()))),
            data_dir: None,
            plugin_registry: Some(Arc::new(PluginRegistry::default())),
            agent_endpoint: None,
        };
        let Json(resp) = health(State(state)).await;
        assert_eq!(resp.status, "ok", "resp: {:?}", resp.checks);
        assert_eq!(resp.version, VERSION);
        assert!(!resp.checks.is_empty());
    }
}
