//! Keeps `.osf/moon.yml`'s wide-glob osf check tasks in sync with every
//! `.gitignore` in this repository: moon's own glob-based input hashing
//! does not read `.gitignore`, so a git-ignored build or dependency folder
//! that a task does not separately exclude would join its cache
//! fingerprint and change on every unrelated run, the same way moon's own
//! cache state and a `target/` directory once did. Moon is required on
//! `PATH` for the per-task checks, which read each task's own resolved
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
/// `wide_glob_tasks_is_exactly_every_osf_check_task` checks this list
/// against `.osf/moon.yml` itself, so a check task added here without
/// being added there fails loudly instead of silently going unchecked.
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
    "review",
];

/// The moon.yml tasks that are not osf checks: narrow, Rust-source inputs,
/// and not part of [`WIDE_GLOB_TASKS`].
const NON_CHECK_TASKS: &[&str] = &["fmt", "clippy", "test", "test-nested"];

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

/// What a `.gitignore` entry with no trailing slash names.
#[derive(Debug, PartialEq, Eq)]
enum SlashlessKind {
    /// `base` (the `.gitignore`'s own directory) holds an actual directory
    /// by that name right now: unambiguous, whatever its name looks like.
    ConfirmedDirectory,
    /// No directory exists there today, but the entry's final path segment
    /// carries no `.` at all — a real directory name (`vendor`) and a bare
    /// file name (`LICENSE`, `Makefile`) look exactly alike here, so this
    /// entry could be either.
    PlausibleDirectoryOrFile,
    /// The final path segment has a `.` in it (`build.txt`, `*.log`): a
    /// much stronger signal of a file pattern than of a directory's name.
    NotADirectory,
}

fn classify_slashless_entry(base: &Path, entry: &str) -> SlashlessKind {
    if base.join(entry).is_dir() {
        return SlashlessKind::ConfirmedDirectory;
    }
    let basename = entry.rsplit('/').next().unwrap_or(entry);
    if basename.contains('.') {
        SlashlessKind::NotADirectory
    } else {
        SlashlessKind::PlausibleDirectoryOrFile
    }
}

/// Whether `line`, from a `.gitignore`, names nothing: blank, a comment,
/// or a negation (`!pattern` un-ignores something; it never adds a
/// directory this test needs to track).
fn names_nothing(line: &str) -> bool {
    line.is_empty() || line.starts_with('#') || line.starts_with('!')
}

/// The directory-shaped entries a `.gitignore`'s raw text names, checked
/// against `base` (its own directory) for a slashless entry that might
/// still be one. A trailing-slash entry is always an unambiguous
/// directory, the slash stripped. The `bool` is `true` when the entry is
/// only plausibly a directory (see [`SlashlessKind::PlausibleDirectoryOrFile`]):
/// it might equally be a bare file, so a caller must require an exclude
/// that works for either.
fn directory_entries_in(base: &Path, text: &str) -> Vec<(String, bool)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !names_nothing(l))
        .filter_map(|l| {
            if let Some(stripped) = l.strip_suffix('/') {
                Some((stripped.to_string(), false))
            } else {
                match classify_slashless_entry(base, l) {
                    SlashlessKind::ConfirmedDirectory => Some((l.to_string(), false)),
                    SlashlessKind::PlausibleDirectoryOrFile => Some((l.to_string(), true)),
                    SlashlessKind::NotADirectory => None,
                }
            }
        })
        .collect()
}

fn directory_entries(path: &Path) -> Vec<(String, bool)> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    directory_entries_in(base, &text)
}

fn is_allowed_exception(path: &str) -> bool {
    EXCEPTIONS.iter().any(|e| e.path == path)
}

/// The length, in path segments, of the shortest ancestor of `path`
/// covered by a `!/<ancestor>/**` exclude in `moon_yml` — `1` for `path`
/// itself when nothing shorter covers it — or `None` if no ancestor is
/// covered at all.
fn shortest_covering_ancestor_len(moon_yml: &str, path: &str) -> Option<usize> {
    let segments: Vec<&str> = path.split('/').collect();
    (1..=segments.len()).find(|&n| {
        let Some(prefix) = segments.get(..n) else {
            return false;
        };
        let ancestor = prefix.join("/");
        moon_yml.contains(&format!("!/{ancestor}/**"))
    })
}

/// Whether `moon_yml`'s text names the exact, bare exclude `!/<path>`
/// (with nothing after it but the closing quote): the form that excludes
/// `path` if it turns out to be a plain file rather than a directory. A
/// directory-shaped `!/<path>/**` entry does not satisfy this: it starts
/// with the same text but never ends there.
fn covered_by_a_bare_exclude(moon_yml: &str, path: &str) -> bool {
    moon_yml.contains(&format!("!/{path}'"))
}

/// Whether `path` is covered well enough for its own ambiguity.
///
/// A *strict* ancestor exclude (one shorter than `path` itself, such as
/// `!/.superpowers/**` covering `.superpowers/sdd/anything`) already
/// reaches every file and directory beneath it, so nothing more is needed
/// regardless of ambiguity. Only when the sole covering exclude sits at
/// `path`'s own exact length — `!/<path>/**`, naming this entry and no
/// shorter one — does the file case matter: that pattern excludes a
/// directory's contents, but not a plain file of the same name, so an
/// entry that might be either also needs [`covered_by_a_bare_exclude`].
fn is_fully_covered(moon_yml: &str, path: &str, ambiguous: bool) -> bool {
    let own_len = path.split('/').count();
    match shortest_covering_ancestor_len(moon_yml, path) {
        None => false,
        Some(covering_len) if covering_len < own_len => true,
        Some(_) => !ambiguous || covered_by_a_bare_exclude(moon_yml, path),
    }
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
        for (entry, ambiguous) in directory_entries(gitignore) {
            let full_rel = if parent_rel.is_empty() {
                entry
            } else {
                format!("{parent_rel}/{entry}")
            };
            if is_allowed_exception(&full_rel) {
                continue;
            }
            if !is_fully_covered(&moon_yml, &full_rel, ambiguous) {
                let named_in = gitignore
                    .strip_prefix(&root)
                    .expect("under the repository root")
                    .display();
                let hint = if ambiguous {
                    " (this entry could be a file or a directory; both `!/<path>` and `!/<path>/**` are needed)"
                } else {
                    ""
                };
                missing.push(format!("{full_rel} (named in {named_in}){hint}"));
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
/// `moon` call here must resolve `root`'s own workspace, not whichever one
/// an outer `MOON_WORKSPACE_ROOT` names.
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

/// Every task id `.osf/moon.yml` defines for the `osf` project, from
/// `moon project osf --json`.
fn all_task_ids(root: &Path) -> Vec<String> {
    let mut command = Command::new("moon");
    command.args(["project", "osf", "--json"]).current_dir(root);
    strip_inherited_moon_vars(&mut command);
    let output = command
        .output()
        .unwrap_or_else(|e| panic!("cannot run moon project osf: {e}"));
    assert!(
        output.status.success(),
        "moon project osf failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("moon project osf's output is not JSON: {e}"));
    json.get("tasks")
        .and_then(serde_json::Value::as_object)
        .unwrap_or_else(|| panic!("moon project osf's output has no tasks object"))
        .keys()
        .cloned()
        .collect()
}

/// `WIDE_GLOB_TASKS` must name every osf check task and nothing else: a
/// check task added to `.osf/moon.yml` without being added to
/// `WIDE_GLOB_TASKS` would otherwise never be checked by
/// `every_wide_glob_osf_task_references_the_shared_exclude_group` or by
/// `every_gitignored_directory_is_covered_by_the_shared_exclude_group`'s
/// own assumptions about what the group must reach.
#[test]
fn wide_glob_tasks_is_exactly_every_osf_check_task() {
    let root = repo_root();
    let mut actual: Vec<String> = all_task_ids(&root)
        .into_iter()
        .filter(|id| !NON_CHECK_TASKS.contains(&id.as_str()))
        .collect();
    actual.sort();
    let mut expected: Vec<String> = WIDE_GLOB_TASKS.iter().map(|s| (*s).to_string()).collect();
    expected.sort();
    assert_eq!(
        actual, expected,
        "WIDE_GLOB_TASKS must equal every osf check task (every task except {NON_CHECK_TASKS:?})"
    );
}

#[test]
fn the_parser_counts_a_slashless_directory_name_but_not_a_file_pattern() {
    // A fresh, empty directory: no `vendor` subdirectory actually exists
    // here, so the "no extension" fallback alone must be enough to count
    // it (as an ambiguous entry, since it is unconfirmed), and the
    // extension-bearing entries must still be excluded even so.
    let dir = TempDir::new("osf-gitignore-scan-excludes-test-parser");
    let text = "vendor\nvendor/\n*.log\nbuild.txt\n";
    let entries = directory_entries_in(&dir, text);
    assert_eq!(
        entries,
        vec![("vendor".to_string(), true), ("vendor".to_string(), false)]
    );
}

#[test]
fn a_slashless_entry_also_counts_when_the_directory_actually_exists() {
    let dir = TempDir::new("osf-gitignore-scan-excludes-test-exists");
    std::fs::create_dir_all(dir.join("vendor")).expect("vendor dir creates");
    assert_eq!(
        classify_slashless_entry(&dir, "vendor"),
        SlashlessKind::ConfirmedDirectory,
        "an actual directory on disk must count, even with no trailing slash"
    );
}

/// `LICENSE` and `Makefile`-shaped entries: no dot, no directory on disk,
/// so they could be a bare file or a not-yet-created directory. Coverage
/// must demand both the bare and the directory-shaped exclude form; either
/// alone is not enough.
#[test]
fn an_ambiguous_slashless_entry_needs_both_the_bare_and_directory_forms() {
    let dir = TempDir::new("osf-gitignore-scan-excludes-test-ambiguous");
    assert_eq!(
        classify_slashless_entry(&dir, "LICENSE"),
        SlashlessKind::PlausibleDirectoryOrFile
    );

    let only_directory_form = "ignored-dirs:\n    - '!/LICENSE/**'\n";
    let only_bare_form = "ignored-dirs:\n    - '!/LICENSE'\n";
    let both_forms = "ignored-dirs:\n    - '!/LICENSE'\n    - '!/LICENSE/**'\n";

    assert!(!is_fully_covered(only_directory_form, "LICENSE", true));
    assert!(!is_fully_covered(only_bare_form, "LICENSE", true));
    assert!(is_fully_covered(both_forms, "LICENSE", true));

    // The same directory-only text is enough for an entry known, not
    // merely guessed, to be a directory.
    assert!(is_fully_covered(only_directory_form, "LICENSE", false));
}
