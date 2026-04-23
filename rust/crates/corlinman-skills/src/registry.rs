//! In-memory skill registry loaded from a directory tree.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::debug;

use crate::error::SkillLoadError;
use crate::parse::parse_skill;
use crate::skill::Skill;

/// Owns the set of skills loaded from disk and provides lookups plus
/// runtime requirement checks.
#[derive(Debug, Default, Clone)]
pub struct SkillRegistry {
    skills: HashMap<String, Arc<Skill>>,
}

impl SkillRegistry {
    /// Walk `root` recursively and parse every `*.md` file into a
    /// [`Skill`]. Duplicate `name` fields are a hard error: the second
    /// occurrence wins nothing, we refuse to load at all.
    pub fn load_from_dir(root: impl AsRef<Path>) -> Result<Self, SkillLoadError> {
        let root = root.as_ref();
        let mut skills: HashMap<String, Arc<Skill>> = HashMap::new();

        if !root.exists() {
            // An absent skills dir just means "no skills"; the context
            // assembler treats this as a benign empty registry.
            debug!(path = %root.display(), "skills directory does not exist; empty registry");
            return Ok(Self { skills });
        }

        let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let path = entry.path();
                let ft = entry.file_type()?;
                if ft.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !ft.is_file() {
                    continue;
                }
                if path.extension().and_then(|s| s.to_str()) != Some("md") {
                    continue;
                }

                let text = fs::read_to_string(&path)?;
                let skill = parse_skill(path.clone(), &text)?;
                if let Some(existing) = skills.get(&skill.name) {
                    return Err(SkillLoadError::DuplicateName {
                        name: skill.name.clone(),
                        first: existing.source_path.clone(),
                        second: path,
                    });
                }
                debug!(name = %skill.name, path = %path.display(), "loaded skill");
                skills.insert(skill.name.clone(), Arc::new(skill));
            }
        }

        Ok(Self { skills })
    }

    /// Look up a skill by its `name` field.
    pub fn get(&self, name: &str) -> Option<&Arc<Skill>> {
        self.skills.get(name)
    }

    /// Iterate over all loaded skills in unspecified order.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<Skill>> {
        self.skills.values()
    }

    /// Sorted list of all skill names, handy for CLI listings.
    pub fn names(&self) -> Vec<String> {
        let mut out: Vec<String> = self.skills.keys().cloned().collect();
        out.sort();
        out
    }

    /// Verify every requirement for `skill_name`. Returns `Ok(())` if
    /// the skill can run; otherwise a list of actionable messages, one
    /// per unmet requirement.
    ///
    /// `config_lookup(key)` should return `Some(value)` for a set,
    /// non-empty config key and `None` otherwise.
    pub fn check_requirements(
        &self,
        skill_name: &str,
        config_lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<(), Vec<String>> {
        let skill = match self.skills.get(skill_name) {
            Some(s) => s,
            None => {
                return Err(vec![format!("skill '{skill_name}' is not registered")]);
            }
        };

        let mut problems: Vec<String> = Vec::new();
        let req = &skill.requires;

        for bin in &req.bins {
            if which::which(bin).is_err() {
                problems.push(format!(
                    "skill '{}' requires binary '{}' on $PATH; install it first",
                    skill.name, bin
                ));
            }
        }

        if !req.any_bins.is_empty() {
            let any_ok = req.any_bins.iter().any(|b| which::which(b).is_ok());
            if !any_ok {
                problems.push(format!(
                    "skill '{}' requires one of: {{{}}}; none found",
                    skill.name,
                    req.any_bins.join(", ")
                ));
            }
        }

        for key in &req.config {
            let present = config_lookup(key)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            if !present {
                problems.push(format!(
                    "skill '{}' requires config '{}' to be set (non-empty)",
                    skill.name, key
                ));
            }
        }

        for var in &req.env {
            let present = std::env::var(var).map(|v| !v.is_empty()).unwrap_or(false);
            if !present {
                problems.push(format!(
                    "skill '{}' requires env var '{}' to be set",
                    skill.name, var
                ));
            }
        }

        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems)
        }
    }
}
