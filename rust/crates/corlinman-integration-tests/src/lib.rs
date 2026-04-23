//! `corlinman-integration-tests` — cross-crate smoke harness for Batch 1.
//!
//! This crate is intentionally empty as a library. The interesting code lives
//! under `tests/` so each scenario runs as its own binary and cannot leak state
//! between tests. See the workstream plan (rust-python-golden-pillow, B1-BE6).
