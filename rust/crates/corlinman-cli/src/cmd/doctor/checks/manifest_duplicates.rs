//! Detect duplicate plugin names across the configured search roots.
//!
//! `corlinman_plugins::discover` tolerates duplicates — the last one wins —
//! but that means a stale global install silently shadows a local one (or
//! vice-versa). We surface it as a `Warn` so users notice before a tool
//! call goes to the wrong binary.

use std::collections::HashMap;

use async_trait::async_trait;
use corlinman_plugins::{discover, Origin, SearchRoot};

use super::{DoctorCheck, DoctorContext, DoctorResult};

pub struct ManifestDuplicatesCheck;

impl ManifestDuplicatesCheck {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ManifestDuplicatesCheck {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DoctorCheck for ManifestDuplicatesCheck {
    fn name(&self) -> &str {
        "manifest_duplicates"
    }

    async fn run(&self, ctx: &DoctorContext) -> DoctorResult {
        let plugins_dir = ctx.data_dir.join("plugins");
        if !plugins_dir.exists() {
            return DoctorResult::Ok {
                message: "no plugins dir; nothing to check".into(),
            };
        }
        let roots = vec![SearchRoot::new(&plugins_dir, Origin::Global)];
        let (entries, _diags) = discover(&roots);

        let mut counts: HashMap<String, usize> = HashMap::new();
        for e in &entries {
            *counts.entry(e.manifest.name.clone()).or_insert(0) += 1;
        }
        let dups: Vec<(&String, &usize)> = counts.iter().filter(|(_, n)| **n > 1).collect();
        if dups.is_empty() {
            DoctorResult::Ok {
                message: format!("{} plugin(s); no duplicates", entries.len()),
            }
        } else {
            let list: Vec<String> = dups
                .iter()
                .map(|(name, n)| format!("{name} (×{n})"))
                .collect();
            DoctorResult::Warn {
                message: format!("duplicate plugin name(s): {}", list.join(", ")),
                hint: Some("remove or rename the extra plugin dir".into()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn ctx_for(data_dir: std::path::PathBuf) -> DoctorContext {
        DoctorContext {
            config_path: data_dir.join("config.toml"),
            data_dir,
            config: None,
        }
    }

    fn write_manifest(dir: &std::path::Path, name: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("plugin-manifest.toml"),
            format!(
                "name = \"{name}\"\nversion = \"0.1.0\"\nplugin_type = \"sync\"\n[entry_point]\ncommand = \"true\"\n",
            ),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn no_plugins_dir_is_ok() {
        let dir = tempdir().unwrap();
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = ManifestDuplicatesCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok");
    }

    #[tokio::test]
    async fn unique_plugins_are_ok() {
        let dir = tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        write_manifest(&plugins.join("alpha"), "alpha");
        write_manifest(&plugins.join("beta"), "beta");
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = ManifestDuplicatesCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "ok", "got: {:?}", res);
    }

    #[tokio::test]
    async fn duplicate_plugin_name_is_warn() {
        let dir = tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        write_manifest(&plugins.join("alpha-v1"), "alpha");
        write_manifest(&plugins.join("alpha-v2"), "alpha");
        let ctx = ctx_for(dir.path().to_path_buf());
        let res = ManifestDuplicatesCheck::new().run(&ctx).await;
        assert_eq!(res.status_str(), "warn", "got: {:?}", res);
    }
}
