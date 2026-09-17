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

/// The owner segment of the `origin` remote: `acme` for
/// `https://host/acme/repo.git`, `ssh://git@host/acme/repo`, or
/// `git@host:acme/repo.git`.
///
/// # Errors
/// Returns an error if git cannot run in `dir`, there is no `origin`, or
/// its URL has no owner and repository segments.
pub fn remote_owner(dir: &Path) -> Result<String, GitError> {
    let url = run_text(dir, &["remote", "get-url", "origin"])?;
    parse_remote_owner(url.trim()).ok_or_else(|| {
        GitError(format!(
            "cannot read an owner from the origin URL '{}'",
            url.trim()
        ))
    })
}

/// The owner in a remote URL of any of the three common shapes, or `None`
/// when the URL has fewer than two path segments.
fn parse_remote_owner(url: &str) -> Option<String> {
    let path = match url.find("://") {
        Some(i) => url.get(i + 3..)?.split_once('/')?.1,
        None => url.split_once(':').map_or(url, |(_, p)| p),
    };
    let path = path.trim_end_matches('/').trim_end_matches(".git");
    let mut segments = path.rsplit('/');
    let _repo = segments.next().filter(|s| !s.is_empty())?;
    let owner = segments.next().filter(|s| !s.is_empty())?;
    Some(owner.to_string())
}

/// Turns a git-reported forward-slash path into a real path under `dir`.
#[must_use]
pub fn to_local_path(dir: &Path, git_path: &str) -> PathBuf {
    dir.join(git_path.replace('/', std::path::MAIN_SEPARATOR_STR))
}
