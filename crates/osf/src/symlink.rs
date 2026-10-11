//! Symlink resolution shared by `osf check` and `osf scan`: a link whose
//! real target is inside the repository is followed to the files behind it,
//! and one that points outside the repository, into `.git`, at nothing, or
//! in a loop is refused.

use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Expands every symlink among `files`, which are repository-relative
/// forward-slash paths. A plain file and a missing path pass through
/// unchanged, so the caller's own not-found error still fires for the
/// second. A symlink is replaced by the repository-relative paths of the
/// real files behind it. Duplicates are removed, first seen wins.
///
/// # Errors
/// Returns an error naming the link when it points outside the repository,
/// into `.git`, at nothing, or in a loop.
pub(crate) fn expand_links(root: &Path, files: &[String]) -> Result<Vec<String>, String> {
    let canonical_root =
        std::fs::canonicalize(root).map_err(|e| format!("cannot read {}: {e}", root.display()))?;
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for file in files {
        let logical = root.join(file);
        let meta = match std::fs::symlink_metadata(&logical) {
            Ok(meta) => meta,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                push_unique(&mut out, &mut seen, file.clone());
                continue;
            }
            Err(e) => return Err(format!("cannot read {file}: {e}")),
        };
        if !meta.file_type().is_symlink() {
            push_unique(&mut out, &mut seen, file.clone());
            continue;
        }
        let label = Path::new(file);
        let resolved = resolve_link_from(&canonical_root, label, &logical, &mut Vec::new())?;
        for (_label, real) in resolved {
            let rel = real.strip_prefix(&canonical_root).unwrap_or(&real);
            push_unique(
                &mut out,
                &mut seen,
                rel.to_string_lossy().replace('\\', "/"),
            );
        }
    }
    Ok(out)
}

/// Adds `value` to `out` once, keeping the first-seen order.
fn push_unique(out: &mut Vec<String>, seen: &mut HashSet<String>, value: String) {
    if seen.insert(value.clone()) {
        out.push(value);
    }
}

/// Resolves the symlink at `link` against the repository `root`, returning
/// one `(label, real)` pair per file behind it: the path walked through the
/// link, and the real file to read.
///
/// # Errors
/// Returns an error naming `link` when it points outside the repository,
/// into `.git`, at nothing, or in a loop.
pub(crate) fn resolve_link(root: &Path, link: &Path) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let canonical_root =
        std::fs::canonicalize(root).map_err(|e| format!("cannot read {}: {e}", root.display()))?;
    resolve_link_from(&canonical_root, link, link, &mut Vec::new())
}

/// Resolves the link at `link`, named `label` in errors and labels, with
/// `ancestors` the real directories already being walked behind a link.
fn resolve_link_from(
    canonical_root: &Path,
    label: &Path,
    link: &Path,
    ancestors: &mut Vec<PathBuf>,
) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let target = follow(link, label)?;
    if !inside_repository(canonical_root, &target) {
        return Err(refused(label, "points outside the repository"));
    }
    if !target.is_dir() {
        return Ok(vec![(label.to_path_buf(), target)]);
    }
    if ancestors.contains(&target) {
        return Err(refused(label, "loops"));
    }
    ancestors.push(target.clone());
    let mut out = Vec::new();
    walk_dir(canonical_root, label, &target, ancestors, &mut out)?;
    ancestors.pop();
    Ok(out)
}

/// Follows `link`'s symlink chain to its real target. `label` names the
/// original link in every error.
fn follow(link: &Path, label: &Path) -> Result<PathBuf, String> {
    let mut current = link.to_path_buf();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    loop {
        let meta = match std::fs::symlink_metadata(&current) {
            Ok(meta) => meta,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Err(refused(label, "points at nothing"));
            }
            Err(e) => return Err(format!("cannot read {}: {e}", current.display())),
        };
        if !meta.file_type().is_symlink() {
            return std::fs::canonicalize(&current)
                .map_err(|e| format!("cannot read {}: {e}", current.display()));
        }
        if !seen.insert(link_key(&current)) {
            return Err(refused(label, "loops"));
        }
        let target = std::fs::read_link(&current)
            .map_err(|e| format!("cannot read {}: {e}", current.display()))?;
        current = if target.is_absolute() {
            target
        } else {
            let parent = current.parent().unwrap_or_else(|| Path::new("."));
            parent.join(target)
        };
    }
}

/// The identity of the link at `path`: its real parent folder plus its name,
/// so the same link reached two ways reads as one.
fn link_key(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    match (std::fs::canonicalize(parent), path.file_name()) {
        (Ok(parent), Some(name)) => parent.join(name),
        _ => path.to_path_buf(),
    }
}

/// Whether `target`, already real, is inside `canonical_root` but not inside
/// its `.git` folder.
fn inside_repository(canonical_root: &Path, target: &Path) -> bool {
    target.starts_with(canonical_root) && !target.starts_with(canonical_root.join(".git"))
}

/// The one refusal shape: the link, then why.
fn refused(link: &Path, reason: &str) -> String {
    format!("symlink refused: {} {reason}", link.display())
}

/// Walks the real directory `dir` behind a link into `(label, real)` pairs,
/// sorted, skipping `.git`, and resolving any nested link by the same rules.
fn walk_dir(
    canonical_root: &Path,
    label_base: &Path,
    dir: &Path,
    ancestors: &mut Vec<PathBuf>,
    out: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), String> {
    let mut entries = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .collect::<Result<Vec<_>, std::io::Error>>()
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let real = entry.path();
        let label = label_base.join(&name);
        let meta = std::fs::symlink_metadata(&real)
            .map_err(|e| format!("cannot read {}: {e}", label.display()))?;
        if meta.file_type().is_symlink() {
            out.extend(resolve_link_from(canonical_root, &label, &real, ancestors)?);
        } else if meta.is_dir() {
            let canonical = std::fs::canonicalize(&real)
                .map_err(|e| format!("cannot read {}: {e}", label.display()))?;
            if ancestors.contains(&canonical) {
                return Err(refused(&label, "loops"));
            }
            ancestors.push(canonical);
            walk_dir(canonical_root, &label, &real, ancestors, out)?;
            ancestors.pop();
        } else {
            out.push((label, real));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    /// Writes `content` to `dir`/`path`, creating parent folders.
    fn write(dir: &Path, path: &str, content: &str) {
        let full = dir.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("parent creates");
        }
        std::fs::write(full, content).expect("fixture writes");
    }

    /// Creates a symlink at `link` pointing at `target`.
    #[cfg(unix)]
    fn symlink(link: &Path, target: &str) {
        std::os::unix::fs::symlink(target, link).expect("symlink creates");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_an_inside_directory_expands_to_its_real_files() {
        let root = TempDir::new("osf-symlink-inside-dir");
        write(&root, "real/b.md", "b\n");
        write(&root, "real/a.md", "a\n");
        symlink(&root.join("link"), "real");
        let expanded = expand_links(&root, &["link".to_string()]).expect("expands");
        assert_eq!(
            expanded,
            vec!["real/a.md".to_string(), "real/b.md".to_string()]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_an_inside_file_expands_to_that_real_file() {
        let root = TempDir::new("osf-symlink-inside-file");
        write(&root, "real/a.md", "a\n");
        symlink(&root.join("link"), "real/a.md");
        let expanded = expand_links(&root, &["link".to_string()]).expect("expands");
        assert_eq!(expanded, vec!["real/a.md".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_an_outside_directory_is_refused() {
        let root = TempDir::new("osf-symlink-outside-root");
        let outside = TempDir::new("osf-symlink-outside-target");
        write(&outside, "secret.md", "secret\n");
        symlink(&root.join("link"), &outside.to_string_lossy());
        let err = expand_links(&root, &["link".to_string()]).expect_err("refused");
        assert!(err.starts_with("symlink refused:"), "{err}");
        assert!(err.contains("outside"), "{err}");
        assert!(err.contains("link"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_the_parent_directory_loops() {
        let root = TempDir::new("osf-symlink-parent-loop");
        write(&root, "real/sub/file.md", "x\n");
        symlink(&root.join("real/sub/loop"), "..");
        let err = expand_links(&root, &["real/sub/loop".to_string()]).expect_err("refused");
        assert!(err.starts_with("symlink refused:"), "{err}");
        assert!(err.contains("loops"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn two_links_pointing_at_each_other_are_refused() {
        let root = TempDir::new("osf-symlink-mutual");
        symlink(&root.join("a"), "b");
        symlink(&root.join("b"), "a");
        let err = expand_links(&root, &["a".to_string()]).expect_err("refused");
        assert!(err.starts_with("symlink refused:"), "{err}");
        assert!(err.contains("loops"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_link_is_refused() {
        let root = TempDir::new("osf-symlink-dangling");
        symlink(&root.join("link"), "nowhere");
        let err = expand_links(&root, &["link".to_string()]).expect_err("refused");
        assert!(err.starts_with("symlink refused:"), "{err}");
        assert!(err.contains("nothing"), "{err}");
    }

    #[test]
    fn a_plain_path_and_a_missing_path_pass_through_unchanged() {
        let root = TempDir::new("osf-symlink-passthrough");
        write(&root, "plain.md", "plain\n");
        let expanded = expand_links(&root, &["plain.md".to_string(), "missing.md".to_string()])
            .expect("passes through");
        assert_eq!(
            expanded,
            vec!["plain.md".to_string(), "missing.md".to_string()]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_into_the_git_folder_is_refused() {
        let root = TempDir::new("osf-symlink-git");
        write(&root, ".git/config", "[core]\n");
        symlink(&root.join("link"), ".git");
        let err = expand_links(&root, &["link".to_string()]).expect_err("refused");
        assert!(err.starts_with("symlink refused:"), "{err}");
        assert!(err.contains("outside"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn duplicate_expansions_keep_first_seen_order() {
        let root = TempDir::new("osf-symlink-dedup");
        write(&root, "real/a.md", "a\n");
        symlink(&root.join("one"), "real");
        symlink(&root.join("two"), "real");
        let expanded = expand_links(
            &root,
            &[
                "one".to_string(),
                "two".to_string(),
                "real/a.md".to_string(),
            ],
        )
        .expect("expands");
        assert_eq!(expanded, vec!["real/a.md".to_string()]);
    }
}
