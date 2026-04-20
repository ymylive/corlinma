//! Gateway-internal service layer.
//!
//! These services expose the chat pipeline as callable Rust APIs so other
//! in-process components (channels, scheduler, admin jobs) can drive it
//! without a round-trip through HTTP.

pub mod chat_service;

pub use chat_service::ChatService;
