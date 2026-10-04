//! The tracker seam stays provider-neutral: its own source names no provider or tool.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

const TRACKER_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/tracker");
const PROVIDER_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/github_tracker.rs");

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

#[test]
fn every_tracker_file_is_provider_neutral() {
    let entries = fs::read_dir(PathBuf::from(TRACKER_DIR)).expect("reads the tracker folder");
    let mut names = BTreeSet::new();
    let mut texts = Vec::new();
    for entry in entries {
        let path = entry.expect("reads one entry").path();
        let name = path
            .file_name()
            .expect("a tracker file has a name")
            .to_string_lossy()
            .into_owned();
        names.insert(name);
        texts.push(fs::read_to_string(&path).expect("reads one tracker file"));
    }
    let expected: BTreeSet<String> = ["fake.rs", "mod.rs", "ready.rs", "report.rs"]
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    assert_eq!(
        names, expected,
        "the tracker folder holds exactly its four files"
    );
    let banned = ["github", "gh"];
    for text in &texts {
        for word in words(text) {
            assert!(
                !banned.contains(&word.as_str()),
                "tracker source names {word}"
            );
        }
    }
}

#[test]
fn the_scanner_finds_the_provider_word_in_a_provider_file() {
    let text = fs::read_to_string(PathBuf::from(PROVIDER_FILE)).expect("reads the provider file");
    assert!(
        words(&text).iter().any(|word| word == "github"),
        "the scanner must be able to fail"
    );
}
