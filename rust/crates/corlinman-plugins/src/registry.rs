//! Plugin registry: deduped, origin-ranked view of discovered manifests.
//!
//! Sprint 2 T4: the registry is now mutable behind a `RwLock` so the
//! [`watcher::HotReloader`] can `upsert` / `remove` entries in response to
//! `plugin-manifest.toml` edits on disk.
//!
//! Reader contract:
//!   - [`PluginRegistry::list`] / [`PluginRegistry::get`] /
//!     [`PluginRegistry::diagnostics`] return **owned clones** under a brief
//!     read lock. Callers must never hold the lock across `.await` points —
//!     this API is intentionally snapshot-based to prevent that.
//!   - Writers go through `upsert` / `remove` / `set_diagnostics`, each of
//!     which takes a short write lock and releases it before returning.
//!
//! The `RwLock` choice (over `Mutex`) matches the workload: hot path is the
//! tool-call dispatch (`registry.get("echo")`) which is read-heavy; writes
//! only fire on manifest filesystem changes (typically minutes apart).

pub mod watcher;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::async_task::AsyncTaskRegistry;
use crate::discovery::{discover, DiscoveredPlugin, DiscoveryDiagnostic, Origin, SearchRoot};
use crate::manifest::PluginManifest;

/// One resolved plugin entry. The registry stores these by `name` (the winning
/// name after origin-rank dedup).
#[derive(Debug, Clone)]
pub struct PluginEntry {
    pub manifest: Arc<PluginManifest>,
    pub origin: Origin,
    pub manifest_path: PathBuf,
    /// Whether another manifest with the same name was shadowed by this one.
    pub shadowed_count: usize,
}

impl PluginEntry {
    pub fn plugin_dir(&self) -> PathBuf {
        self.manifest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    }
}

/// Diagnostic types surfaced via `Registry::diagnostics`.
#[derive(Debug, Clone)]
pub enum Diagnostic {
    /// Manifest failed to parse.
    ParseError {
        path: PathBuf,
        origin: Origin,
        message: String,
    },
    /// Two manifests claim the same plugin name. `loser` was dropped.
    NameCollision {
        name: String,
        winner: PathBuf,
        winner_origin: Origin,
        loser: PathBuf,
        loser_origin: Origin,
    },
}

/// Mutable inner state guarded by a single `RwLock`.
#[derive(Debug, Default)]
struct RegistryInner {
    /// Active entries keyed by plugin name.
    entries: HashMap<String, PluginEntry>,
}

/// Plugin registry populated from a set of search roots and optionally
/// refreshed at runtime by [`watcher::HotReloader`].
///
/// Cheap to clone: the inner state sits behind `Arc<RwLock<_>>`.
#[derive(Debug, Clone)]
pub struct PluginRegistry {
    inner: Arc<RwLock<RegistryInner>>,
    diagnostics: Arc<RwLock<Vec<Diagnostic>>>,
    roots: Arc<Vec<SearchRoot>>,
    /// Shared parking lot for async plugin task ids. Cheap to clone.
    async_tasks: Arc<AsyncTaskRegistry>,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(RegistryInner::default())),
            diagnostics: Arc::new(RwLock::new(Vec::new())),
            roots: Arc::new(Vec::new()),
            async_tasks: Arc::new(AsyncTaskRegistry::new()),
        }
    }
}

impl PluginRegistry {
    /// Construct from a set of search roots, running discovery eagerly.
    pub fn from_roots(roots: Vec<SearchRoot>) -> Self {
        let (plugins, parse_diags) = discover(&roots);
        let (entries, dedup_diags) = resolve(plugins);
        let mut diagnostics: Vec<_> = parse_diags
            .into_iter()
            .map(
                |DiscoveryDiagnostic {
                     path,
                     origin,
                     message,
                 }| Diagnostic::ParseError {
                    path,
                    origin,
                    message,
                },
            )
            .collect();
        diagnostics.extend(dedup_diags);
        Self {
            inner: Arc::new(RwLock::new(RegistryInner { entries })),
            diagnostics: Arc::new(RwLock::new(diagnostics)),
            roots: Arc::new(roots),
            async_tasks: Arc::new(AsyncTaskRegistry::new()),
        }
    }

    /// Shared async-task parking lot. The gateway's tool executor parks
    /// `AcceptedForLater` task ids here; the `/plugin-callback/:task_id`
    /// HTTP handler resolves them.
    pub fn async_tasks(&self) -> Arc<AsyncTaskRegistry> {
        self.async_tasks.clone()
    }

    /// All registered plugins sorted alphabetically by name (stable output
    /// for CLI + snapshot tests).
    ///
    /// Returns a **clone**: the read lock is released before this function
    /// returns so callers can safely hold the result across `.await` points.
    pub fn list(&self) -> Vec<PluginEntry> {
        let guard = self.inner.read().expect("registry inner lock poisoned");
        let mut v: Vec<PluginEntry> = guard.entries.values().cloned().collect();
        drop(guard);
        v.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
        v
    }

    /// Look up a plugin by name. Returns an owned clone.
    pub fn get(&self, name: &str) -> Option<PluginEntry> {
        let guard = self.inner.read().expect("registry inner lock poisoned");
        guard.entries.get(name).cloned()
    }

    /// Snapshot of diagnostics emitted during the most recent (re-)discover.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diagnostics
            .read()
            .expect("diagnostics lock poisoned")
            .clone()
    }

    /// Search roots configured at construction time. Stable for the life of
    /// the registry — the hot reloader watches exactly these paths.
    pub fn roots(&self) -> &[SearchRoot] {
        self.roots.as_slice()
    }

    pub fn len(&self) -> usize {
        self.inner
            .read()
            .expect("registry inner lock poisoned")
            .entries
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner
            .read()
            .expect("registry inner lock poisoned")
            .entries
            .is_empty()
    }

    /// Insert or replace `entry` keyed by `entry.manifest.name`.
    ///
    /// Intended for the hot reloader; the `pub(crate)` visibility keeps
    /// application code on the read-only surface.
    pub(crate) fn upsert(&self, entry: PluginEntry) {
        let mut guard = self.inner.write().expect("registry inner lock poisoned");
        guard.entries.insert(entry.manifest.name.clone(), entry);
    }

    /// Remove a plugin by name. Returns the previous entry if one existed.
    pub(crate) fn remove(&self, name: &str) -> Option<PluginEntry> {
        let mut guard = self.inner.write().expect("registry inner lock poisoned");
        guard.entries.remove(name)
    }

    /// Overwrite the diagnostics vector in one write lock.
    pub(crate) fn set_diagnostics(&self, diags: Vec<Diagnostic>) {
        *self.diagnostics.write().expect("diagnostics lock poisoned") = diags;
    }
}

/// Apply origin-rank dedup. On equal rank, the manifest discovered first
/// wins — "last write wins within the same origin" by virtue of our walk
/// order being stable.
fn resolve(mut plugins: Vec<DiscoveredPlugin>) -> (HashMap<String, PluginEntry>, Vec<Diagnostic>) {
    // Sort by origin rank *descending* so higher-rank manifests are inserted
    // first; duplicates coming after them are losers.
    plugins.sort_by_key(|p| std::cmp::Reverse(p.origin.rank()));

    let mut out: HashMap<String, PluginEntry> = HashMap::new();
    let mut diags = Vec::new();

    for p in plugins {
        let name = p.manifest.name.clone();
        match out.get_mut(&name) {
            Some(existing) => {
                existing.shadowed_count += 1;
                diags.push(Diagnostic::NameCollision {
                    name: name.clone(),
                    winner: existing.manifest_path.clone(),
                    winner_origin: existing.origin,
                    loser: p.manifest_path.clone(),
                    loser_origin: p.origin,
                });
            }
            None => {
                out.insert(
                    name,
                    PluginEntry {
                        manifest: Arc::new(p.manifest),
                        origin: p.origin,
                        manifest_path: p.manifest_path,
                        shadowed_count: 0,
                    },
                );
            }
        }
    }

    (out, diags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch_manifest(dir: &std::path::Path, plugin: &str, body: &str) {
        let p = dir.join(plugin);
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join(crate::manifest::MANIFEST_FILENAME), body).unwrap();
    }

    fn body(name: &str, version: &str) -> String {
        format!(
            "name = \"{name}\"\nversion = \"{version}\"\nplugin_type = \"sync\"\n[entry_point]\ncommand = \"true\"\n"
        )
    }

    #[test]
    fn higher_origin_wins_lower_becomes_collision_diag() {
        let low = tempfile::tempdir().unwrap();
        let high = tempfile::tempdir().unwrap();

        scratch_manifest(low.path(), "shared", &body("shared", "0.0.1"));
        scratch_manifest(high.path(), "shared", &body("shared", "9.9.9"));

        let roots = vec![
            SearchRoot::new(low.path(), Origin::Bundled),
            SearchRoot::new(high.path(), Origin::Config),
        ];
        let reg = PluginRegistry::from_roots(roots);

        let entry = reg.get("shared").unwrap();
        assert_eq!(entry.manifest.version, "9.9.9");
        assert_eq!(entry.origin, Origin::Config);
        let diags = reg.diagnostics();
        assert_eq!(diags.len(), 1);
        match &diags[0] {
            Diagnostic::NameCollision {
                name, loser_origin, ..
            } => {
                assert_eq!(name, "shared");
                assert_eq!(*loser_origin, Origin::Bundled);
            }
            _ => panic!("expected collision"),
        }
    }

    #[test]
    fn upsert_then_remove_round_trips() {
        let reg = PluginRegistry::default();
        assert!(reg.is_empty());

        let manifest: PluginManifest = toml::from_str(&body("alpha", "0.1.0")).unwrap();
        let entry = PluginEntry {
            manifest: Arc::new(manifest),
            origin: Origin::Workspace,
            manifest_path: PathBuf::from("/tmp/alpha/plugin-manifest.toml"),
            shadowed_count: 0,
        };
        reg.upsert(entry.clone());
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.get("alpha").unwrap().manifest.version, "0.1.0");

        let prev = reg.remove("alpha").unwrap();
        assert_eq!(prev.manifest.name, "alpha");
        assert!(reg.get("alpha").is_none());
        assert!(reg.is_empty());
    }

    #[test]
    fn set_diagnostics_replaces_snapshot() {
        let reg = PluginRegistry::default();
        assert!(reg.diagnostics().is_empty());
        reg.set_diagnostics(vec![Diagnostic::ParseError {
            path: PathBuf::from("/tmp/bad/plugin-manifest.toml"),
            origin: Origin::Config,
            message: "bad".into(),
        }]);
        assert_eq!(reg.diagnostics().len(), 1);
    }
}
