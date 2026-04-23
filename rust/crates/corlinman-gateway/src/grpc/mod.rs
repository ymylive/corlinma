//! Rust-hosted gRPC services.
//!
//! Most corlinman gRPC surfaces are owned by the Python side
//! (`corlinman-server` hosts Agent / Embedding / etc on `/tmp/corlinman-py.sock`)
//! and the Rust gateway is a client. This module is the reverse direction:
//! services that *Python* dials against the Rust runtime.
//!
//! Currently hosts:
//!   * `Placeholder` — wraps [`corlinman_core::placeholder::PlaceholderEngine`]
//!     so the Python `context_assembler` can expand `{{namespace.name}}`
//!     tokens without re-implementing the resolver registry.

pub mod placeholder;

pub use placeholder::{
    serve as serve_placeholder, PlaceholderService, DEFAULT_RUST_SOCKET, ENV_RUST_SOCKET,
};
