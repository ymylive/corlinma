//! Integration tests for `corlinman-skills`.

use std::fs;
use std::path::PathBuf;

use corlinman_skills::{SkillLoadError, SkillRegistry};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Copy a subset of fixture files into a fresh tempdir so each test can
/// have its own isolated registry without cross-contamination.
fn make_dir(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for (name, body) in files {
        fs::write(dir.path().join(name), body).expect("write fixture");
    }
    dir
}

// ---------------------------------------------------------------------------
// 1. loads_happy_path_skill
// ---------------------------------------------------------------------------
#[test]
fn loads_happy_path_skill() {
    let dir = make_dir(&[(
        "web_search.md",
        &fs::read_to_string(fixtures_dir().join("web_search.md")).unwrap(),
    )]);

    let reg = SkillRegistry::load_from_dir(dir.path()).expect("load");
    let skill = reg.get("web_search").expect("skill present");

    assert_eq!(skill.name, "web_search");
    assert_eq!(skill.description, "Search the web via Brave Search API");
    assert_eq!(skill.emoji.as_deref(), Some("🔍"));
    assert_eq!(skill.requires.config, vec!["providers.brave.api_key"]);
    assert!(skill.requires.bins.is_empty());
    assert_eq!(
        skill.install.as_deref(),
        Some("Get an API key at https://brave.com/search/api/"),
    );
    assert_eq!(skill.allowed_tools, vec!["web.search", "web.fetch"]);
    assert!(skill.body_markdown.contains("Use the `web.search` tool"));
}

// ---------------------------------------------------------------------------
// 2. loads_dir_with_multiple_skills
// ---------------------------------------------------------------------------
#[test]
fn loads_dir_with_multiple_skills() {
    let src = fixtures_dir();
    let dir = make_dir(&[
        (
            "a.md",
            &fs::read_to_string(src.join("web_search.md")).unwrap(),
        ),
        (
            "b.md",
            &fs::read_to_string(src.join("shell_runner.md")).unwrap(),
        ),
        (
            "c.md",
            &fs::read_to_string(src.join("code_reviewer.md")).unwrap(),
        ),
    ]);

    let reg = SkillRegistry::load_from_dir(dir.path()).expect("load");
    let mut names = reg.names();
    names.sort();
    assert_eq!(names, vec!["code_reviewer", "shell_runner", "web_search"]);
    assert_eq!(reg.iter().count(), 3);
}

// ---------------------------------------------------------------------------
// 3. missing_name_field_fails
// ---------------------------------------------------------------------------
#[test]
fn missing_name_field_fails() {
    let bad = "---\ndescription: no name here\n---\nbody\n";
    let dir = make_dir(&[("bad.md", bad)]);

    let err = SkillRegistry::load_from_dir(dir.path()).expect_err("should fail");
    match err {
        SkillLoadError::MissingField { path, field } => {
            assert_eq!(field, "name");
            assert!(
                path.ends_with("bad.md"),
                "error path should point at the offending file, got {}",
                path.display()
            );
        }
        other => panic!("expected MissingField, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 4. duplicate_name_fails
// ---------------------------------------------------------------------------
#[test]
fn duplicate_name_fails() {
    let body = "---\nname: web_search\ndescription: dup\n---\nhi\n";
    let dir = make_dir(&[("first.md", body), ("second.md", body)]);

    let err = SkillRegistry::load_from_dir(dir.path()).expect_err("should fail");
    match err {
        SkillLoadError::DuplicateName {
            name,
            first,
            second,
        } => {
            assert_eq!(name, "web_search");
            let first_name = first.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let second_name = second.file_name().and_then(|s| s.to_str()).unwrap_or("");
            // Iteration order isn't stable, so just require both paths
            // show up in the error.
            let names = [first_name, second_name];
            assert!(
                names.contains(&"first.md"),
                "error missing first.md path: {names:?}"
            );
            assert!(
                names.contains(&"second.md"),
                "error missing second.md path: {names:?}"
            );
        }
        other => panic!("expected DuplicateName, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 5. check_requirements_bin_missing
// ---------------------------------------------------------------------------
#[test]
fn check_requirements_bin_missing() {
    let body = "---\nname: needs_bin\ndescription: d\nmetadata:\n  openclaw:\n    requires:\n      bins: [\"this-bin-does-not-exist-xyzzy\"]\n---\nbody\n";
    let dir = make_dir(&[("s.md", body)]);
    let reg = SkillRegistry::load_from_dir(dir.path()).unwrap();

    let err = reg
        .check_requirements("needs_bin", |_| None)
        .expect_err("missing bin");
    assert_eq!(err.len(), 1);
    assert!(
        err[0].contains("needs_bin")
            && err[0].contains("this-bin-does-not-exist-xyzzy")
            && err[0].contains("install it first"),
        "unexpected message: {}",
        err[0]
    );
}

// ---------------------------------------------------------------------------
// 6. check_requirements_config_empty
// ---------------------------------------------------------------------------
#[test]
fn check_requirements_config_empty() {
    let body = "---\nname: needs_cfg\ndescription: d\nmetadata:\n  openclaw:\n    requires:\n      config: [\"providers.brave.api_key\"]\n---\nbody\n";
    let dir = make_dir(&[("s.md", body)]);
    let reg = SkillRegistry::load_from_dir(dir.path()).unwrap();

    // Lookup returns None → unset.
    let err = reg
        .check_requirements("needs_cfg", |_| None)
        .expect_err("missing config");
    assert_eq!(err.len(), 1);
    assert!(
        err[0].contains("providers.brave.api_key") && err[0].contains("non-empty"),
        "unexpected message: {}",
        err[0]
    );

    // Whitespace-only counts as empty too.
    let err2 = reg
        .check_requirements("needs_cfg", |_| Some("   ".to_string()))
        .expect_err("whitespace config still empty");
    assert_eq!(err2.len(), 1);

    // Non-empty value satisfies the requirement.
    reg.check_requirements("needs_cfg", |_| Some("secret".to_string()))
        .expect("config now present");
}

// ---------------------------------------------------------------------------
// 7. body_markdown_captured_after_frontmatter
// ---------------------------------------------------------------------------
#[test]
fn body_markdown_captured_after_frontmatter() {
    // Note: leading newline, trailing whitespace, embedded blank lines.
    let raw = "---\nname: verbatim\ndescription: d\n---\n\n# Heading\n\nparagraph one\n\n   trailing-spaces   \n";
    let dir = make_dir(&[("v.md", raw)]);
    let reg = SkillRegistry::load_from_dir(dir.path()).unwrap();
    let skill = reg.get("verbatim").unwrap();

    // Body starts right after the closing `---\n`, so the first char
    // here should be the blank line we put after the frontmatter.
    let expected = "\n# Heading\n\nparagraph one\n\n   trailing-spaces   \n";
    assert_eq!(skill.body_markdown, expected);
}
