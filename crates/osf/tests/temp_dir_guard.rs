//! Ruling F8: every temporary test directory must come from one of the two
//! shared helpers, so two processes running the same test never collide.
//! This walks the crate's own source and fails on any other direct call to
//! the OS temp directory, naming the file and line.

use std::path::{Path, PathBuf};

/// Where a file may build a path from the OS temp directory directly.
enum Lines {
    /// The shared helper's own file: every line in it is allowed.
    Whole,
    /// Production code that already keys its own path uniquely and is
    /// never wiped before use: only these one-based lines are allowed.
    Only(&'static [usize]),
}

/// A file's own allowance, matched against its crate-root-relative path.
struct Allowed {
    path: &'static str,
    lines: Lines,
}

const ALLOWED: &[Allowed] = &[
    Allowed {
        path: "src/test_support.rs",
        lines: Lines::Whole,
    },
    Allowed {
        path: "tests/common/mod.rs",
        lines: Lines::Whole,
    },
    Allowed {
        path: "src/review.rs",
        lines: Lines::Only(&[737]),
    },
    Allowed {
        path: "src/hook.rs",
        lines: Lines::Only(&[619, 638]),
    },
    Allowed {
        path: "src/checkpoint.rs",
        lines: Lines::Only(&[227]),
    },
    Allowed {
        path: "src/status.rs",
        lines: Lines::Only(&[740]),
    },
];

/// This file's own name: it names the call syntax in a string to search
/// for it, which is not itself a call.
const SELF_FILE: &str = "temp_dir_guard.rs";

const PATTERN: &str = "temp_dir()";

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

fn allowance_for(rel: &str) -> Option<&'static Lines> {
    ALLOWED.iter().find(|a| a.path == rel).map(|a| &a.lines)
}

#[test]
fn no_file_calls_the_os_temp_dir_outside_the_two_shared_helpers() {
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
        let allow = allowance_for(&rel);
        if matches!(allow, Some(Lines::Whole)) {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        for (i, line) in text.lines().enumerate() {
            if !line.contains(PATTERN) {
                continue;
            }
            let line_no = i + 1;
            let ok = matches!(allow, Some(Lines::Only(lines)) if lines.contains(&line_no));
            if !ok {
                violations.push(format!("{rel}:{line_no}: {}", line.trim()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "temp_dir() called outside the shared helpers:\n{}",
        violations.join("\n")
    );
}
