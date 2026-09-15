//! Minimal git plumbing: shells out to the `git` binary the same way a
//! person would, so `osf scan` sees exactly what git sees.
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

/// Turns a git-reported forward-slash path into a real path under `dir`.
#[must_use]
pub fn to_local_path(dir: &Path, git_path: &str) -> PathBuf {
    dir.join(git_path.replace('/', std::path::MAIN_SEPARATOR_STR))
}
