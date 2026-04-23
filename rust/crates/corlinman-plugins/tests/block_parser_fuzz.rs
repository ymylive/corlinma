//! Corpus-driven integration test for the structured-block parser.
//!
//! For the "happy" corpus files we assert at least one successful
//! parse per file; for `mixed_valid_and_invalid.txt` we additionally
//! require at least one parse error — to make sure the parser doesn't
//! silently accept malformed envelopes.

use std::fs;
use std::path::PathBuf;

use corlinman_plugins::protocol::block::parse_all;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/block_corpus")
}

#[test]
fn happy_corpus_files_have_at_least_one_success() {
    for name in [
        "happy.txt",
        "multi.txt",
        "trailing_commas.txt",
        "multiline_value.txt",
    ] {
        let path = corpus_dir().join(name);
        let source =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let results = parse_all(&source);
        assert!(
            !results.is_empty(),
            "{name}: expected at least one envelope, got zero"
        );
        assert!(
            results.iter().any(Result::is_ok),
            "{name}: no successful parse in {results:?}"
        );
    }
}

#[test]
fn mixed_corpus_file_has_success_and_error() {
    let path = corpus_dir().join("mixed_valid_and_invalid.txt");
    let source = fs::read_to_string(&path).unwrap();
    let results = parse_all(&source);
    assert!(
        results.iter().any(Result::is_ok),
        "mixed: expected at least one Ok, got {results:?}"
    );
    assert!(
        results.iter().any(Result::is_err),
        "mixed: expected at least one Err, got {results:?}"
    );
}
