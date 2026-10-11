//! The provider-neutral `harness` folder must name no harness and no model
//! vendor, and the scanner that checks it must be able to fail.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The provider-neutral harness source folder.
fn harness_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("harness")
}

/// Every run of alphanumeric characters in `text`, lowercased.
fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

/// The first whitespace-separated word of every supported agent's name,
/// lowercased.
fn agent_words() -> Vec<String> {
    osf::agents::AGENTS
        .iter()
        .filter_map(|agent| agent.name.split_whitespace().next())
        .map(str::to_lowercase)
        .collect()
}

/// `agent_words` plus the platform words the folder must not carry.
fn banned_words(agent_words: &[String]) -> Vec<String> {
    let mut banned = agent_words.to_vec();
    for word in ["docker", "podman", "container", "containers"] {
        banned.push(word.to_string());
    }
    banned
}

/// The model vendor and model family names the folder must not carry.
const VENDOR_WORDS: [&str; 12] = [
    "anthropic",
    "openai",
    "deepseek",
    "google",
    "gemini",
    "claude",
    "gpt",
    "mistral",
    "llama",
    "cohere",
    "grok",
    "qwen",
];

/// `VENDOR_WORDS` as owned, lowercase words.
fn vendor_words() -> Vec<String> {
    VENDOR_WORDS
        .iter()
        .map(|word| word.to_lowercase())
        .collect()
}

/// The banned words in `text`, as whole words, sorted and deduplicated.
fn banned_in_text(text: &str, banned: &[String]) -> Vec<String> {
    let banned: BTreeSet<&str> = banned.iter().map(String::as_str).collect();
    let mut found: BTreeSet<String> = BTreeSet::new();
    for word in words(text) {
        if banned.contains(word.as_str()) {
            found.insert(word);
        }
    }
    found.into_iter().collect()
}

/// The banned words in the file at `path`.
fn banned_in_file(path: &Path, banned: &[String]) -> Vec<String> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    banned_in_text(&text, banned)
}

/// The file names directly inside the harness folder.
fn harness_file_names() -> BTreeSet<String> {
    std::fs::read_dir(harness_dir())
        .expect("the harness folder reads")
        .map(|entry| {
            entry
                .expect("a directory entry reads")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

#[test]
fn the_harness_folder_holds_exactly_mod_and_fake() {
    let expected: BTreeSet<String> = ["fake.rs", "mod.rs"]
        .into_iter()
        .map(str::to_string)
        .collect();
    assert_eq!(
        harness_file_names(),
        expected,
        "the harness folder holds exactly mod.rs and fake.rs"
    );
}

#[test]
fn no_supported_agent_name_appears_in_the_harness_folder() {
    let agent_words = agent_words();
    assert!(
        !agent_words.is_empty(),
        "the supported-agent list must name at least one agent"
    );
    let banned = banned_words(&agent_words);
    let mut violations = Vec::new();
    for name in harness_file_names() {
        let found = banned_in_file(&harness_dir().join(&name), &banned);
        if !found.is_empty() {
            violations.push(format!("{name}: {found:?}"));
        }
    }
    assert!(
        violations.is_empty(),
        "the provider-neutral harness folder names a harness: {}",
        violations.join("; ")
    );
}

#[test]
fn the_scanner_flags_a_source_that_names_an_agent() {
    let agent_words = agent_words();
    let banned = banned_words(&agent_words);
    let first = osf::agents::AGENTS
        .first()
        .expect("at least one supported agent");
    let name = first.name;
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(format!("{name}_harness.rs"));
    let found = banned_in_file(&path, &banned);
    assert!(
        !found.is_empty(),
        "the scanner found no harness name in {}",
        path.display()
    );
}

#[test]
fn no_model_vendor_name_appears_in_the_harness_folder() {
    let banned = vendor_words();
    let mut violations = Vec::new();
    for name in harness_file_names() {
        let found = banned_in_file(&harness_dir().join(&name), &banned);
        if !found.is_empty() {
            violations.push(format!("{name}: {found:?}"));
        }
    }
    assert!(
        violations.is_empty(),
        "the provider-neutral harness folder names a model vendor: {}",
        violations.join("; ")
    );
}

#[test]
fn the_scanner_flags_a_source_that_names_a_model_vendor() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("dsh_harness.rs");
    let found = banned_in_file(&path, &vendor_words());
    assert!(
        found.iter().any(|word| word == "deepseek"),
        "the scanner found no model vendor in {}: {found:?}",
        path.display()
    );
}

#[test]
fn banned_in_text_matches_whole_vendor_words_only() {
    let banned = vendor_words();
    let found = banned_in_text("Ask Claude.", &banned);
    assert!(
        found.iter().any(|word| word == "claude"),
        "the scanner must find claude in a whole word"
    );
    assert!(
        banned_in_text("claudette", &banned).is_empty(),
        "the scanner must not match claude inside claudette"
    );
    let found = banned_in_text("a GPT model", &banned);
    assert!(
        found.iter().any(|word| word == "gpt"),
        "the scanner must find gpt in a whole word"
    );
    assert!(
        banned_in_text("gptq", &banned).is_empty(),
        "the scanner must not match gpt inside gptq"
    );
}
