//! End-to-end test for the plugin hot-reload pipeline.
//!
//! Spawns a real `HotReloader` against a `tempdir`, mutates the directory,
//! and asserts the live registry catches up within the debounce window.
//!
//! `notify` event latency varies wildly by platform — CI macOS FSEvents in
//! particular likes to coalesce and delay by hundreds of ms. Wait windows
//! below are deliberately generous (800–1500 ms per step) to match the
//! spec's reliability bar.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use corlinman_plugins::discovery::{Origin, SearchRoot};
use corlinman_plugins::manifest::MANIFEST_FILENAME;
use corlinman_plugins::registry::watcher::{HotReloader, DEFAULT_DEBOUNCE};
use corlinman_plugins::registry::PluginRegistry;
use tokio_util::sync::CancellationToken;

fn body(name: &str, version: &str) -> String {
    format!(
        "name = \"{name}\"\nversion = \"{version}\"\nplugin_type = \"sync\"\n[entry_point]\ncommand = \"true\"\n"
    )
}

fn write_plugin(root: &Path, name: &str, version: &str) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(MANIFEST_FILENAME), body(name, version)).unwrap();
}

/// Wait up to `deadline` for `predicate()` to return `true`, polling every
/// 50ms. Keeps tests resilient to notify's platform-specific jitter without
/// hard-coded sleeps that blow up CI.
async fn wait_until<F>(deadline: Duration, mut predicate: F) -> bool
where
    F: FnMut() -> bool,
{
    let start = tokio::time::Instant::now();
    loop {
        if predicate() {
            return true;
        }
        if start.elapsed() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Spawn a [`HotReloader`] against `root` and return a cancellation handle
/// plus the live registry.
async fn spawn_reloader(root: &Path) -> (Arc<PluginRegistry>, CancellationToken) {
    let registry = Arc::new(PluginRegistry::from_roots(vec![SearchRoot::new(
        root,
        Origin::Workspace,
    )]));
    let cancel = CancellationToken::new();
    let reloader = HotReloader::new(registry.clone(), vec![root.to_path_buf()], DEFAULT_DEBOUNCE);
    let cancel_child = cancel.clone();
    tokio::spawn(async move {
        let _ = reloader.run(cancel_child).await;
    });
    // Give notify a beat to install the watcher before we start mutating.
    tokio::time::sleep(Duration::from_millis(100)).await;
    (registry, cancel)
}

#[tokio::test]
async fn picks_up_newly_created_plugin() {
    let tmp = tempfile::tempdir().unwrap();
    let (registry, cancel) = spawn_reloader(tmp.path()).await;
    assert!(registry.get("alpha").is_none());

    write_plugin(tmp.path(), "alpha", "0.1.0");
    let ok = wait_until(Duration::from_millis(2000), || {
        registry.get("alpha").is_some()
    })
    .await;
    assert!(ok, "alpha should appear in the registry after creation");
    assert_eq!(registry.get("alpha").unwrap().manifest.version, "0.1.0");
    cancel.cancel();
}

#[tokio::test]
async fn observes_version_bump_on_manifest_edit() {
    let tmp = tempfile::tempdir().unwrap();
    write_plugin(tmp.path(), "alpha", "0.1.0");
    let (registry, cancel) = spawn_reloader(tmp.path()).await;
    assert_eq!(registry.get("alpha").unwrap().manifest.version, "0.1.0");

    // Rewrite the manifest with a bumped version.
    std::fs::write(
        tmp.path().join("alpha").join(MANIFEST_FILENAME),
        body("alpha", "0.2.0"),
    )
    .unwrap();

    let ok = wait_until(Duration::from_millis(2000), || {
        registry
            .get("alpha")
            .map(|e| e.manifest.version == "0.2.0")
            .unwrap_or(false)
    })
    .await;
    assert!(ok, "version bump should propagate to the registry");
    cancel.cancel();
}

#[tokio::test]
async fn removes_entry_when_manifest_is_deleted() {
    let tmp = tempfile::tempdir().unwrap();
    write_plugin(tmp.path(), "alpha", "0.1.0");
    let (registry, cancel) = spawn_reloader(tmp.path()).await;
    assert!(registry.get("alpha").is_some());

    std::fs::remove_file(tmp.path().join("alpha").join(MANIFEST_FILENAME)).unwrap();
    let ok = wait_until(Duration::from_millis(2000), || {
        registry.get("alpha").is_none()
    })
    .await;
    assert!(
        ok,
        "alpha should vanish from registry after manifest delete"
    );
    cancel.cancel();
}

#[tokio::test]
async fn picks_up_new_sibling_plugin() {
    let tmp = tempfile::tempdir().unwrap();
    write_plugin(tmp.path(), "alpha", "0.1.0");
    let (registry, cancel) = spawn_reloader(tmp.path()).await;
    assert!(registry.get("alpha").is_some());
    assert!(registry.get("beta").is_none());

    write_plugin(tmp.path(), "beta", "0.1.0");
    let ok = wait_until(Duration::from_millis(2000), || {
        registry.get("beta").is_some()
    })
    .await;
    assert!(ok, "beta should be discovered in the same root");
    // alpha should still be there.
    assert!(registry.get("alpha").is_some());
    let names: Vec<String> = registry
        .list()
        .into_iter()
        .map(|e| e.manifest.name.clone())
        .collect();
    assert!(names.contains(&"alpha".to_string()));
    assert!(names.contains(&"beta".to_string()));
    cancel.cancel();
}

#[tokio::test]
async fn concurrent_reads_do_not_deadlock_during_writes() {
    // Hammers the read path from 8 tasks while the reloader is flushing
    // writes. If the RwLock is misused (e.g. read guards held across .await
    // in the reader, or recursive write), this test hangs on the 5s timeout.
    let tmp = tempfile::tempdir().unwrap();
    write_plugin(tmp.path(), "alpha", "0.1.0");
    let (registry, cancel) = spawn_reloader(tmp.path()).await;

    let mut joins = Vec::new();
    for _ in 0..8 {
        let reg = registry.clone();
        joins.push(tokio::spawn(async move {
            for _ in 0..50 {
                let _ = reg.list();
                let _ = reg.get("alpha");
                let _ = reg.diagnostics();
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }));
    }

    // While the readers hammer, keep bumping the manifest so the writer
    // path fires several times.
    for v in 1..6 {
        std::fs::write(
            tmp.path().join("alpha").join(MANIFEST_FILENAME),
            body("alpha", &format!("0.{v}.0")),
        )
        .unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    // Bounded join so a stuck lock surfaces as a test failure rather than a
    // hung CI.
    let joined = tokio::time::timeout(Duration::from_secs(5), async {
        for j in joins {
            let _ = j.await;
        }
    })
    .await;
    assert!(joined.is_ok(), "reader tasks stuck — likely a lock issue");
    cancel.cancel();
}
