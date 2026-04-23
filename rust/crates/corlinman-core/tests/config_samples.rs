//! Integration test for B2-BE5: the shipped sample config + sample data
//! tree (`skills/`, `agents/`, `TVStxt/`) must decode and pass
//! `Config::validate()` end-to-end.
//!
//! This is a stricter cousin of `config_example.rs`: where that test
//! tolerates the `no_provider_enabled` warning, this one asserts
//! `validate()` (the hard facade) returns `Ok(())`, which is what a
//! downstream operator sees when running `corlinman config validate`.

use std::path::PathBuf;

use corlinman_core::config::Config;

fn repo_root() -> PathBuf {
    // tests/ sits at repo/rust/crates/corlinman-core/tests/.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[test]
fn sample_config_validates_cleanly() {
    let path = repo_root().join("docs").join("config.example.toml");
    let cfg = Config::load_from_path(&path)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));

    match cfg.validate() {
        Ok(()) => {}
        Err(errs) => panic!(
            "sample config failed validate(); {} error(s):\n  - {}",
            errs.len(),
            errs.join("\n  - ")
        ),
    }
}

#[test]
fn sample_data_dirs_exist() {
    let root = repo_root();

    // Paths here mirror the keys under [skills], [variables], [agents]
    // in `docs/config.example.toml`. If a maintainer renames one, this
    // test is the fastest place to notice the drift.
    for rel in [
        "skills/web_search.md",
        "skills/code_review.md",
        "skills/memory.md",
        "agents/mentor.yaml",
        "agents/researcher.yaml",
        "agents/editor.yaml",
        "TVStxt/tar/CurrentProject.txt",
        "TVStxt/sar/SarPrompt1.txt",
        "TVStxt/fixed/README.md",
    ] {
        let p = root.join(rel);
        assert!(p.exists(), "expected sample file missing: {}", p.display());
    }
}
