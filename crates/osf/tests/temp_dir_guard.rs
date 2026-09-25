//! Ruling F8: a temporary test directory must be safe for two processes to
//! use at once, and a production one must never be wiped before use. Every
//! direct call to the OS temp directory must say why on its own line, with
//! a `// osf: temp-dir allowed, <reason>` marker, so an unrelated edit
//! elsewhere in the file can never make an old line-number allowance stale
//! without anyone noticing.

use std::path::{Path, PathBuf};

/// This file's own name: it names the call syntax and the marker text in
/// strings to search for them, which is not itself a call or a marker.
const SELF_FILE: &str = "temp_dir_guard.rs";

const PATTERN: &str = "temp_dir()";

/// The text right before a marker's own reason. The reason must be
/// non-empty: a marker is a place to say why, not a way to silence this
/// check for free.
const MARKER_PREFIX: &str = "osf: temp-dir allowed,";

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry reads").path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The marker's own reason text on `line`, trimmed. `None` when `line`
/// carries no marker at all; `Some("")` when it carries one with nothing
/// after the comma.
fn marker_reason(line: &str) -> Option<&str> {
    let idx = line.find(MARKER_PREFIX)?;
    Some(line[idx + MARKER_PREFIX.len()..].trim())
}

#[test]
fn a_marker_is_only_found_with_a_reason_after_its_comma() {
    assert_eq!(
        marker_reason("let x = 1; // osf: temp-dir allowed, shared per-run file"),
        Some("shared per-run file")
    );
    assert_eq!(
        marker_reason("let x = 1; // osf: temp-dir allowed,"),
        Some("")
    );
    assert_eq!(marker_reason("let x = 1;"), None);
}

#[test]
fn no_file_calls_the_os_temp_dir_without_a_marker_naming_why() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_rs_files(&root.join("src"), &mut files);
    collect_rs_files(&root.join("tests"), &mut files);

    let mut violations = Vec::new();
    for path in files {
        if path.file_name().and_then(|n| n.to_str()) == Some(SELF_FILE) {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .expect("file is under the crate root")
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        for (i, line) in text.lines().enumerate() {
            if !line.contains(PATTERN) {
                continue;
            }
            let line_no = i + 1;
            match marker_reason(line) {
                Some(reason) if !reason.is_empty() => {}
                Some(_) => violations.push(format!(
                    "{rel}:{line_no}: marker with no reason: {}",
                    line.trim()
                )),
                None => violations.push(format!(
                    "{rel}:{line_no}: no marker naming why: {}",
                    line.trim()
                )),
            }
        }
    }
    assert!(
        violations.is_empty(),
        "temp_dir() called with no `// osf: temp-dir allowed, <reason>` marker on the same line:\n{}",
        violations.join("\n")
    );
}
