//! `corlinman-skills` — registry for openclaw-style skill markdown files.
//!
//! A **skill** is a small Markdown file with YAML frontmatter describing
//! its identity, the runtime prerequisites it needs (binaries, config
//! keys, env vars), and the tools it is permitted to invoke. The body is
//! the prose the context assembler injects into an agent's prompt when
//! the skill is referenced by a session manifest.
//!
//! This crate is intentionally passive: it parses files off disk and
//! exposes lookups. Wiring into the context assembler / gateway happens
//! in a later workstream; the registry here is the data source that
//! step will consume.

mod error;
mod parse;
mod registry;
mod skill;

pub use error::SkillLoadError;
pub use registry::SkillRegistry;
pub use skill::{Skill, SkillRequirements};
