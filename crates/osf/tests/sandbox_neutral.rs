//! The sandbox seam stays provider-neutral: only allow-listed provider-side
//! files under the crate may name a provider or its tooling.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const SRC_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
const SANDBOX_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/sandbox");
const PROVIDER_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/docker_sandbox.rs");

/// Every whole word the neutral source must not hold.
const BANNED: [&str; 9] = [
    "docker",
    "podman",
    "containerd",
    "nerdctl",
    "buildah",
    "runc",
    "colima",
    "container",
    "containers",
];

/// The provider-side files, each with why it may hold a banned word.
const ALLOWED: [(&str, &str); 9] = [
    ("docker_sandbox.rs", "the sandbox provider adapter itself"),
    (
        "sandbox_cli.rs",
        "the command line that drives the provider",
    ),
    ("main.rs", "the `osf sandbox run` wiring and help text"),
    ("lib.rs", "declares the provider module"),
    (
        "lints/script_pins.rs",
        "a script-pin lint naming install images, unrelated to the seam",
    ),
    (
        "lints/skill.rs",
        "a rule doc string naming container commands, unrelated to the seam",
    ),
    (
        "lints/writing/names.rs",
        "an allowed-name list holding a product name, unrelated to the seam",
    ),
    (
        "agents.rs",
        "the agent list names each harness's container settings; it is not the sandbox seam",
    ),
    (
        "reviewers.rs",
        "the reviewer roster reads the factory-container flag; it is not the sandbox seam",
    ),
];

/// Splits `text` into lowercase words on any non-alphanumeric character.
fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

/// Every banned whole word in `text`, in the order `words` yields it.
fn banned_words(text: &str) -> Vec<String> {
    words(text)
        .into_iter()
        .filter(|word| BANNED.contains(&word.as_str()))
        .collect()
}

/// Every `.rs` path under `dir`, recursively.
fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current).expect("reads a source folder") {
            let path = entry.expect("reads one entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    files
}

/// `path` relative to `SRC_DIR`, with `/` separators.
fn relative(path: &Path) -> String {
    path.strip_prefix(Path::new(SRC_DIR))
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[test]
fn every_sandbox_file_is_provider_neutral() {
    let entries = fs::read_dir(PathBuf::from(SANDBOX_DIR)).expect("reads the sandbox folder");
    let mut names = BTreeSet::new();
    for entry in entries {
        let path = entry.expect("reads one entry").path();
        let name = path
            .file_name()
            .expect("a sandbox file has a name")
            .to_string_lossy()
            .into_owned();
        names.insert(name);
    }
    let expected: BTreeSet<String> = ["fake.rs", "mod.rs", "validate.rs"]
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    assert_eq!(
        names, expected,
        "the sandbox folder holds exactly its three files"
    );

    let allowed: BTreeSet<String> = ALLOWED
        .iter()
        .map(|(path, _)| (*path).to_string())
        .collect();
    for path in rust_files(Path::new(SRC_DIR)) {
        let name = relative(&path);
        if allowed.contains(&name) {
            continue;
        }
        let text = fs::read_to_string(&path).expect("reads one source file");
        let found = banned_words(&text);
        assert!(found.is_empty(), "{name} names {found:?}");
    }
}

#[test]
fn the_allow_list_holds_only_files_that_exist() {
    for (path, reason) in ALLOWED {
        assert!(!reason.is_empty(), "{path} states a reason");
        assert!(
            Path::new(SRC_DIR).join(path).is_file(),
            "the allow-listed file exists: {path}"
        );
    }
}

#[test]
fn the_scanner_finds_the_provider_word_in_a_provider_file() {
    let text = fs::read_to_string(PathBuf::from(PROVIDER_FILE)).expect("reads the provider file");
    assert!(
        banned_words(&text).iter().any(|word| word == "docker"),
        "the scanner must be able to fail"
    );
}

#[test]
fn a_made_up_text_holding_podman_is_reported() {
    assert_eq!(banned_words("a Podman host"), ["podman".to_string()]);
    assert!(
        banned_words("devcontainer").is_empty(),
        "a compound word is not a banned token"
    );
}
