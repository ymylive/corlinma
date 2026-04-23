//! `corlinman-wstool` — distributed tool-execution protocol over WebSocket.
//!
//! The plugin system's local `jsonrpc_stdio` runtime spawns a child
//! process and talks JSON-RPC over stdin/stdout. This crate generalises
//! that idea to a network socket: a runner (same host or different) dials
//! the gateway, advertises a set of tools, and serves invocations over a
//! multiplexed WebSocket connection.
//!
//! The crate is split into two halves that talk the same wire protocol
//! defined in [`message`]:
//!
//! - [`server`] runs inside the gateway and hosts `/wstool/connect`.
//! - [`runner`] is the client-side library a tool author imports into
//!   their own binary.
//!
//! [`runtime::WsToolRuntime`] bridges the server side into
//! [`corlinman_plugins::runtime::PluginRuntime`]: from the caller's
//! perspective a remote tool is indistinguishable from a locally spawned
//! plugin.
//!
//! All timing-sensitive loops (heartbeat, invocation timeout, reconnect
//! backoff) use `tokio::time`, so tests that want determinism can pause
//! the runtime's clock via `tokio::time::pause()`.

pub mod error;
pub mod file_fetcher;
pub mod message;
pub mod runner;
pub mod runtime;
pub mod server;

pub use error::{ToolError, WsToolError};
pub use file_fetcher::{
    file_server_advert, file_server_handler, DiskFileServer, FetchedBlob, FileFetcher,
    FileFetcherError, FileReadInfo, FileServer, FileServerHandler, FILE_FETCHER_TOOL,
};
pub use message::{ToolAdvert, WsToolMessage};
pub use runner::{ProgressSink, ToolHandler, WsToolRunner};
pub use runtime::WsToolRuntime;
pub use server::{WsToolConfig, WsToolServer};
