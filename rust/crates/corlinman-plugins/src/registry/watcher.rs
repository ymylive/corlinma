//! Filesystem hot-reload for the plugin registry.
//!
//! Watches the registry's search roots with the `notify` crate. When a
//! `plugin-manifest.toml` is created, rewritten, or deleted, the affected
//! plugin is re-discovered and either `upsert`ed or `remove`d on the live
//! [`PluginRegistry`] without restarting the gateway.
//!
//! Failure model
//! -------------
//! - `notify::RecommendedWatcher` failing to install on any root (e.g. a
//!   flaky FSEvents stream on macOS) degrades to a **polling fallback** that
//!   re-runs full discovery every 60s. The registry never goes stale.
//! - Individual manifest parse errors are recorded in the registry's
//!   diagnostics; the previous entry (if any) is retained so a typo during
//!   editing does not nuke the plugin from under in-flight callers.
//! - A deleted plugin directory removes every entry whose `manifest_path`
//!   lives beneath it.
//!
//! Out of scope for this revision
//! ------------------------------
//! - `service` plugins: restarting the supervised gRPC child on manifest
//!   changes is T1 territory. We still upsert the metadata so the next
//!   spawn picks up the new version, but a running service is not killed.
//!   See the TODO at the bottom of [`apply_refresh`].

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Watcher};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;

use corlinman_core::CorlinmanError;

use crate::discovery::{discover, Origin, SearchRoot};
use crate::manifest::{parse_manifest_file, MANIFEST_FILENAME};
use crate::registry::{Diagnostic, PluginEntry, PluginRegistry};

/// Default debounce window: `notify` typically emits a burst of events per
/// save; we coalesce them into one re-discover pass.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(500);

/// Polling cadence used when `notify` cannot install a native watcher.
const POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Attaches a live [`PluginRegistry`] to a filesystem watcher so manifest
/// edits under the registry's search roots show up in subsequent `list` /
/// `get` calls without a restart.
pub struct HotReloader {
    registry: Arc<PluginRegistry>,
    roots: Vec<PathBuf>,
    debounce: Duration,
}

impl HotReloader {
    pub fn new(registry: Arc<PluginRegistry>, roots: Vec<PathBuf>, debounce: Duration) -> Self {
        Self {
            registry,
            roots,
            debounce,
        }
    }

    /// Run until `cancel` fires. Returns `Ok(())` on a clean shutdown.
    ///
    /// Entered from `tokio::spawn`. The watcher thread is joined before
    /// return so no file descriptors leak on cancellation.
    pub async fn run(self, cancel: CancellationToken) -> Result<(), CorlinmanError> {
        let search_roots = self.search_roots();
        let (tx, rx) = mpsc::unbounded_channel::<WatchEvent>();

        // Install a native watcher. On failure (macOS FSEvents flakiness,
        // inotify quota exhausted, …) fall back to periodic full re-discover.
        let _watcher = match install_watcher(&self.roots, tx.clone()) {
            Ok(w) => Some(w),
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    roots = ?self.roots,
                    "notify watcher install failed; falling back to {POLL_INTERVAL:?} polling",
                );
                spawn_polling_fallback(tx.clone(), cancel.clone());
                None
            }
        };

        run_loop(self.registry, search_roots, rx, self.debounce, cancel).await;
        Ok(())
    }

    fn search_roots(&self) -> Vec<SearchRoot> {
        self.registry.roots().to_vec()
    }
}

/// Signal emitted either by the `notify` thread or by the polling fallback.
///
/// We intentionally do not carry the raw `notify::Event` across the channel:
/// event granularity varies wildly by platform (FSEvents coalesces, inotify
/// splits) and everything reduces to "re-scan the roots" after debounce.
#[derive(Debug, Clone)]
struct WatchEvent {
    /// Paths touched by this event batch. Used only for logging; we always
    /// re-scan the full set of roots to stay correct under directory renames
    /// and atomic-write temp files.
    paths: Vec<PathBuf>,
}

fn install_watcher(
    roots: &[PathBuf],
    tx: UnboundedSender<WatchEvent>,
) -> Result<notify::RecommendedWatcher, notify::Error> {
    let mut watcher = recommended_watcher(move |res: notify::Result<Event>| match res {
        Ok(event) => {
            if matters(&event) {
                let _ = tx.send(WatchEvent {
                    paths: event.paths.clone(),
                });
            }
        }
        Err(err) => {
            tracing::warn!(error = %err, "notify event error");
        }
    })?;
    for root in roots {
        if !root.exists() {
            tracing::debug!(path = %root.display(), "watch root missing; skipping");
            continue;
        }
        if let Err(err) = watcher.watch(root, RecursiveMode::Recursive) {
            tracing::warn!(
                error = %err,
                path = %root.display(),
                "failed to start watching plugin root; falling back to polling for this root",
            );
            return Err(err);
        }
    }
    Ok(watcher)
}

/// Skip access/metadata noise; we only care about create / modify / remove.
fn matters(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

fn spawn_polling_fallback(tx: UnboundedSender<WatchEvent>, cancel: CancellationToken) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(POLL_INTERVAL);
        // First tick fires immediately; skip it so we do not collide with
        // the initial registry load.
        ticker.tick().await;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = ticker.tick() => {
                    if tx.send(WatchEvent { paths: Vec::new() }).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

async fn run_loop(
    registry: Arc<PluginRegistry>,
    roots: Vec<SearchRoot>,
    mut rx: UnboundedReceiver<WatchEvent>,
    debounce: Duration,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::debug!("hot reloader cancelled");
                return;
            }
            maybe = rx.recv() => {
                let Some(first) = maybe else {
                    tracing::debug!("hot reloader channel closed");
                    return;
                };

                // Debounce: drain everything that arrives within the window,
                // then re-scan exactly once.
                let mut touched: Vec<PathBuf> = first.paths;
                let deadline = tokio::time::Instant::now() + debounce;
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => return,
                        _ = tokio::time::sleep_until(deadline) => break,
                        more = rx.recv() => match more {
                            Some(ev) => touched.extend(ev.paths),
                            None => break,
                        }
                    }
                }

                tracing::debug!(touched = ?touched, "hot reloader debounce flush");
                apply_refresh(&registry, &roots).await;
            }
        }
    }
}

/// Re-run `discover` against every root and reconcile the registry.
///
/// Reconciliation strategy:
///   1. Build a `{name -> (entry, origin_rank)}` map from the fresh scan,
///      honouring origin rank exactly like `PluginRegistry::from_roots`.
///   2. For every name currently in the registry that is absent from the
///      scan, emit `remove(name)`.
///   3. For every name in the scan, `upsert(entry)`. This is idempotent —
///      identical content re-inserts the same bytes.
///   4. Replace diagnostics in one write lock so readers never see a torn
///      half-state.
async fn apply_refresh(registry: &Arc<PluginRegistry>, roots: &[SearchRoot]) {
    let registry = registry.clone();
    let roots = roots.to_vec();

    // Discovery walks the filesystem; shove it onto a blocking task so we
    // don't stall the reactor on a slow disk.
    let (plugins, parse_diags) = tokio::task::spawn_blocking(move || discover(&roots))
        .await
        .unwrap_or_else(|join_err| {
            tracing::error!(error = %join_err, "discover task panicked");
            (Vec::new(), Vec::new())
        });

    // Build the fresh winner table, honouring origin rank (higher wins).
    let mut winners: HashMap<String, (PluginEntry, u8, usize)> = HashMap::new();
    let mut collisions: Vec<Diagnostic> = Vec::new();
    for dp in plugins {
        let name = dp.manifest.name.clone();
        let rank = dp.origin.rank();
        let manifest_path = dp.manifest_path.clone();
        let origin = dp.origin;
        let fresh_entry = PluginEntry {
            manifest: Arc::new(dp.manifest),
            origin,
            manifest_path: manifest_path.clone(),
            shadowed_count: 0,
        };
        match winners.get_mut(&name) {
            Some((existing, existing_rank, shadowed)) => {
                if rank > *existing_rank {
                    let loser_path = existing.manifest_path.clone();
                    let loser_origin = existing.origin;
                    *existing = fresh_entry;
                    *existing_rank = rank;
                    *shadowed += 1;
                    collisions.push(Diagnostic::NameCollision {
                        name: name.clone(),
                        winner: manifest_path,
                        winner_origin: origin,
                        loser: loser_path,
                        loser_origin,
                    });
                } else {
                    *shadowed += 1;
                    collisions.push(Diagnostic::NameCollision {
                        name: name.clone(),
                        winner: existing.manifest_path.clone(),
                        winner_origin: existing.origin,
                        loser: manifest_path,
                        loser_origin: origin,
                    });
                }
            }
            None => {
                winners.insert(name, (fresh_entry, rank, 0));
            }
        }
    }

    // Stamp `shadowed_count` before writing.
    let fresh_names: HashSet<String> = winners.keys().cloned().collect();
    let mut fresh_entries: HashMap<String, PluginEntry> = HashMap::new();
    for (name, (mut entry, _, shadowed)) in winners {
        entry.shadowed_count = shadowed;
        fresh_entries.insert(name, entry);
    }

    // Names that vanished from disk get removed — but a manifest that
    // merely failed to parse should NOT evict the previous good entry. We
    // treat the plugin's directory as "still present" when a parse diag
    // names it, so transient edits don't nuke the registry entry.
    let parse_error_dirs: HashSet<PathBuf> = parse_diags
        .iter()
        .filter_map(|d| d.path.parent().map(Path::to_path_buf))
        .collect();

    let existing_entries: Vec<PluginEntry> = registry.list();
    let existing_names: Vec<String> = existing_entries
        .iter()
        .map(|e| e.manifest.name.clone())
        .collect();
    for entry in &existing_entries {
        let name = &entry.manifest.name;
        if fresh_names.contains(name) {
            continue;
        }
        // Parse failure on the same directory → keep the old entry.
        if let Some(parent) = entry.manifest_path.parent() {
            if parse_error_dirs.contains(parent) {
                tracing::debug!(plugin = %name, "parse failure; retaining previous entry");
                continue;
            }
        }
        if let Some(prev) = registry.remove(name) {
            tracing::info!(
                plugin = %name,
                path = %prev.manifest_path.display(),
                "plugin removed (manifest gone)"
            );
        }
    }

    for (name, entry) in fresh_entries {
        let is_new = !existing_names.contains(&name);
        let version = entry.manifest.version.clone();
        let plugin_type = entry.manifest.plugin_type;
        registry.upsert(entry);
        if is_new {
            tracing::info!(plugin = %name, %version, "plugin added");
        } else {
            tracing::debug!(plugin = %name, %version, "plugin upserted");
        }

        // TODO(T1 service plugins): when `plugin_type == Service`, coordinate
        // with the supervisor (stop_service + spawn_service) so the running
        // gRPC child is restarted with the new manifest. Currently the
        // supervisor picks up changes on next natural restart only.
        let _ = plugin_type;
    }

    // Merge parse diagnostics (worth preserving — ugly TOML shouldn't nuke
    // the previous good entry) with fresh collision diagnostics.
    let mut diagnostics: Vec<Diagnostic> = parse_diags
        .into_iter()
        .map(|d| Diagnostic::ParseError {
            path: d.path,
            origin: d.origin,
            message: d.message,
        })
        .collect();
    diagnostics.extend(collisions);
    registry.set_diagnostics(diagnostics);
}

/// Helper used by tests: synchronously re-scan the registry once. Exposed
/// only under `cfg(test)` so production code always goes through `run`.
#[cfg(test)]
pub(crate) async fn refresh_once(registry: &Arc<PluginRegistry>) {
    let roots = registry.roots().to_vec();
    apply_refresh(registry, &roots).await;
}

/// Resolve the manifest path for a single plugin directory. Surfaced for
/// the deletion-detection fast path (watchers see a directory remove event
/// and we want to remove entries whose path lives beneath that dir).
#[allow(dead_code)]
fn manifest_in_dir(dir: &Path) -> Option<PathBuf> {
    let candidate = dir.join(MANIFEST_FILENAME);
    candidate.is_file().then_some(candidate)
}

/// Try to parse a single manifest and map the result to a `PluginEntry`.
/// Exposed for targeted unit tests without bringing up the full watcher.
#[allow(dead_code)]
fn load_single(dir: &Path, origin: Origin) -> Option<Result<PluginEntry, Diagnostic>> {
    let manifest_path = manifest_in_dir(dir)?;
    Some(match parse_manifest_file(&manifest_path) {
        Ok(manifest) => Ok(PluginEntry {
            manifest: Arc::new(manifest),
            origin,
            manifest_path,
            shadowed_count: 0,
        }),
        Err(err) => Err(Diagnostic::ParseError {
            path: manifest_path,
            origin,
            message: err.to_string(),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn body(name: &str, version: &str) -> String {
        format!(
            "name = \"{name}\"\nversion = \"{version}\"\nplugin_type = \"sync\"\n[entry_point]\ncommand = \"true\"\n"
        )
    }

    fn write_plugin(root: &Path, name: &str, version: &str) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(MANIFEST_FILENAME), body(name, version)).unwrap();
    }

    #[tokio::test]
    async fn refresh_once_picks_up_new_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let registry = Arc::new(PluginRegistry::from_roots(vec![SearchRoot::new(
            tmp.path(),
            Origin::Workspace,
        )]));
        assert!(registry.is_empty());

        write_plugin(tmp.path(), "alpha", "0.1.0");
        refresh_once(&registry).await;
        assert_eq!(registry.get("alpha").unwrap().manifest.version, "0.1.0");
    }

    #[tokio::test]
    async fn refresh_once_detects_deletion() {
        let tmp = tempfile::tempdir().unwrap();
        write_plugin(tmp.path(), "alpha", "0.1.0");
        let registry = Arc::new(PluginRegistry::from_roots(vec![SearchRoot::new(
            tmp.path(),
            Origin::Workspace,
        )]));
        assert!(registry.get("alpha").is_some());

        fs::remove_file(tmp.path().join("alpha").join(MANIFEST_FILENAME)).unwrap();
        refresh_once(&registry).await;
        assert!(registry.get("alpha").is_none());
    }

    #[tokio::test]
    async fn refresh_once_bad_manifest_keeps_old_entry() {
        let tmp = tempfile::tempdir().unwrap();
        write_plugin(tmp.path(), "alpha", "0.1.0");
        let registry = Arc::new(PluginRegistry::from_roots(vec![SearchRoot::new(
            tmp.path(),
            Origin::Workspace,
        )]));

        // Overwrite with garbage.
        fs::write(
            tmp.path().join("alpha").join(MANIFEST_FILENAME),
            "not = valid = toml",
        )
        .unwrap();
        refresh_once(&registry).await;

        // The old entry should still be around, and the parse failure should
        // surface in diagnostics.
        assert_eq!(registry.get("alpha").unwrap().manifest.version, "0.1.0");
        assert!(registry
            .diagnostics()
            .iter()
            .any(|d| matches!(d, Diagnostic::ParseError { .. })));
    }
}
