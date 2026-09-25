//! Keeps `.osf/moon.yml`'s `scan-*` input excludes in sync with every
//! `.gitignore` in this repository: moon's own glob-based input hashing
//! does not read `.gitignore`, so a newly git-ignored build or dependency
//! folder that `scan-*` does not separately exclude would join its cache
//! fingerprint and change on every unrelated run, the same way moon's own
//! cache state and a `target/` directory once did.

use std::path::{Path, PathBuf};

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

/// The directory-shaped entries in the `.gitignore` at `path`: a trimmed,
/// non-comment, non-empty line ending in `/`, with the trailing slash
/// removed. A pattern with no trailing slash (a bare filename such as
/// `Thumbs.db`, or a glob such as `*`) names no directory and is skipped.
fn directory_entries(path: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && l.ends_with('/'))
        .map(|l| l.trim_end_matches('/').to_string())
        .collect()
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

#[test]
fn every_gitignored_directory_is_excluded_from_the_scan_tasks_wide_glob() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let root = root
        .canonicalize()
        .unwrap_or_else(|e| panic!("cannot canonicalise {}: {e}", root.display()));

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
        ".osf/moon.yml's scan-* tasks do not exclude every git-ignored directory:\n{}",
        missing.join("\n")
    );
}
