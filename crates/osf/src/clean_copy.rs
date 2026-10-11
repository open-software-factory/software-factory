//! A reviewer's working copy of a change: a temporary folder holding the
//! change's files and none of the settings, plugins or instruction files a
//! coding agent loads from the folder it starts in.
//!
//! A reviewer starts in this copy, never in the checkout of the change, so
//! the change cannot configure the agent that reviews it. A symbolic link is
//! never followed and never copied, so nothing in the copy points outside it.
//! Which names are left out comes from [`crate::agents::project_settings`].

use crate::{agents, git};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

/// The folder is removed when this value is dropped.
#[derive(Debug)]
pub struct CleanCopy {
    dir: PathBuf,
}

impl CleanCopy {
    /// Copies the files of the repository at `root` that git tracks, plus
    /// those it neither tracks nor ignores, into a new folder under the OS
    /// temp directory, leaving out every agent's project settings and every
    /// symbolic link.
    ///
    /// # Errors
    /// Names the reason when git cannot list the files, or the folder or a
    /// file cannot be written.
    pub fn create(root: &Path) -> Result<Self, String> {
        Self::copy(root, false)
    }

    /// The same copy as [`create`](Self::create), then made read-only, deepest
    /// first, so the coding agent that starts in it cannot change it.
    ///
    /// # Errors
    /// Names the reason when git cannot list the files, the folder or a file
    /// cannot be written, or its read-only mode cannot be set.
    pub fn create_read_only(root: &Path) -> Result<Self, String> {
        Self::copy(root, true)
    }

    /// The one copy body both constructors share.
    fn copy(root: &Path, read_only: bool) -> Result<Self, String> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let mut files = git::tracked_files(root, None).map_err(|e| format!("files: {e}"))?;
        files.extend(git::untracked_files(root).map_err(|e| format!("files: {e}"))?);
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir() // osf: temp-dir allowed, one clean copy per reviewer run
            .join(format!(
                "osf-review-copy-{}-{unique}-{n}",
                std::process::id()
            ));
        std::fs::create_dir(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let copy = Self { dir };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&copy.dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("cannot protect {}: {e}", copy.dir.display()))?;
        }
        let left_out = agents::project_settings();
        for rel in &files {
            copy.add(root, rel, &left_out)?;
        }
        if read_only {
            set_read_only_tree(&copy.dir, true)?;
        }
        Ok(copy)
    }

    /// The copy's folder.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Copies the file `rel` under `root`, unless it or a folder above it is
    /// a left-out name or a symbolic link, or it is not a plain file.
    fn add(&self, root: &Path, rel: &str, left_out: &[&str]) -> Result<(), String> {
        let rel_path = Path::new(rel);
        let plain = rel_path
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
        let named_out = rel_path.components().any(|c| {
            left_out
                .iter()
                .any(|name| c.as_os_str().eq_ignore_ascii_case(OsStr::new(name)))
        });
        if !plain || named_out {
            return Ok(());
        }
        let mut walked = root.to_path_buf();
        let mut last = None;
        for component in rel_path.components() {
            walked.push(component);
            match std::fs::symlink_metadata(&walked) {
                Ok(meta) if meta.file_type().is_symlink() => return Ok(()),
                Ok(meta) => last = Some(meta),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(format!("{}: {e}", walked.display())),
            }
        }
        if !last.is_some_and(|meta| meta.is_file()) {
            return Ok(());
        }
        let dest = self.dir.join(rel_path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::copy(&walked, &dest).map_err(|e| format!("{}: {e}", walked.display()))?;
        Ok(())
    }
}

/// Turns `path`'s write permission off or on for every file and folder under
/// it, children before the folder that holds them.
fn set_read_only_tree(path: &Path, read_only: bool) -> Result<(), String> {
    if path.is_dir() {
        let entries =
            std::fs::read_dir(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        for entry in entries {
            let child = entry
                .map_err(|e| format!("cannot read {}: {e}", path.display()))?
                .path();
            set_read_only_tree(&child, read_only)?;
        }
    }
    set_read_only(path, read_only)
}

/// Turns `path`'s write permission off or on: files 0o444 and folders 0o555 on
/// Unix, the read-only attribute on each file elsewhere.
fn set_read_only(path: &Path, read_only: bool) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = match (path.is_dir(), read_only) {
            (true, true) => 0o555,
            (true, false) => 0o700,
            (false, true) => 0o444,
            (false, false) => 0o600,
        };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .map_err(|e| format!("cannot protect {}: {e}", path.display()))
    }
    #[cfg(not(unix))]
    {
        // Windows carries read-only only on a file, and it is best effort.
        if path.is_dir() {
            return Ok(());
        }
        let mut permissions = std::fs::metadata(path)
            .map_err(|e| format!("cannot protect {}: {e}", path.display()))?
            .permissions();
        permissions.set_readonly(read_only);
        std::fs::set_permissions(path, permissions)
            .map_err(|e| format!("cannot protect {}: {e}", path.display()))
    }
}

impl Drop for CleanCopy {
    fn drop(&mut self) {
        let _ = set_read_only_tree(&self.dir, false);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;
    use std::process::Command;

    fn repo_with(files: &[(&str, &str)]) -> TempDir {
        let root = TempDir::new("osf-clean-copy-repo");
        let git = |args: &[&str]| {
            let out = Command::new("git")
                .current_dir(&root)
                .args(args)
                .output()
                .expect("git runs");
            assert!(out.status.success(), "git {args:?}: {out:?}");
        };
        git(&["init", "--quiet"]);
        for (path, text) in files {
            let full = root.join(path);
            std::fs::create_dir_all(full.parent().expect("parent")).expect("dirs");
            std::fs::write(full, text).expect("file writes");
        }
        git(&["add", "--all"]);
        root
    }

    fn listed(dir: &Path) -> Vec<String> {
        let mut found = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            for entry in std::fs::read_dir(&current).expect("dir reads") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let rel = path.strip_prefix(dir).expect("inside");
                    found.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        found.sort();
        found
    }

    #[test]
    fn the_copy_holds_the_changes_files_and_no_agent_settings() {
        let root = repo_with(&[
            ("src/lib.rs", "fn one() {}\n"),
            ("README.md", "text\n"),
            (".opencode/plugin/extra.js", "x"),
            ("opencode.json", "{}"),
            ("opencode.jsonc", "{}"),
            (".omp/agent/hooks/a/index.js", "x"),
            (".codex/hooks.json", "{}"),
            (".claude/settings.json", "{}"),
            (".mcp.json", "{}"),
            (".cursor/rules/a.mdc", "x"),
            (".dsh/cordis.patch.yml", "x"),
            ("AGENTS.md", "x"),
            ("CLAUDE.md", "x"),
            ("docs/AGENTS.md", "x"),
            ("docs/claude.md", "x"),
            ("pkg/.claude/commands/a.md", "x"),
        ]);
        let copy = CleanCopy::create(&root).expect("copy creates");
        assert_eq!(listed(copy.dir()), vec!["README.md", "src/lib.rs"]);
    }

    #[test]
    fn a_file_git_does_not_track_or_ignore_is_copied_and_an_ignored_one_is_not() {
        let root = repo_with(&[(".gitignore", "built/\n"), ("a.txt", "a")]);
        std::fs::write(root.join("fresh.txt"), "new").expect("writes");
        std::fs::create_dir_all(root.join("built")).expect("dir");
        std::fs::write(root.join("built/out.bin"), "x").expect("writes");
        let copy = CleanCopy::create(&root).expect("copy creates");
        assert_eq!(listed(copy.dir()), vec![".gitignore", "a.txt", "fresh.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_symbolic_link_in_the_change_is_neither_followed_nor_copied() {
        let outside = TempDir::new("osf-clean-copy-outside");
        std::fs::write(outside.join("secret.txt"), "outside").expect("writes");
        std::fs::create_dir_all(outside.join("dir")).expect("dir");
        std::fs::write(outside.join("dir/inner.txt"), "outside").expect("writes");
        let root = repo_with(&[("real.txt", "inside")]);
        std::os::unix::fs::symlink(outside.join("secret.txt"), root.join("file-link"))
            .expect("file link");
        std::os::unix::fs::symlink(outside.join("dir"), root.join("dir-link")).expect("dir link");
        std::os::unix::fs::symlink("real.txt", root.join("inside-link")).expect("inside link");
        let copy = CleanCopy::create(&root).expect("copy creates");
        assert_eq!(listed(copy.dir()), vec!["real.txt"]);
        for entry in std::fs::read_dir(copy.dir()).expect("dir reads") {
            let meta = entry.expect("entry").metadata().expect("meta");
            assert!(!meta.file_type().is_symlink());
        }
    }

    #[test]
    fn the_copy_is_removed_when_it_is_dropped() {
        let root = repo_with(&[("a.txt", "a")]);
        let copy = CleanCopy::create(&root).expect("copy creates");
        let dir = copy.dir().to_path_buf();
        assert!(dir.join("a.txt").is_file());
        drop(copy);
        assert!(!dir.exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_copy_has_read_only_files_and_folders_and_resists_writing() {
        use std::os::unix::fs::PermissionsExt as _;
        let root = repo_with(&[("a.txt", "a"), ("dir/b.txt", "b")]);
        let copy = CleanCopy::create_read_only(&root).expect("copy creates");
        let mode = |path: &Path| {
            std::fs::metadata(path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode(copy.dir()), 0o555);
        assert_eq!(mode(&copy.dir().join("a.txt")), 0o444);
        assert_eq!(mode(&copy.dir().join("dir")), 0o555);
        assert_eq!(mode(&copy.dir().join("dir/b.txt")), 0o444);
        let err = std::fs::write(copy.dir().join("new.txt"), "x").expect_err("write fails");
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn a_read_only_copy_holds_the_same_files_as_a_writable_one() {
        let root = repo_with(&[
            ("src/lib.rs", "fn one() {}\n"),
            ("README.md", "text\n"),
            ("nested/deep/note.md", "x"),
        ]);
        let writable = CleanCopy::create(&root).expect("copy creates");
        let read_only = CleanCopy::create_read_only(&root).expect("copy creates");
        assert_eq!(listed(read_only.dir()), listed(writable.dir()));
    }

    #[test]
    fn a_read_only_copy_is_removed_when_it_is_dropped() {
        let root = repo_with(&[("a.txt", "a")]);
        let copy = CleanCopy::create_read_only(&root).expect("copy creates");
        let dir = copy.dir().to_path_buf();
        assert!(dir.join("a.txt").is_file());
        drop(copy);
        assert!(!dir.exists());
    }
}
