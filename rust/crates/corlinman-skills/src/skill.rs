//! Skill data model. Mirrors openclaw's SKILL.md frontmatter shape.

use std::path::PathBuf;

/// Runtime prerequisites a skill needs before it can execute. All lists
/// default to empty; an unmet item yields a human-readable message from
/// [`crate::SkillRegistry::check_requirements`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillRequirements {
    /// Every binary in this list must be found on `$PATH`.
    pub bins: Vec<String>,
    /// At least one binary in this list must be found on `$PATH`.
    pub any_bins: Vec<String>,
    /// Dotted config keys (e.g. `providers.brave.api_key`) that must
    /// resolve to a non-empty string via the caller-supplied lookup.
    pub config: Vec<String>,
    /// Environment variables that must be set to a non-empty value.
    pub env: Vec<String>,
}

/// A single skill parsed from a SKILL.md file on disk.
#[derive(Debug, Clone)]
pub struct Skill {
    /// Unique identifier. Used to look the skill up from a manifest's
    /// `skill_refs`.
    pub name: String,
    /// Short human summary shown in listings.
    pub description: String,
    /// Optional glyph used by the CLI/UI.
    pub emoji: Option<String>,
    /// Runtime prerequisites.
    pub requires: SkillRequirements,
    /// Optional install hint surfaced when `requires` isn't satisfied.
    pub install: Option<String>,
    /// Tools this skill is allowed to invoke at runtime. Enforcement
    /// happens elsewhere; we just carry the list.
    pub allowed_tools: Vec<String>,
    /// The Markdown body (everything after the closing `---` of the
    /// frontmatter), preserved verbatim.
    pub body_markdown: String,
    /// Absolute path to the file this skill was loaded from.
    pub source_path: PathBuf,
}
