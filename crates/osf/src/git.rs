//! Minimal git plumbing: shells out to the `git` binary the same way a
//! person would, so `osf scan` and `osf verify` see exactly what git sees.
//! Every function takes the directory to run in, so a test can point it at
//! a throwaway repository instead of the real one.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Git could not run, or the command it ran failed. Distinct from git
/// running fine and reporting that there is nothing to see.
#[derive(Debug)]
pub struct GitError(String);

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GitError {}

/// Removes every inherited `GIT_*` variable from `command`, so a git call aimed at one directory is never redirected by a caller's own `GIT_DIR`/`GIT_WORK_TREE`/`GIT_INDEX_FILE`.
pub fn scrub_git_env(command: &mut Command) {
    for (key, _) in std::env::vars() {
        if key.starts_with("GIT_") {
            command.env_remove(key);
        }
    }
}

/// The `GIT_*` variables that tell git which repository to use, as opposed
/// to `GIT_INDEX_FILE`, which only names which index to read once the
/// repository is already settled.
const LOCATING_VARS: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
];

/// `dir`'s own `.git` directory, absolute, resolved with every locating
/// variable already removed so the answer is `dir`'s and not a caller's.
/// This is also the correct answer from inside a linked worktree: git
/// resolves it to that worktree's own `<main>/.git/worktrees/<name>`.
fn absolute_git_dir(dir: &Path) -> Option<PathBuf> {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .args(["rev-parse", "--absolute-git-dir"]);
    for var in LOCATING_VARS {
        command.env_remove(var);
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Some(PathBuf::from(text.trim()))
}

/// Whether `a` and `b` name the same path component, case-insensitively on
/// Windows (where the filesystem itself is), byte-for-byte elsewhere.
fn path_component_matches(a: &std::ffi::OsStr, b: &std::ffi::OsStr) -> bool {
    #[cfg(windows)]
    {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

/// Whether every component of `parent` is a prefix of `child`'s own
/// components, in order: `child` is `parent` itself or somewhere under it.
fn path_contains(parent: &Path, child: &Path) -> bool {
    let mut child_components = child.components();
    for parent_component in parent.components() {
        match child_components.next() {
            Some(c) if path_component_matches(parent_component.as_os_str(), c.as_os_str()) => {}
            _ => return false,
        }
    }
    true
}

/// Whether `index_file` lies inside `dir`'s own git directory: the only
/// case that makes it the index a caller working in `dir` is entitled to
/// read. Canonicalises both sides first, so a symlink or a case difference
/// never causes a false negative on Windows.
fn index_file_is_within_dirs_git_dir(index_file: &std::ffi::OsStr, dir: &Path) -> bool {
    let Some(git_dir) = absolute_git_dir(dir) else {
        return false;
    };
    let Ok(git_dir) = std::fs::canonicalize(&git_dir) else {
        return false;
    };
    let Ok(index_file) = std::fs::canonicalize(Path::new(index_file)) else {
        return false;
    };
    path_contains(&git_dir, &index_file)
}

/// Removes the repository-locating `GIT_*` variables from `command`, so a
/// git call aimed at `dir` finds its repository from `dir`, never from a
/// caller's own inherited `GIT_DIR`/`GIT_WORK_TREE`. An inherited
/// `GIT_INDEX_FILE` is kept only when its own absolute path lies inside
/// `dir`'s own git directory (a pre-commit hook's `git commit -a` sets
/// exactly that, so the staged-content scan can still read it); otherwise
/// it is removed, since a path outside `dir`'s own git directory names some
/// other repository's index and cannot be read against `dir`'s objects.
/// Also forces `LC_ALL=C` and removes `LANGUAGE`, so git's own messages are
/// stable English text a caller can match, whatever locale is inherited.
pub fn scrub_git_env_for_dir(command: &mut Command, dir: &Path) {
    for var in LOCATING_VARS {
        command.env_remove(var);
    }
    if let Some(index_file) = std::env::var_os("GIT_INDEX_FILE") {
        if !index_file_is_within_dirs_git_dir(&index_file, dir) {
            command.env_remove("GIT_INDEX_FILE");
        }
    }
    command.env("LC_ALL", "C");
    command.env_remove("LANGUAGE");
}

fn run(dir: &Path, args: &[&str]) -> Result<Vec<u8>, GitError> {
    let mut command = Command::new("git");
    command.current_dir(dir).args(args);
    scrub_git_env_for_dir(&mut command, dir);
    let output = command
        .output()
        .map_err(|e| GitError(format!("cannot run git: {e}")))?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(GitError(format!(
        "git {} failed: {}",
        args.join(" "),
        stderr.trim()
    )))
}

fn run_text(dir: &Path, args: &[&str]) -> Result<String, GitError> {
    run(dir, args).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

fn split_nul(raw: &[u8]) -> Vec<String> {
    raw.split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect()
}

/// Every path git tracks, optionally under one subtree.
///
/// # Errors
/// Returns an error if git cannot run in `dir`.
pub fn tracked_files(dir: &Path, under: Option<&Path>) -> Result<Vec<String>, GitError> {
    let mut args = vec!["ls-files".to_string(), "-z".to_string()];
    if let Some(p) = under {
        args.push(p.to_string_lossy().replace('\\', "/"));
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(dir, &refs).map(|raw| split_nul(&raw))
}

/// Every commit hash in `range`, oldest first.
///
/// # Errors
/// Returns an error if git cannot run in `dir`, such as when `range` does not resolve.
pub fn commit_hashes(dir: &Path, range: &str) -> Result<Vec<String>, GitError> {
    let text = run_text(dir, &["log", "--format=%H", "--reverse", range])?;
    Ok(text.lines().map(str::to_string).collect())
}

/// One commit's full message.
///
/// # Errors
/// Returns an error if git cannot run in `dir`, or `hash` does not resolve.
pub fn commit_message(dir: &Path, hash: &str) -> Result<String, GitError> {
    run_text(dir, &["log", "-1", "--format=%B", hash])
}

/// Paths staged for commit: added, copied, modified or renamed. A deleted
/// path is never scanned, since there is no content left to check.
///
/// # Errors
/// Returns an error if git cannot run in `dir`.
pub fn staged_files(dir: &Path) -> Result<Vec<String>, GitError> {
    run(
        dir,
        &[
            "diff",
            "--cached",
            "--name-only",
            "-z",
            "--diff-filter=ACMR",
        ],
    )
    .map(|raw| split_nul(&raw))
}

/// Paths that differ between `base` and `HEAD`: added, copied, modified or renamed.
///
/// # Errors
/// Returns an error if git cannot run in `dir`, such as when `base` does not resolve.
pub fn changed_files(dir: &Path, base: &str) -> Result<Vec<String>, GitError> {
    let range = format!("{base}...HEAD");
    run(
        dir,
        &["diff", "--name-only", "-z", "--diff-filter=ACMR", &range],
    )
    .map(|raw| split_nul(&raw))
}

/// The content of `path` as staged in the index right now.
///
/// # Errors
/// Returns an error if git cannot run in `dir`, or `path` is not staged.
pub fn staged_content(dir: &Path, path: &str) -> Result<Vec<u8>, GitError> {
    run(dir, &["show", &format!(":{path}")])
}

/// The content of `path` as it is at `rev`.
///
/// # Errors
/// Returns an error if git cannot run in `dir`, or `path` does not exist at `rev`.
pub fn content_at(dir: &Path, rev: &str, path: &str) -> Result<Vec<u8>, GitError> {
    run(dir, &["show", &format!("{rev}:{path}")])
}

/// The branch a fresh clone checks out: the remote's `HEAD` symbol, else
/// `main`, else `master`.
///
/// # Errors
/// Returns an error if none of the three can be found.
pub fn default_branch(dir: &Path) -> Result<String, GitError> {
    if let Ok(text) = run_text(
        dir,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    ) {
        if let Some(name) = text.trim().strip_prefix("origin/") {
            return Ok(name.to_string());
        }
    }
    for candidate in ["main", "master"] {
        if run(dir, &["rev-parse", "--verify", "--quiet", candidate]).is_ok() {
            return Ok(candidate.to_string());
        }
    }
    Err(GitError(
        "cannot find a default branch: no origin/HEAD, no main, no master".to_string(),
    ))
}

/// The `origin` remote, split into the host it lives on, the owner, and
/// the repository name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    pub host: String,
    pub owner: String,
    pub name: String,
}

/// The `origin` remote: `github.com`, `acme`, `repo` for any of
/// `https://github.com/acme/repo.git`, `ssh://git@github.com/acme/repo`,
/// or `git@github.com:acme/repo.git`.
///
/// # Errors
/// Returns an error if git cannot run in `dir`, there is no `origin`, or
/// its URL has no owner and repository segments.
pub fn remote(dir: &Path) -> Result<Remote, GitError> {
    let url = run_text(dir, &["remote", "get-url", "origin"])?;
    parse_remote(url.trim()).ok_or_else(|| {
        GitError(format!(
            "cannot read a host, an owner and a name from the origin URL '{}'",
            url.trim()
        ))
    })
}

/// A remote URL of any of the three common shapes, or `None` when it has
/// no host or fewer than two path segments.
fn parse_remote(url: &str) -> Option<Remote> {
    let (authority, path) = match url.find("://") {
        Some(i) => url.get(i + 3..)?.split_once('/')?,
        None => url.split_once(':')?,
    };
    let host = authority.rsplit('@').next()?.split(':').next()?;
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    let mut segments = path.rsplit('/');
    let name = segments.next().filter(|s| !s.is_empty())?;
    let owner = segments.next().filter(|s| !s.is_empty())?;
    if host.is_empty() {
        return None;
    }
    Some(Remote {
        host: host.to_string(),
        owner: owner.to_string(),
        name: name.to_string(),
    })
}

/// Turns a git-reported forward-slash path into a real path under `dir`.
#[must_use]
pub fn to_local_path(dir: &Path, git_path: &str) -> PathBuf {
    dir.join(git_path.replace('/', std::path::MAIN_SEPARATOR_STR))
}

/// Whether git's own wording for `stderr` says plainly that `dir` sits in
/// no git repository at all, as against any other failure such as a bare
/// repository's own "must be run in a work tree" answer.
fn says_no_repository_here(stderr: &str) -> bool {
    stderr.contains("not a git repository")
}

/// `dir`'s repository root, if it has one with a working tree.
///
/// `Ok(None)` when git ran cleanly and said plainly that `dir` is not
/// inside a git repository. `Err` for anything else: git could not start,
/// `dir` sits in a bare repository, or any other git failure.
///
/// # Errors
/// Returns an error for every failure except a plain "not a git
/// repository" answer.
pub fn repo_root_if_any(dir: &Path) -> Result<Option<PathBuf>, GitError> {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .args(["rev-parse", "--show-toplevel"]);
    scrub_git_env_for_dir(&mut command, dir);
    let output = command
        .output()
        .map_err(|e| GitError(format!("cannot run git: {e}")))?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        return Ok(Some(PathBuf::from(text.trim())));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if says_no_repository_here(&stderr) {
        return Ok(None);
    }
    Err(GitError(format!(
        "git rev-parse --show-toplevel failed: {}",
        stderr.trim()
    )))
}

/// The repository's working-tree root.
///
/// # Errors
/// Returns an error if git cannot run in `dir`, or `dir` is not inside a
/// git repository.
pub fn repo_root(dir: &Path) -> Result<PathBuf, GitError> {
    repo_root_if_any(dir)?.ok_or_else(|| GitError("not a git repository".to_string()))
}

/// `HEAD`'s short commit hash.
///
/// # Errors
/// Returns an error if git cannot run in `dir`.
pub fn head_short_sha(dir: &Path) -> Result<String, GitError> {
    run_text(dir, &["rev-parse", "--short", "HEAD"]).map(|t| t.trim().to_string())
}

/// True when `rev` resolves to a commit in `dir`; false, never an error,
/// when it does not.
#[must_use]
pub fn commit_exists(dir: &Path, rev: &str) -> bool {
    run(
        dir,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{commit}}"),
        ],
    )
    .is_ok()
}

/// Tracked paths whose working tree differs from the index right now: a
/// file staged for commit and then edited again shows up here too, so a
/// caller can tell the author that two versions of it were checked.
///
/// # Errors
/// Returns an error if git cannot run in `dir`.
pub fn unstaged_files(dir: &Path) -> Result<Vec<String>, GitError> {
    run(dir, &["diff", "--name-only", "-z"]).map(|raw| split_nul(&raw))
}

/// One key's value from `dir`'s own local git config, never the global or
/// system config. `Ok(None)` when the key is not set there at all.
///
/// # Errors
/// Returns an error only when git itself cannot run or fails for a reason
/// other than the key being unset.
pub fn config_get_local(dir: &Path, key: &str) -> Result<Option<String>, GitError> {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .args(["config", "--local", "--get", key]);
    scrub_git_env_for_dir(&mut command, dir);
    let output = command
        .output()
        .map_err(|e| GitError(format!("cannot run git: {e}")))?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        return Ok(Some(text.trim().to_string()));
    }
    if output.status.code() == Some(1) {
        return Ok(None);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(GitError(format!(
        "git config --get {key} failed: {}",
        stderr.trim()
    )))
}

/// Sets `key` to `value` in `dir`'s own local git config.
///
/// # Errors
/// Returns an error if git cannot run in `dir` or the write fails.
pub fn config_set_local(dir: &Path, key: &str, value: &str) -> Result<(), GitError> {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .args(["config", "--local", key, value]);
    scrub_git_env_for_dir(&mut command, dir);
    let output = command
        .output()
        .map_err(|e| GitError(format!("cannot run git: {e}")))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(GitError(format!(
        "git config {key} {value} failed: {}",
        stderr.trim()
    )))
}

/// Every path that differs between `rev` and the working tree plus index,
/// added, modified, deleted or renamed alike. Unlike [`changed_files`],
/// nothing is filtered out: a deleted path still names a changed path.
///
/// # Errors
/// Returns an error if git cannot run in `dir`, such as when `rev` does not resolve.
pub fn diff_name_only(dir: &Path, rev: &str) -> Result<Vec<String>, GitError> {
    run_text(dir, &["diff", "--name-only", rev]).map(|t| t.lines().map(str::to_string).collect())
}

/// One raw `git diff --numstat` line per changed path: added lines, then
/// deleted lines, then the path, tab-separated. A binary file reports `-`
/// for both counts.
///
/// # Errors
/// Returns an error if git cannot run in `dir`, such as when `rev` does not resolve.
pub fn diff_numstat(dir: &Path, rev: &str) -> Result<Vec<String>, GitError> {
    run_text(dir, &["diff", "--numstat", rev]).map(|t| t.lines().map(str::to_string).collect())
}

/// Every path git neither tracks nor ignores.
///
/// # Errors
/// Returns an error if git cannot run in `dir`.
pub fn untracked_files(dir: &Path) -> Result<Vec<String>, GitError> {
    run(dir, &["ls-files", "--others", "--exclude-standard", "-z"]).map(|raw| split_nul(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;
    use std::sync::Mutex;

    /// Serialises every test that sets `GIT_DIR`/`GIT_INDEX_FILE`: both are process-wide.
    static GIT_ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Runs `f` with `vars` applied for its duration (`None` means unset), restoring whatever each one held before.
    fn with_git_env<T>(vars: &[(&str, Option<&str>)], f: impl FnOnce() -> T) -> T {
        let guard = GIT_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ambient: Vec<(&str, Option<String>)> = vars
            .iter()
            .map(|(k, _)| (*k, std::env::var(k).ok()))
            .collect();
        for (k, v) in vars {
            match v {
                // SAFETY: serialised by GIT_ENV_LOCK.
                Some(v) => unsafe { std::env::set_var(k, v) },
                // SAFETY: serialised by GIT_ENV_LOCK.
                None => unsafe { std::env::remove_var(k) },
            }
        }
        let result = f();
        for (k, prior) in ambient {
            match prior {
                // SAFETY: serialised by GIT_ENV_LOCK.
                Some(v) => unsafe { std::env::set_var(k, v) },
                // SAFETY: serialised by GIT_ENV_LOCK.
                None => unsafe { std::env::remove_var(k) },
            }
        }
        drop(guard);
        result
    }

    #[test]
    fn the_three_common_remote_shapes_parse_the_same_way() {
        for url in [
            "https://github.com/acme/tools.git",
            "https://github.com/acme/tools",
            "ssh://git@github.com/acme/tools",
            "git@github.com:acme/tools.git",
        ] {
            let r = parse_remote(url).unwrap_or_else(|| panic!("{url}"));
            assert_eq!(r.host, "github.com", "{url}");
            assert_eq!(r.owner, "acme", "{url}");
            assert_eq!(r.name, "tools", "{url}");
        }
    }

    #[test]
    fn a_remote_on_another_host_keeps_its_host() {
        let r = parse_remote("git@gitlab.example.com:group/project.git").expect("parses");
        assert_eq!(r.host, "gitlab.example.com");
        assert_eq!(r.owner, "group");
    }

    #[test]
    fn a_url_with_too_few_segments_is_none() {
        assert!(parse_remote("https://github.com/tools").is_none());
        assert!(parse_remote("nonsense").is_none());
    }

    /// Runs `git init --quiet [--bare] dir`, GIT_* scrubbed first.
    fn init_repo(dir: &Path, bare: bool) {
        let mut command = Command::new("git");
        command.arg("init").arg("--quiet");
        if bare {
            command.arg("--bare");
        }
        command.arg(dir);
        scrub_git_env(&mut command);
        let out = command.output().expect("git init runs");
        assert!(
            out.status.success(),
            "git init failed for {}: {}",
            dir.display(),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A plain non-repository directory answers `Ok(None)`.
    /// An unset key reads back as `None`, never an error.
    #[test]
    fn config_get_local_reports_an_unset_key_as_none() {
        let dir = TempDir::new("osf-git-test-config-unset");
        init_repo(&dir, false);
        assert_eq!(
            config_get_local(&dir, "core.hooksPath").expect("git runs"),
            None
        );
    }

    /// A value set with `config_set_local` reads back through `config_get_local`.
    #[test]
    fn config_set_local_is_read_back_by_config_get_local() {
        let dir = TempDir::new("osf-git-test-config-roundtrip");
        init_repo(&dir, false);
        config_set_local(&dir, "core.hooksPath", "/somewhere/githooks").expect("set");
        assert_eq!(
            config_get_local(&dir, "core.hooksPath").expect("git runs"),
            Some("/somewhere/githooks".to_string())
        );
    }

    #[test]
    fn a_plain_directory_has_no_repo_root() {
        let dir = TempDir::new("osf-git-test-no-repo");
        assert_eq!(repo_root_if_any(&dir).expect("git runs"), None);
    }

    /// A normal repository answers `Ok(Some(root))`.
    #[test]
    fn a_normal_repository_has_a_repo_root() {
        let dir = TempDir::new("osf-git-test-normal-repo");
        init_repo(&dir, false);
        let root = repo_root_if_any(&dir)
            .expect("git runs")
            .expect("a repository root");
        assert_eq!(
            std::fs::canonicalize(&root).expect("root canonicalises"),
            std::fs::canonicalize(&dir).expect("dir canonicalises")
        );
    }

    /// A bare repository is an error, not a plain "no repository" answer:
    /// its own "must be run in a work tree" wording differs from git's
    /// "not a git repository" wording, though both exit the same way.
    #[test]
    fn a_bare_repository_is_an_error_not_a_plain_absence() {
        let dir = TempDir::new("osf-git-test-bare-repo");
        init_repo(&dir, true);
        let err = repo_root_if_any(&dir).expect_err("a bare repository is an error");
        assert!(!err.to_string().contains("not a git repository"), "{err}");
    }

    /// Every locating variable is removed, regardless of what is inherited.
    #[test]
    fn scrub_removes_the_locating_variables_unconditionally() {
        with_git_env(&[("GIT_DIR", None), ("GIT_INDEX_FILE", None)], || {
            let dir = TempDir::new("osf-git-test-scrub-locating");
            let mut command = Command::new("git");
            scrub_git_env_for_dir(&mut command, &dir);
            let removed: Vec<String> = command
                .get_envs()
                .filter(|(_, v)| v.is_none())
                .map(|(k, _)| k.to_string_lossy().into_owned())
                .collect();
            for var in LOCATING_VARS {
                assert!(removed.iter().any(|r| r == var), "{var} not removed");
            }
        });
    }

    /// `GIT_INDEX_FILE` is left alone when its own path lies inside `dir`'s own git directory.
    #[test]
    fn git_index_file_survives_when_it_lies_inside_dirs_own_git_dir() {
        let dir = TempDir::new("osf-git-test-index-inside");
        init_repo(&dir, false);
        let index_path = dir.join(".git").join("fake-index");
        std::fs::write(&index_path, b"stand-in for an index").expect("fake index writes");
        with_git_env(
            &[(
                "GIT_INDEX_FILE",
                Some(index_path.to_str().expect("utf8 path")),
            )],
            || {
                let mut command = Command::new("git");
                scrub_git_env_for_dir(&mut command, &dir);
                let touched = command
                    .get_envs()
                    .any(|(k, _)| k == std::ffi::OsStr::new("GIT_INDEX_FILE"));
                assert!(!touched, "GIT_INDEX_FILE should have been left alone");
            },
        );
    }

    /// `GIT_INDEX_FILE` is removed when its own path lies outside `dir`'s
    /// own git directory: the two-repository shape the reviewer reproduced.
    #[test]
    fn git_index_file_is_removed_when_it_lies_outside_dirs_own_git_dir() {
        let other = TempDir::new("osf-git-test-index-outside-other");
        init_repo(&other, false);
        let other_index = other.join(".git").join("fake-index");
        std::fs::write(&other_index, b"stand-in for another repository's index")
            .expect("fake index writes");
        let dir = TempDir::new("osf-git-test-index-outside-target");
        init_repo(&dir, false);
        with_git_env(
            &[(
                "GIT_INDEX_FILE",
                Some(other_index.to_str().expect("utf8 path")),
            )],
            || {
                let mut command = Command::new("git");
                scrub_git_env_for_dir(&mut command, &dir);
                let removed = command
                    .get_envs()
                    .any(|(k, v)| k == std::ffi::OsStr::new("GIT_INDEX_FILE") && v.is_none());
                assert!(removed, "GIT_INDEX_FILE should have been removed");
            },
        );
    }

    /// `GIT_INDEX_FILE` is removed when `dir` has no repository of its own:
    /// there is nothing to prove containment against.
    #[test]
    fn git_index_file_is_removed_when_dir_has_no_repository_of_its_own() {
        let source = TempDir::new("osf-git-test-index-no-dir-repo-source");
        init_repo(&source, false);
        let source_index = source.join(".git").join("fake-index");
        std::fs::write(&source_index, b"stand-in for an index").expect("fake index writes");
        let dir = TempDir::new("osf-git-test-index-no-dir-repo-target");
        with_git_env(
            &[(
                "GIT_INDEX_FILE",
                Some(source_index.to_str().expect("utf8 path")),
            )],
            || {
                let mut command = Command::new("git");
                scrub_git_env_for_dir(&mut command, &dir);
                let removed = command
                    .get_envs()
                    .any(|(k, v)| k == std::ffi::OsStr::new("GIT_INDEX_FILE") && v.is_none());
                assert!(removed, "GIT_INDEX_FILE should have been removed");
            },
        );
    }

    /// No `GIT_INDEX_FILE` inherited at all: nothing to touch.
    #[test]
    fn no_git_index_file_is_left_untouched() {
        let dir = TempDir::new("osf-git-test-no-index-file");
        init_repo(&dir, false);
        with_git_env(&[("GIT_INDEX_FILE", None)], || {
            let mut command = Command::new("git");
            scrub_git_env_for_dir(&mut command, &dir);
            let touched = command
                .get_envs()
                .any(|(k, _)| k == std::ffi::OsStr::new("GIT_INDEX_FILE"));
            assert!(!touched, "nothing named GIT_INDEX_FILE should be touched");
        });
    }

    /// The reviewer's exact two-repository reproduction: `GIT_INDEX_FILE`
    /// inherited from repo A's real index, no `GIT_DIR` at all, and a git
    /// call made for repo B. Before this fix this failed outright
    /// ("unable to read <sha>") or, with coincidental blobs, silently
    /// scanned the wrong content. `staged_files` must now read repo B's own
    /// index.
    /// Stages `path` in `dir`, with the real stderr in the panic message on
    /// failure: `.status()` alone throws it away, leaving a bare "false"
    /// with no clue why.
    fn stage_or_panic(dir: &Path, path: &str) {
        let mut command = Command::new("git");
        command.current_dir(dir).args(["add", path]);
        scrub_git_env(&mut command);
        let out = command.output().expect("git add runs");
        assert!(
            out.status.success(),
            "git add {path} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn a_foreign_git_index_file_no_longer_lets_a_call_for_another_folder_read_it() {
        // The whole body runs under one lock, not just the final assertion:
        // `git add` below reads the ambient environment at spawn time same
        // as any other git call, so it is just as exposed as `staged_files`
        // to another test's `with_git_env` setting GIT_INDEX_FILE mid-flight
        // if it is left unguarded.
        with_git_env(&[("GIT_DIR", None), ("GIT_INDEX_FILE", None)], || {
            let repo_a = TempDir::new("osf-git-test-two-repo-a");
            init_repo(&repo_a, false);
            std::fs::write(repo_a.join("a.txt"), b"a").expect("a.txt writes");
            stage_or_panic(&repo_a, "a.txt");

            let repo_b = TempDir::new("osf-git-test-two-repo-b");
            init_repo(&repo_b, false);
            std::fs::write(repo_b.join("b.txt"), b"b").expect("b.txt writes");
            stage_or_panic(&repo_b, "b.txt");

            let repo_a_index = std::fs::canonicalize(repo_a.join(".git").join("index"))
                .expect("repo a's index canonicalises");
            // SAFETY: serialised by GIT_ENV_LOCK, held by the enclosing with_git_env call.
            unsafe { std::env::set_var("GIT_INDEX_FILE", &repo_a_index) };
            let files = staged_files(&repo_b).expect("staged_files reads repo b's own index");
            assert_eq!(files, vec!["b.txt".to_string()]);
        });
    }
}
