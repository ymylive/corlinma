//! Errors emitted while loading skill files off disk.

use std::path::PathBuf;
use thiserror::Error;

/// Failure modes for [`crate::SkillRegistry::load_from_dir`].
#[derive(Debug, Error)]
pub enum SkillLoadError {
    /// Filesystem walk or read failed.
    #[error("skill IO error: {0}")]
    Io(#[from] std::io::Error),

    /// The YAML frontmatter in `path` could not be parsed.
    #[error("skill YAML parse failed at {path}: {err}")]
    YamlParse {
        path: PathBuf,
        #[source]
        err: serde_yaml::Error,
    },

    /// Two skill files declared the same `name`.
    #[error(
        "duplicate skill name '{name}': first defined at {} then redefined at {}",
        first.display(),
        second.display()
    )]
    DuplicateName {
        name: String,
        first: PathBuf,
        second: PathBuf,
    },

    /// Required frontmatter field was missing or empty.
    #[error("skill at {path} is missing required field '{field}'")]
    MissingField { path: PathBuf, field: &'static str },
}
