//! Keeps `.osf/moon.yml`'s wide-glob osf check tasks in sync with every
//! `.gitignore` in this repository: moon's own glob-based input hashing
//! does not read `.gitignore`, so a git-ignored build or dependency folder
//! that a task does not separately exclude would join its cache
//! fingerprint and change on every unrelated run, the same way moon's own
//! cache state and a `target/` directory once did. Moon is required on
//! `PATH` for the per-task check, which reads each task's own resolved
//! inputs from `moon task <target> --json` rather than trusting the raw
//! YAML text.

mod common;
use common::TempDir;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The file group `.osf/moon.yml` defines, holding every exclude, and
/// referenced from every wide-glob task as `@group(<name>)`.
const GROUP: &str = "ignored-dirs";

/// Every osf check task whose own glob is not scoped to a narrow, known
/// file type: these are the ones a stray file under a git-ignored
/// directory could reach, so each one must reference [`GROUP`]. `fmt`,
/// `clippy` and `test` are not here: their inputs are Rust-source-specific
/// (`*.rs`, `Cargo.toml`, and the like) and structurally cannot match
/// anything under an integration's own `node_modules/` or `dist/`.
const WIDE_GLOB_TASKS: &[&str] = &[
    "scan-staged",
    "scan-hook",
    "scan-pre-push",
    "scan-gate",
    "scan-commits",
    "scan-commits-gate",
    "lint-writing-hook",
    "lint-writing-pre-push",
    "lint-writing-gate",
    "lint-skill-hook",
    "lint-skill-pre-push",
    "lint-skill-gate",
];

/// Directories this walk never descends into, since none holds a
/// `.gitignore` worth reading and some (`node_modules`, if ever installed)
/// could be large.
const NEVER_WALK: &[&str] = &[".git", "target", "target-linux", "node_modules"];

/// A directory a `.gitignore` names that is deliberately not required to
/// appear in `.osf/moon.yml`'s exclude list, with why.
struct Exception {
    /// Repository-relative, forward-slash path, no leading or trailing slash.
    path: &'static str,
    #[allow(dead_code)]
    reason: &'static str,
}

/// No exceptions today. If one is ever needed, name it here with a reason
/// rather than silencing this test another way.
const EXCEPTIONS: &[Exception] = &[];

fn collect_gitignores(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry reads").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if path.is_dir() {
            if NEVER_WALK.contains(&name) {
                continue;
            }
            collect_gitignores(&path, out);
        } else if name == ".gitignore" {
            out.push(path);
        }
    }
}

/// Whether a `.gitignore` entry with no trailing slash still names a
/// directory: either `base` (the `.gitignore`'s own directory) holds an
/// actual directory by that name right now, or the entry's final path
/// segment carries no `.` at all — a much stronger signal of a directory
/// name than of a file's (`vendor` yes, `build.txt` and `*.log` no).
fn slashless_entry_is_a_directory(base: &Path, entry: &str) -> bool {
    if base.join(entry).is_dir() {
        return true;
    }
    let basename = entry.rsplit('/').next().unwrap_or(entry);
    !basename.contains('.')
}

/// Whether `line`, from a `.gitignore`, names nothing: blank, a comment,
/// or a negation (`!pattern` un-ignores something; it never adds a
/// directory this test needs to track).
fn names_nothing(line: &str) -> bool {
    line.is_empty() || line.starts_with('#') || line.starts_with('!')
}

/// The directory-shaped entries a `.gitignore`'s raw text names, checked
/// against `base` (its own directory) for a slashless entry that might
/// still be one. A trailing-slash entry is always a directory, the slash
/// stripped.
fn directory_entries_in(base: &Path, text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !names_nothing(l))
        .filter_map(|l| {
            if let Some(stripped) = l.strip_suffix('/') {
                Some(stripped.to_string())
            } else if slashless_entry_is_a_directory(base, l) {
                Some(l.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn directory_entries(path: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    directory_entries_in(base, &text)
}

fn is_allowed_exception(path: &str) -> bool {
    EXCEPTIONS.iter().any(|e| e.path == path)
}

/// Whether `moon_yml`'s text names an exclude covering `path`: an exact
/// `!/<path>/**` entry, or one for any ancestor directory of `path` (so
/// `.moon/cache`, say, is covered by a plain `!/.moon/**`).
fn covered_by_an_exclude(moon_yml: &str, path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    (1..=segments.len()).any(|n| {
        let Some(prefix) = segments.get(..n) else {
            return false;
        };
        let ancestor = prefix.join("/");
        moon_yml.contains(&format!("!/{ancestor}/**"))
    })
}

fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    root.canonicalize()
        .unwrap_or_else(|e| panic!("cannot canonicalise {}: {e}", root.display()))
}

#[test]
fn every_gitignored_directory_is_covered_by_the_shared_exclude_group() {
    let root = repo_root();

    let mut gitignores = Vec::new();
    collect_gitignores(&root, &mut gitignores);
    assert!(
        !gitignores.is_empty(),
        "expected to find at least the repository's own root .gitignore"
    );

    let moon_yml_path = root.join(".osf").join("moon.yml");
    let moon_yml = std::fs::read_to_string(&moon_yml_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", moon_yml_path.display()));

    let mut missing = Vec::new();
    for gitignore in &gitignores {
        let parent = gitignore.parent().expect("a .gitignore has a parent dir");
        let parent_rel = parent
            .strip_prefix(&root)
            .expect("under the repository root")
            .to_string_lossy()
            .replace('\\', "/");
        for entry in directory_entries(gitignore) {
            let full_rel = if parent_rel.is_empty() {
                entry
            } else {
                format!("{parent_rel}/{entry}")
            };
            if is_allowed_exception(&full_rel) {
                continue;
            }
            if !covered_by_an_exclude(&moon_yml, &full_rel) {
                let named_in = gitignore
                    .strip_prefix(&root)
                    .expect("under the repository root")
                    .display();
                missing.push(format!("{full_rel} (named in {named_in})"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        ".osf/moon.yml's `{GROUP}` file group does not cover every git-ignored directory:\n{}",
        missing.join("\n")
    );
}

/// Removes every inherited `MOON_*` variable: this repository's own `test`
/// task sets them when `cargo test` itself runs under moon, and a nested
/// `moon task` call here must resolve `root`'s own workspace, not
/// whichever one an outer `MOON_WORKSPACE_ROOT` names.
fn strip_inherited_moon_vars(command: &mut Command) {
    for (key, _) in std::env::vars() {
        if key.starts_with("MOON_") {
            command.env_remove(key);
        }
    }
}

/// `target`'s own resolved inputs, from `moon task osf:<target> --json`,
/// as moon reports them — a file group reference included as its own
/// `@group(<name>)` string, not expanded.
fn task_inputs_json(root: &Path, target: &str) -> serde_json::Value {
    let mut command = Command::new("moon");
    command
        .args(["task", &format!("osf:{target}"), "--json"])
        .current_dir(root);
    strip_inherited_moon_vars(&mut command);
    let output = command
        .output()
        .unwrap_or_else(|e| panic!("cannot run moon task osf:{target}: {e}"));
    assert!(
        output.status.success(),
        "moon task osf:{target} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("moon task osf:{target}'s output is not JSON: {e}"))
}

/// Whether `task_json`'s own `inputs` array names `@group(<GROUP>)`.
fn references_the_group(task_json: &serde_json::Value) -> bool {
    task_json
        .get("inputs")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|inputs| {
            inputs
                .iter()
                .any(|i| i.as_str() == Some(&format!("@group({GROUP})")))
        })
}

#[test]
fn every_wide_glob_osf_task_references_the_shared_exclude_group() {
    let root = repo_root();
    let missing: Vec<&str> = WIDE_GLOB_TASKS
        .iter()
        .filter(|target| !references_the_group(&task_inputs_json(&root, target)))
        .copied()
        .collect();
    assert!(
        missing.is_empty(),
        "these osf tasks have a wide glob but do not reference `@group({GROUP})`: {}",
        missing.join(", ")
    );
}

#[test]
fn the_parser_counts_a_slashless_directory_name_but_not_a_file_pattern() {
    // A fresh, empty directory: no `vendor` subdirectory actually exists
    // here, so the "no extension" fallback alone must be enough to count
    // it, and the extension-bearing entries must still be excluded even
    // so.
    let dir = TempDir::new("osf-gitignore-scan-excludes-test-parser");
    let text = "vendor\nvendor/\n*.log\nbuild.txt\n";
    let entries = directory_entries_in(&dir, text);
    assert_eq!(entries, vec!["vendor".to_string(), "vendor".to_string()]);
}

#[test]
fn a_slashless_entry_also_counts_when_the_directory_actually_exists() {
    let dir = TempDir::new("osf-gitignore-scan-excludes-test-exists");
    std::fs::create_dir_all(dir.join("vendor")).expect("vendor dir creates");
    assert!(
        slashless_entry_is_a_directory(&dir, "vendor"),
        "an actual directory on disk must count, even with no trailing slash"
    );
}
