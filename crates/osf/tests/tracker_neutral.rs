//! The tracker seam stays provider-neutral: its own source names no provider or tool.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const TRACKER_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/tracker");
const PROVIDER_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/github_tracker.rs");
const SRC_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src");

const BANNED: &[&str] = &["github", "gh", "gitlab", "bitbucket", "gitea"];

#[test]
fn banned_words_in_reports_every_provider_spelling() {
    let cases = [
        ("GitHubTracker", "github"),
        ("GHRunner", "gh"),
        ("github_tracker", "github"),
        ("GitHub", "github"),
        ("use GITHUB;", "github"),
        ("ghRunner", "gh"),
        ("GitLabClient", "gitlab"),
    ];
    for (text, expected) in cases {
        let found = banned_words_in(text, BANNED);
        assert!(
            found.iter().any(|word| word.as_str() == expected),
            "{text:?} must report {expected:?}, found {found:?}"
        );
    }
}

#[test]
fn banned_words_in_ignores_whole_segments_and_lookalikes() {
    let cases = [
        "ghost",
        "weigh",
        "highlight",
        "Ghana",
        "light",
        "Tracker",
        "through",
    ];
    for text in cases {
        let found = banned_words_in(text, BANNED);
        assert!(
            found.is_empty(),
            "{text:?} must report no banned word, found {found:?}"
        );
    }
}

#[test]
fn collect_files_walks_a_folder_recursively() {
    let files = collect_files(Path::new(SRC_DIR));
    let expected = Path::new("tracker").join("mod.rs");
    assert!(
        files.iter().any(|path| path.ends_with(&expected)),
        "the recursive walk must reach tracker/mod.rs"
    );
}

#[test]
fn every_tracker_file_is_provider_neutral() {
    let folder = Path::new(TRACKER_DIR);
    let files = collect_files(folder);
    let names: BTreeSet<String> = files
        .iter()
        .map(|path| {
            path.strip_prefix(folder)
                .expect("the walk stays inside the tracker folder")
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    let expected: BTreeSet<String> = ["fake.rs", "mod.rs", "ready.rs", "report.rs"]
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    assert_eq!(
        names, expected,
        "the tracker folder holds exactly its four files"
    );

    let mut hits = Vec::new();
    for path in &files {
        let text = fs::read_to_string(path).expect("reads one tracker file");
        for word in banned_words_in(&text, BANNED) {
            hits.push(format!("{} names {word}", path.display()));
        }
    }
    assert!(
        hits.is_empty(),
        "tracker source is provider-neutral, found {hits:?}"
    );
}

#[test]
fn the_scanner_finds_the_provider_word_in_a_provider_file() {
    let text = fs::read_to_string(PathBuf::from(PROVIDER_FILE)).expect("reads the provider file");
    assert!(
        banned_words_in(&text, BANNED)
            .iter()
            .any(|word| word == "github"),
        "the scanner must be able to fail"
    );
}

/// Returns each banned word that appears as a whole segment or as the join of consecutive segments.
fn banned_words_in(text: &str, banned: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    for word in banned {
        let present = text
            .split(|character: char| !character.is_alphanumeric())
            .any(|token| !token.is_empty() && token_names(token, word));
        if present {
            found.push((*word).to_string());
        }
    }
    found
}

/// Checks one alphanumeric token: a banned word matches a segment or a run of consecutive segments.
fn token_names(token: &str, banned: &str) -> bool {
    let segments = camel_segments(token);
    for (start, _) in segments.iter().enumerate() {
        let mut candidate = String::new();
        for segment in segments.iter().skip(start) {
            candidate.push_str(segment);
            if candidate.eq_ignore_ascii_case(banned) {
                return true;
            }
        }
    }
    false
}

/// Splits one token at camel-case boundaries, so `GHRunner` becomes `GH`, `Runner`.
fn camel_segments(token: &str) -> Vec<String> {
    let characters: Vec<char> = token.chars().collect();
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut previous: Option<char> = None;
    for (index, &character) in characters.iter().enumerate() {
        if let Some(previous) = previous {
            let lower_or_digit = previous.is_lowercase() || previous.is_ascii_digit();
            let acronym = previous.is_uppercase()
                && character.is_uppercase()
                && characters
                    .get(index + 1)
                    .is_some_and(|next| next.is_lowercase());
            if (lower_or_digit && character.is_uppercase()) || acronym {
                segments.push(std::mem::take(&mut current));
            }
        }
        current.push(character);
        previous = Some(character);
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

/// Collects every file under `dir` at any depth, sorted for a stable order.
fn collect_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let entries = fs::read_dir(&current).expect("reads a directory");
        for entry in entries {
            let path = entry.expect("reads one entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}
