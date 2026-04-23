//! `corlinman-nodebridge` — v1 NodeBridge protocol + stub WebSocket
//! server.
//!
//! Scope (deliberate): this crate ships the **wire contract** for device
//! clients (iOS / Android / macOS / Linux / future Electron) to target.
//! It does *not* ship a real client. See
//! [`docs/protocols/nodebridge.md`] for the prose spec.
//!
//! What's here:
//!   - [`message::NodeBridgeMessage`] — tagged enum covering every frame
//!     the v1 protocol defines, JSON-serialized over WebSocket text
//!     frames.
//!   - [`server::NodeBridgeServer`] — an axum-based reference
//!     implementation. Accepts registrations, monitors heartbeats,
//!     routes `DispatchJob` to the first capable session, and forwards
//!     `Telemetry` to `corlinman_hooks::HookEvent::Telemetry`.
//!   - [`session::NodeSession`] — per-connection state held behind an
//!     `Arc` in the server's state map.
//!
//! The spec version advertised in the `Registered` frame is
//! [`server::SPEC_VERSION`] (`"1.0.0-alpha"`); bump it on any breaking
//! change to [`message::NodeBridgeMessage`].

pub mod error;
pub mod message;
pub mod server;
pub mod session;

pub use error::NodeBridgeError;
pub use message::{Capability, NodeBridgeMessage};
pub use server::{NodeBridgeServer, NodeBridgeServerConfig, SPEC_VERSION};
pub use session::NodeSession;
