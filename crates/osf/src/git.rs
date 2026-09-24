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

fn run(dir: &Path, args: &[&str]) -> Result<Vec<u8>, GitError> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
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
    scrub_git_env(&mut command);
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

    /// A fresh directory outside any git repository.
    fn outside_any_repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        dir
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
        let status = command.status().expect("git init runs");
        assert!(status.success(), "git init failed for {}", dir.display());
    }

    /// A plain non-repository directory answers `Ok(None)`.
    #[test]
    fn a_plain_directory_has_no_repo_root() {
        let dir = outside_any_repo("osf-git-test-no-repo");
        assert_eq!(repo_root_if_any(&dir).expect("git runs"), None);
    }

    /// A normal repository answers `Ok(Some(root))`.
    #[test]
    fn a_normal_repository_has_a_repo_root() {
        let dir = outside_any_repo("osf-git-test-normal-repo");
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
        let dir = outside_any_repo("osf-git-test-bare-repo");
        init_repo(&dir, true);
        let err = repo_root_if_any(&dir).expect_err("a bare repository is an error");
        assert!(!err.to_string().contains("not a git repository"), "{err}");
    }
}
