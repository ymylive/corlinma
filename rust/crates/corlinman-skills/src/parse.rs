//! Hand-rolled splitter for `---` YAML frontmatter + Markdown body.
//!
//! We deliberately avoid a dedicated frontmatter crate: the format is
//! trivial and we want verbatim body preservation (leading/trailing
//! whitespace intact) for downstream prompt injection.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::SkillLoadError;
use crate::skill::{Skill, SkillRequirements};

/// Raw frontmatter shape. We only deserialize what we care about;
/// unknown keys are ignored so skills can carry metadata for other
/// harnesses.
#[derive(Debug, Deserialize, Default)]
struct RawFrontmatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    metadata: Option<RawMetadata>,
    #[serde(default, rename = "allowed-tools")]
    allowed_tools: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RawMetadata {
    #[serde(default)]
    openclaw: Option<RawOpenclaw>,
}

#[derive(Debug, Deserialize, Default)]
struct RawOpenclaw {
    #[serde(default)]
    emoji: Option<String>,
    #[serde(default)]
    requires: Option<RawRequires>,
    #[serde(default)]
    install: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RawRequires {
    #[serde(default)]
    bins: Vec<String>,
    #[serde(default, rename = "anyBins")]
    any_bins: Vec<String>,
    #[serde(default)]
    config: Vec<String>,
    #[serde(default)]
    env: Vec<String>,
}

/// Split `input` into `(yaml_str, body_str)`. Returns `None` if the
/// file does not start with a `---` frontmatter fence.
///
/// Recognised fence: a line that is exactly `---` (optionally followed
/// by CR). The opening fence must be the very first line of the file.
fn split_frontmatter(input: &str) -> Option<(&str, &str)> {
    // Must start with `---` followed by newline.
    let rest = input
        .strip_prefix("---\n")
        .or_else(|| input.strip_prefix("---\r\n"))?;

    // Find closing fence: a line that is exactly `---`.
    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" {
            let yaml = &rest[..offset];
            let body_start = offset + line.len();
            let body = &rest[body_start..];
            return Some((yaml, body));
        }
        offset += line.len();
    }
    None
}

/// Parse a single skill file's raw text into a [`Skill`].
pub(crate) fn parse_skill(source_path: PathBuf, text: &str) -> Result<Skill, SkillLoadError> {
    let (yaml_str, body) = split_frontmatter(text).ok_or(SkillLoadError::MissingField {
        path: source_path.clone(),
        field: "frontmatter",
    })?;

    let raw: RawFrontmatter =
        serde_yaml::from_str(yaml_str).map_err(|err| SkillLoadError::YamlParse {
            path: source_path.clone(),
            err,
        })?;

    let name = required_non_empty(raw.name, &source_path, "name")?;
    let description = required_non_empty(raw.description, &source_path, "description")?;

    let openclaw = raw.metadata.and_then(|m| m.openclaw).unwrap_or_default();
    let requires_raw = openclaw.requires.unwrap_or_default();

    Ok(Skill {
        name,
        description,
        emoji: openclaw.emoji,
        requires: SkillRequirements {
            bins: requires_raw.bins,
            any_bins: requires_raw.any_bins,
            config: requires_raw.config,
            env: requires_raw.env,
        },
        install: openclaw.install,
        allowed_tools: raw.allowed_tools,
        body_markdown: body.to_string(),
        source_path,
    })
}

fn required_non_empty(
    value: Option<String>,
    path: &Path,
    field: &'static str,
) -> Result<String, SkillLoadError> {
    match value {
        Some(v) if !v.trim().is_empty() => Ok(v),
        _ => Err(SkillLoadError::MissingField {
            path: path.to_path_buf(),
            field,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_frontmatter_simple() {
        let input = "---\nname: foo\n---\nbody text\n";
        let (yaml, body) = split_frontmatter(input).unwrap();
        assert_eq!(yaml, "name: foo\n");
        assert_eq!(body, "body text\n");
    }

    #[test]
    fn split_frontmatter_missing_close() {
        let input = "---\nname: foo\nno close\n";
        assert!(split_frontmatter(input).is_none());
    }

    #[test]
    fn split_frontmatter_no_fence() {
        assert!(split_frontmatter("hello").is_none());
    }
}
