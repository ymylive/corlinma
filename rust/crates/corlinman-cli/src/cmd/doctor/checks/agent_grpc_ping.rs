//! Ping the Python agent gRPC endpoint with a 500 ms TCP connect.
//!
//! We deliberately don't speak gRPC here — a TCP connect is enough to tell
//! whether the python process is up and bound to the expected address. The
//! endpoint resolution mirrors `corlinman_agent_client::resolve_endpoint`.

use std::time::Duration;

use async_trait::async_trait;
use tokio::time::timeout;

use super::{DoctorCheck, DoctorContext, DoctorResult};

pub struct AgentGrpcPingCheck;

impl AgentGrpcPingCheck {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AgentGrpcPingCheck {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve the endpoint using the same precedence the agent client uses.
fn resolve_endpoint() -> String {
    if let Ok(addr) = std::env::var("CORLINMAN_PY_ADDR") {
        return addr;
    }
    if let Ok(port) = std::env::var("CORLINMAN_PY_PORT") {
        return format!("127.0.0.1:{port}");
    }
    "127.0.0.1:50051".to_string()
}

async fn probe(endpoint: &str) -> DoctorResult {
    let addr = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))
        .unwrap_or(endpoint);
    match timeout(
        Duration::from_millis(500),
        tokio::net::TcpStream::connect(addr),
    )
    .await
    {
        Ok(Ok(_)) => DoctorResult::Ok {
            message: format!("reachable: {addr}"),
        },
        Ok(Err(e)) => DoctorResult::Warn {
            message: format!("connect {addr} failed: {e}"),
            hint: Some("run `corlinman-python-server` (or systemd unit) to start it".into()),
        },
        Err(_) => DoctorResult::Warn {
            message: format!("connect {addr} timed out (500ms)"),
            hint: Some("the python agent may be unreachable or not listening".into()),
        },
    }
}

#[async_trait]
impl DoctorCheck for AgentGrpcPingCheck {
    fn name(&self) -> &str {
        "agent_grpc_ping"
    }

    async fn run(&self, _ctx: &DoctorContext) -> DoctorResult {
        probe(&resolve_endpoint()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unreachable_port_is_warn() {
        // Port 1 on 127.0.0.1 is reserved → ConnRefused.
        let res = probe("127.0.0.1:1").await;
        assert_eq!(res.status_str(), "warn", "got: {res:?}");
    }

    #[tokio::test]
    async fn reachable_listener_is_ok() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let res = probe(&addr.to_string()).await;
        drop(listener);
        assert_eq!(res.status_str(), "ok", "got: {res:?}");
    }
}
