//! No test starts `git` itself, or names a variable that tells git which
//! repository to use. A test that builds a scratch repository goes through
//! `common::scratch_git`, which runs git from an empty environment against a
//! directory under the temp root.
//!
//! What this covers: every `.rs` file under `crates/*/src` and
//! `crates/*/tests`, parsed with the Rust grammar. Test code is a file under
//! `tests/`, a file with `#![cfg(test)]`, or an item marked `#[cfg(test)]` or
//! `#[test]`. In it, the check refuses `Command::new("git")` (so also any
//! helper that wraps it) and a string literal naming `GIT_DIR`,
//! `GIT_WORK_TREE`, `GIT_INDEX_FILE` or `GIT_COMMON_DIR`.
//!
//! What it does not cover: git started through a computed program name,
//! through a shell string, or by another program a test starts; production
//! code, which is meant to run git; and the two files that must name these
//! things, the helper and this file.

use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser};

/// The files that may start git or name the variables: the helper, and this rule's own source.
const EXEMPT: &[&str] = &[
    "crates/osf/tests/common/scratch_git.rs",
    "crates/osf/tests/test_git_discipline.rs",
];

/// String literals a test must not contain.
const FORBIDDEN_VARIABLES: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the workspace root is two folders above this crate")
        .to_path_buf()
}

/// The text of a string literal node without its quotes or raw-string markers.
fn literal_content(text: &str) -> &str {
    text.trim_start_matches('r')
        .trim_matches('#')
        .trim_matches('"')
}

/// Whether `node` is an attribute that makes the next item test-only.
fn is_test_attribute(node: Node, source: &str) -> bool {
    if node.kind() != "attribute_item" {
        return false;
    }
    let text: String = node_text(node, source)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    text == "#[cfg(test)]" || text == "#[test]"
}

fn node_text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

/// Every offence under `node` as `line: what`, counting only test code.
fn walk(node: Node, source: &str, in_test: bool, found: &mut Vec<String>) {
    if in_test {
        check(node, source, found);
    }
    let mut cursor = node.walk();
    let mut pending_test_attribute = false;
    for child in node.children(&mut cursor) {
        if is_test_attribute(child, source) {
            pending_test_attribute = true;
            continue;
        }
        if child.kind() == "attribute_item" || child.kind().ends_with("comment") {
            continue;
        }
        walk(child, source, in_test || pending_test_attribute, found);
        pending_test_attribute = false;
    }
}

fn check(node: Node, source: &str, found: &mut Vec<String>) {
    let line = node.start_position().row + 1;
    match node.kind() {
        "call_expression" => {
            let calls_command_new = node
                .child_by_field_name("function")
                .is_some_and(|f| node_text(f, source).ends_with("Command::new"));
            let first_argument = node
                .child_by_field_name("arguments")
                .and_then(|a| a.named_child(0));
            if calls_command_new
                && first_argument.is_some_and(|a| {
                    matches!(a.kind(), "string_literal" | "raw_string_literal")
                        && literal_content(node_text(a, source)) == "git"
                })
            {
                found.push(format!("{line}: starts git with Command::new(\"git\")"));
            }
        }
        "string_literal" | "raw_string_literal" => {
            let content = literal_content(node_text(node, source));
            if FORBIDDEN_VARIABLES.contains(&content) {
                found.push(format!("{line}: names {content}"));
            }
        }
        _ => {}
    }
}

/// The offences in one source file. `whole_file_is_test` marks a file under `tests/`.
fn offences_in(source: &str, whole_file_is_test: bool) -> Vec<String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("the Rust grammar loads");
    let tree = parser.parse(source, None).expect("the source parses");
    assert!(
        !tree.root_node().has_error(),
        "a source file did not parse, so it cannot be checked"
    );
    let root = tree.root_node();
    let inner_cfg_test = {
        let mut cursor = root.walk();
        let found = root.children(&mut cursor).any(|child| {
            child.kind() == "inner_attribute_item"
                && node_text(child, source)
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
                    == "#![cfg(test)]"
        });
        found
    };
    let mut found = Vec::new();
    walk(
        root,
        source,
        whole_file_is_test || inner_cfg_test,
        &mut found,
    );
    found
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("a directory entry reads").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "fixtures") {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_test_starts_git_or_names_a_repository_locating_variable() {
    let root = workspace_root();
    let mut offences = Vec::new();
    let mut checked = 0;
    for crate_dir in std::fs::read_dir(root.join("crates")).expect("the crates folder reads") {
        let crate_dir = crate_dir.expect("a crate folder reads").path();
        for (sub, whole_file_is_test) in [("src", false), ("tests", true)] {
            let mut files = Vec::new();
            rust_files(&crate_dir.join(sub), &mut files);
            for file in files {
                let relative = file
                    .strip_prefix(&root)
                    .expect("under the root")
                    .to_string_lossy()
                    .replace('\\', "/");
                if EXEMPT.contains(&relative.as_str()) {
                    continue;
                }
                let source = std::fs::read_to_string(&file).expect("a source file reads");
                checked += 1;
                for offence in offences_in(&source, whole_file_is_test) {
                    offences.push(format!("{relative}:{offence}"));
                }
            }
        }
    }
    assert!(checked > 50, "only {checked} files were checked");
    assert!(
        offences.is_empty(),
        "test code must use common::scratch_git and never name these:\n{}",
        offences.join("\n")
    );
}

#[test]
fn the_check_finds_a_direct_git_call_in_test_code_and_a_wrapper_around_it() {
    let source = "#[cfg(test)]\nmod tests {\n    fn run(args: &[&str]) {\n        std::process::Command::new(\"git\").args(args);\n    }\n}\n";
    assert_eq!(offences_in(source, false).len(), 1);
}

#[test]
fn the_check_finds_each_forbidden_variable_name_in_test_code() {
    for name in FORBIDDEN_VARIABLES {
        let source = format!("#[test]\nfn t() {{ let _ = \"{name}\"; }}\n");
        assert_eq!(offences_in(&source, false).len(), 1, "{name}");
    }
}

#[test]
fn the_check_finds_git_in_a_file_marked_as_wholly_test_code() {
    let source = "#![cfg(test)]\nfn run() { Command::new(\"git\"); }\n";
    assert_eq!(offences_in(source, false).len(), 1);
}

#[test]
fn the_check_leaves_production_code_and_other_programs_alone() {
    let source = "fn run() { Command::new(\"git\"); let _ = \"GIT_DIR\"; }\n#[cfg(test)]\nmod tests { fn f() { Command::new(\"moon\"); let _ = \"git\"; } }\n";
    assert_eq!(offences_in(source, false), Vec::<String>::new());
}
