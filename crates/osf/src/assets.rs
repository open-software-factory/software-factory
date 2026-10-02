//! `osf assets publish`: push a folder of built files to a data branch in a
//! temporary clone, creating the branch as an orphan the first time, and
//! print the raw content web address for what landed.
//!
//! Mirrors what `pr-lens-publish.sh` did for pr-lens's own rendered
//! diagrams: one push, retried a few times if another run's push lands
//! first, and a push skipped entirely when the content already matches
//! what is there. The token this needs is never written to a remote URL,
//! a git config file, or disk; it is passed to the one `git push` that
//! needs it, through an HTTP extra header, and nowhere else.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A publish could not be planned, or ran out of attempts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetsError(String);

impl AssetsError {
    fn new(message: impl Into<String>) -> Self {
        AssetsError(message.into())
    }
}

impl fmt::Display for AssetsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AssetsError {}

/// Everything [`publish`] needs. Reading files and running `git` is
/// [`publish`]'s own job; this only carries the inputs, so a test can build
/// one without touching the environment.
pub struct PublishArgs<'a> {
    /// Where to clone from and push to: a real GitHub URL in production, or
    /// a local bare repository in a test.
    pub remote: &'a str,
    /// The repository as `owner/name`, used only to build the printed raw
    /// content address.
    pub repo: &'a str,
    pub branch: &'a str,
    /// The path prefix within `branch` that the folder's files land under.
    pub path: &'a str,
    /// The local folder whose files (not subfolders) are published.
    pub dir: &'a Path,
    /// The token to authenticate the push with. `None` pushes
    /// unauthenticated, which only a non-GitHub test remote can accept.
    pub token: Option<&'a str>,
    /// How many times to retry the whole clone-copy-commit-push cycle when
    /// the push loses a race with another run.
    pub max_attempts: u32,
}

/// The raw content web address for `path` on `branch` of `repo`.
#[must_use]
pub fn raw_url(repo: &str, branch: &str, path: &str) -> String {
    format!("https://raw.githubusercontent.com/{repo}/{branch}/{path}")
}

const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The base64 alphabet character for a 6-bit `index`. `index` is always
/// built from a shift or mask below six bits wide, so the fallback never
/// actually triggers; it exists so this never indexes the table directly.
fn table_char(index: u8) -> char {
    char::from(*TABLE.get(usize::from(index)).unwrap_or(&b'A'))
}

/// A standard base64 encoding (RFC 4648) of `input`. Written out by hand so
/// the one `git push` that needs to send an HTTP `Authorization` header
/// does not pull in a dependency for it.
#[must_use]
fn base64_encode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = *chunk.first().unwrap_or(&0);
        let b1 = chunk.get(1).copied();
        let b2 = chunk.get(2).copied();
        out.push(table_char(b0 >> 2));
        out.push(table_char(((b0 & 0x03) << 4) | (b1.unwrap_or(0) >> 4)));
        match b1 {
            Some(b1) => out.push(table_char(((b1 & 0x0f) << 2) | (b2.unwrap_or(0) >> 6))),
            None => out.push('='),
        }
        match b2 {
            Some(b2) => out.push(table_char(b2 & 0x3f)),
            None => out.push('='),
        }
    }
    out
}

/// What to do once a push attempt has landed, failed, or raced. Pure: it
/// takes the outcome as data and never runs a command itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushDecision {
    Done,
    Retry,
    GiveUp,
}

/// Decides the next step after one push attempt, on attempt number `attempt`
/// of `max_attempts`. A successful push is always done. A failed push tries
/// again unless this was already the last attempt allowed, matching
/// `pr-lens-publish.sh`'s blind retry: any push failure is treated as
/// another run's push having landed first, not diagnosed further.
#[must_use]
pub fn decide_after_push(success: bool, attempt: u32, max_attempts: u32) -> PushDecision {
    if success {
        PushDecision::Done
    } else if attempt < max_attempts {
        PushDecision::Retry
    } else {
        PushDecision::GiveUp
    }
}

/// A temporary directory, removed when it goes out of scope, the way a
/// temporary clone is meant to be.
struct Workspace(PathBuf);

impl Workspace {
    fn fresh(unique: &str) -> Result<Self, AssetsError> {
        let dir = std::env::temp_dir().join(format!("osf-assets-publish-{unique}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)
            .map_err(|e| AssetsError::new(format!("cannot create a workspace: {e}")))?;
        Ok(Workspace(dir))
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run_git(dir: &Path, args: &[&str]) -> Result<Output, AssetsError> {
    Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| AssetsError::new(format!("cannot run git {}: {e}", args.join(" "))))
}

fn require_ok(output: &Output, what: &str) -> Result<(), AssetsError> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(AssetsError::new(format!(
        "{what} failed: {}",
        stderr.trim()
    )))
}

/// Sets up the workspace as a git repository tracking `remote`, on
/// `branch`: checked out from the branch's current tip when it already
/// exists there, or started fresh as an orphan when it does not.
fn init_and_checkout(dir: &Path, remote: &str, branch: &str) -> Result<(), AssetsError> {
    require_ok(&run_git(dir, &["init", "--quiet"])?, "git init")?;
    require_ok(
        &run_git(dir, &["config", "user.name", "osf"])?,
        "git config user.name",
    )?;
    require_ok(
        &run_git(
            dir,
            &["config", "user.email", "osf@users.noreply.github.com"],
        )?,
        "git config user.email",
    )?;
    require_ok(
        &run_git(dir, &["remote", "add", "origin", remote])?,
        "git remote add",
    )?;
    let fetched = run_git(dir, &["fetch", "--quiet", "--depth=1", "origin", branch])?;
    if fetched.status.success() {
        require_ok(
            &run_git(dir, &["checkout", "--quiet", "-b", branch, "FETCH_HEAD"])?,
            "git checkout",
        )
    } else {
        require_ok(
            &run_git(dir, &["checkout", "--quiet", "--orphan", branch])?,
            "git checkout --orphan",
        )
    }
}

/// Copies every regular file directly inside `source_dir` (not its
/// subfolders) into `<workspace>/<path>`, creating that folder if it is not
/// already there from a previous publish.
fn copy_files(workspace: &Path, path: &str, source_dir: &Path) -> Result<(), AssetsError> {
    let target = workspace.join(path);
    std::fs::create_dir_all(&target)
        .map_err(|e| AssetsError::new(format!("cannot create {}: {e}", target.display())))?;
    let entries = std::fs::read_dir(source_dir)
        .map_err(|e| AssetsError::new(format!("cannot read {}: {e}", source_dir.display())))?;
    for entry in entries {
        let entry =
            entry.map_err(|e| AssetsError::new(format!("cannot read a directory entry: {e}")))?;
        let file_type = entry
            .file_type()
            .map_err(|e| AssetsError::new(format!("cannot read a file type: {e}")))?;
        if !file_type.is_file() {
            continue;
        }
        let dest = target.join(entry.file_name());
        std::fs::copy(entry.path(), &dest).map_err(|e| {
            AssetsError::new(format!(
                "cannot copy {} to {}: {e}",
                entry.path().display(),
                dest.display()
            ))
        })?;
    }
    Ok(())
}

fn stage(dir: &Path, path: &str) -> Result<(), AssetsError> {
    require_ok(&run_git(dir, &["add", path])?, "git add")
}

/// Whether the index holds any staged change against `HEAD`. `Ok(false)`
/// when nothing changed (this publish's content already matches what is on
/// the branch), never an error: a clean diff is a normal outcome, not a
/// failure of `git diff`.
fn has_staged_changes(dir: &Path) -> Result<bool, AssetsError> {
    let output = run_git(dir, &["diff", "--quiet", "--cached"])?;
    match output.status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(AssetsError::new(format!(
                "git diff --cached failed: {}",
                stderr.trim()
            )))
        }
    }
}

fn commit(dir: &Path, path: &str) -> Result<(), AssetsError> {
    require_ok(
        &run_git(
            dir,
            &["commit", "--quiet", "-m", &format!("publish {path}")],
        )?,
        "git commit",
    )
}

/// Pushes `branch` to `origin`. The token, when there is one, is passed
/// only as an `http.extraheader` on this one command: never folded into the
/// remote URL, never written to `.git/config`, never touching disk.
fn push(dir: &Path, branch: &str, token: Option<&str>) -> Result<bool, AssetsError> {
    let mut args: Vec<String> = Vec::new();
    let header;
    if let Some(token) = token {
        let credential = base64_encode(&format!("x-access-token:{token}"));
        header = format!("http.extraheader=AUTHORIZATION: basic {credential}");
        args.push("-c".to_string());
        args.push(header);
    }
    args.push("push".to_string());
    args.push("--quiet".to_string());
    args.push("origin".to_string());
    args.push(branch.to_string());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = run_git(dir, &refs)?;
    Ok(output.status.success())
}

/// Publishes `args.dir`'s files to `args.path` on `args.branch`, retrying
/// the whole cycle up to `args.max_attempts` times when the push loses a
/// race, and prints nothing itself: the caller decides what to do with the
/// returned address.
///
/// # Errors
/// Returns an error when a git or filesystem step fails outright, or when
/// every push attempt is lost to a race.
pub fn publish(args: &PublishArgs) -> Result<String, AssetsError> {
    for attempt in 1..=args.max_attempts {
        let workspace = Workspace::fresh(&format!("{}-{attempt}", std::process::id()))?;
        init_and_checkout(&workspace.0, args.remote, args.branch)?;
        copy_files(&workspace.0, args.path, args.dir)?;
        stage(&workspace.0, args.path)?;
        if !has_staged_changes(&workspace.0)? {
            return Ok(raw_url(args.repo, args.branch, args.path));
        }
        commit(&workspace.0, args.path)?;
        let success = push(&workspace.0, args.branch, args.token)?;
        match decide_after_push(success, attempt, args.max_attempts) {
            PushDecision::Done => return Ok(raw_url(args.repo, args.branch, args.path)),
            PushDecision::Retry => {}
            PushDecision::GiveUp => {
                return Err(AssetsError::new(format!(
                    "could not publish {}: {} kept moving under {} attempt(s)",
                    args.path, args.branch, args.max_attempts
                )))
            }
        }
    }
    Err(AssetsError::new(format!(
        "could not publish {}: no attempt ran",
        args.path
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_encode_matches_the_rfc_4648_test_vectors() {
        assert_eq!(base64_encode(""), "");
        assert_eq!(base64_encode("f"), "Zg==");
        assert_eq!(base64_encode("fo"), "Zm8=");
        assert_eq!(base64_encode("foo"), "Zm9v");
        assert_eq!(base64_encode("foob"), "Zm9vYg==");
        assert_eq!(base64_encode("fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode("foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_encode_of_a_github_token_credential_decodes_the_way_github_expects() {
        // x-access-token:abc123 is the exact shape a push credential takes.
        assert_eq!(
            base64_encode("x-access-token:abc123"),
            "eC1hY2Nlc3MtdG9rZW46YWJjMTIz"
        );
    }

    #[test]
    fn a_successful_push_is_always_done() {
        assert_eq!(decide_after_push(true, 1, 5), PushDecision::Done);
        assert_eq!(decide_after_push(true, 5, 5), PushDecision::Done);
    }

    #[test]
    fn a_failed_push_retries_before_the_last_attempt() {
        assert_eq!(decide_after_push(false, 1, 5), PushDecision::Retry);
        assert_eq!(decide_after_push(false, 4, 5), PushDecision::Retry);
    }

    #[test]
    fn a_failed_push_on_the_last_attempt_gives_up() {
        assert_eq!(decide_after_push(false, 5, 5), PushDecision::GiveUp);
    }

    #[test]
    fn a_single_attempt_budget_gives_up_immediately_on_failure() {
        assert_eq!(decide_after_push(false, 1, 1), PushDecision::GiveUp);
    }

    #[test]
    fn raw_url_builds_the_expected_address() {
        assert_eq!(
            raw_url("acme/example", "data", "pr/9/deadbeef"),
            "https://raw.githubusercontent.com/acme/example/data/pr/9/deadbeef"
        );
    }
}
