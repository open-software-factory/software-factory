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

/// The best common ancestor of `a` and `b`: the point they diverged from.
///
/// # Errors
/// Returns an error if git cannot run in `dir`, or `a` and `b` share no
/// common ancestor. Git reports that case with an empty stderr, so the
/// message here names the likely cause instead of repeating nothing.
pub fn merge_base(dir: &Path, a: &str, b: &str) -> Result<String, GitError> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(["merge-base", a, b])
        .output()
        .map_err(|e| GitError(format!("cannot run git: {e}")))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.trim().is_empty() {
        return Err(GitError(format!(
            "no common ancestor between {a} and {b}; fetch full history (for example a shallow clone)"
        )));
    }
    Err(GitError(format!(
        "git merge-base {a} {b} failed: {}",
        stderr.trim()
    )))
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

/// How one path changed between `base` and `head`. A rename is its own
/// case, carrying both paths, rather than a deletion paired with an
/// unrelated addition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangedPath {
    Added(String),
    Modified(String),
    Deleted(String),
    Renamed { from: String, to: String },
}

impl ChangedPath {
    /// The path to read at `base`, or `None` when the change added it.
    #[must_use]
    pub fn base_path(&self) -> Option<&str> {
        match self {
            ChangedPath::Added(_) => None,
            ChangedPath::Modified(p) | ChangedPath::Deleted(p) => Some(p),
            ChangedPath::Renamed { from, .. } => Some(from),
        }
    }

    /// The path to read at `head`, or `None` when the change deleted it.
    #[must_use]
    pub fn head_path(&self) -> Option<&str> {
        match self {
            ChangedPath::Deleted(_) => None,
            ChangedPath::Added(p) | ChangedPath::Modified(p) => Some(p),
            ChangedPath::Renamed { to, .. } => Some(to),
        }
    }

    /// The path this change is best named by: `head`'s, when there is one,
    /// else `base`'s, so a plain deletion still names something.
    #[must_use]
    pub fn display_path(&self) -> &str {
        self.head_path().or_else(|| self.base_path()).unwrap_or("")
    }
}

/// Every path that differs between `base` and `head`, at their merge base:
/// the change this range actually introduces, not whatever `base` itself
/// has gained or lost on its own since the two diverged. A renamed path is
/// reported once, as [`ChangedPath::Renamed`], carrying both its old and
/// new name, rather than as a deletion plus an unrelated addition.
///
/// # Errors
/// Returns an error if git cannot run in `dir`, such as when `base` or
/// `head` does not resolve.
pub fn diff_name_status_between(
    dir: &Path,
    base: &str,
    head: &str,
) -> Result<Vec<ChangedPath>, GitError> {
    let range = format!("{base}...{head}");
    let raw = run(dir, &["diff", "--name-status", "-M", "-z", &range])?;
    let mut tokens = split_nul(&raw).into_iter();
    let mut out = Vec::new();
    while let Some(status) = tokens.next() {
        if let Some(change) = changed_path_from_status(&status, &mut tokens) {
            out.push(change);
        }
    }
    Ok(out)
}

/// Builds one [`ChangedPath`] from a `git diff --name-status -M -z` status
/// token (such as `M` or `R100`) and the path token(s) that follow it in
/// the same NUL-separated stream: one for every status but a rename or a
/// copy, which carry two. A type change (`T`, such as a file replaced by a
/// symlink at the same path) is treated like `M`: the path is compared at
/// both revisions, so content that stops being parseable there is reported
/// as unparsed rather than silently dropped. `None` for a status this
/// build has no case for, so one unrecognised entry never stops the rest
/// being read.
fn changed_path_from_status(
    status: &str,
    tokens: &mut impl Iterator<Item = String>,
) -> Option<ChangedPath> {
    match status.as_bytes().first()? {
        b'A' => Some(ChangedPath::Added(tokens.next()?)),
        b'M' | b'T' => Some(ChangedPath::Modified(tokens.next()?)),
        b'D' => Some(ChangedPath::Deleted(tokens.next()?)),
        b'R' => Some(ChangedPath::Renamed {
            from: tokens.next()?,
            to: tokens.next()?,
        }),
        b'C' => {
            tokens.next()?;
            Some(ChangedPath::Added(tokens.next()?))
        }
        _ => None,
    }
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

/// Whether `path` exists in the tree at `rev`.
///
/// # Errors
/// Returns an error if git cannot run, or `rev` does not resolve. A path
/// that is simply not there at `rev` is `Ok(false)`, never an error, so a
/// caller can tell that apart from a read that failed for some other
/// reason.
pub fn path_exists_at(dir: &Path, rev: &str, path: &str) -> Result<bool, GitError> {
    let output = run(dir, &["ls-tree", rev, "--", path])?;
    Ok(!output.is_empty())
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

/// The repository's working-tree root.
///
/// # Errors
/// Returns an error if git cannot run in `dir`.
pub fn repo_root(dir: &Path) -> Result<PathBuf, GitError> {
    let text = run_text(dir, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(text.trim()))
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

    #[test]
    fn status_tokens_parse_into_their_matching_case() {
        let parse = |status: &str, paths: &[&str]| {
            let mut tokens = paths.iter().map(|s| (*s).to_string());
            changed_path_from_status(status, &mut tokens)
        };
        assert_eq!(
            parse("A", &["new.rs"]),
            Some(ChangedPath::Added("new.rs".to_string()))
        );
        assert_eq!(
            parse("M", &["thing.rs"]),
            Some(ChangedPath::Modified("thing.rs".to_string()))
        );
        assert_eq!(
            parse("D", &["old.rs"]),
            Some(ChangedPath::Deleted("old.rs".to_string()))
        );
        assert_eq!(
            parse("R100", &["old.rs", "new.rs"]),
            Some(ChangedPath::Renamed {
                from: "old.rs".to_string(),
                to: "new.rs".to_string(),
            })
        );
        assert_eq!(
            parse("C100", &["src.rs", "copy.rs"]),
            Some(ChangedPath::Added("copy.rs".to_string()))
        );
    }

    #[test]
    fn a_type_change_status_parses_like_modified() {
        let mut tokens = std::iter::once("thing.rs".to_string());
        assert_eq!(
            changed_path_from_status("T", &mut tokens),
            Some(ChangedPath::Modified("thing.rs".to_string()))
        );
    }

    /// A throwaway git repository for a test that needs real git plumbing,
    /// not just the pure parsing functions above.
    fn test_repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("osf-git-tests-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp repo dir creates");
        dir
    }

    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn merge_base_with_no_common_ancestor_names_both_refs() {
        let dir = test_repo("merge-base-no-common-ancestor");
        git(&dir, &["init", "-q", "-b", "a"]);
        git(&dir, &["config", "user.email", "test@example.com"]);
        git(&dir, &["config", "user.name", "Test"]);
        std::fs::write(dir.join("f.txt"), "a").expect("fixture file writes");
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", "a"]);
        git(&dir, &["checkout", "-q", "--orphan", "b"]);
        git(&dir, &["rm", "-rf", "-q", "."]);
        std::fs::write(dir.join("g.txt"), "b").expect("fixture file writes");
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", "b"]);

        let message = merge_base(&dir, "a", "b")
            .expect_err("a and b share no common ancestor")
            .to_string();
        assert!(
            message.contains("no common ancestor between a and b"),
            "{message}"
        );
        assert!(message.contains("fetch full history"), "{message}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_exists_at_tells_a_missing_path_from_an_unresolved_rev() {
        let dir = test_repo("path-exists-at");
        git(&dir, &["init", "-q", "-b", "main"]);
        git(&dir, &["config", "user.email", "test@example.com"]);
        git(&dir, &["config", "user.name", "Test"]);
        std::fs::write(dir.join("a.txt"), "a").expect("fixture file writes");
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", "base"]);

        assert!(path_exists_at(&dir, "HEAD", "a.txt").expect("HEAD resolves"));
        assert!(!path_exists_at(&dir, "HEAD", "missing.txt").expect("HEAD resolves"));
        assert!(path_exists_at(&dir, "not-a-rev", "a.txt").is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn diff_name_status_between_reads_a_non_ascii_renamed_path() {
        let dir = test_repo("non-ascii-rename");
        git(&dir, &["init", "-q", "-b", "main"]);
        git(&dir, &["config", "user.email", "test@example.com"]);
        git(&dir, &["config", "user.name", "Test"]);
        std::fs::write(dir.join("old.rs"), "fn x() {}\n").expect("fixture file writes");
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", "base"]);
        let base = run_text(&dir, &["rev-parse", "HEAD"])
            .expect("rev-parse runs")
            .trim()
            .to_string();

        git(&dir, &["mv", "old.rs", "café.rs"]);
        git(&dir, &["commit", "-q", "-m", "rename"]);

        let changes =
            diff_name_status_between(&dir, &base, "HEAD").expect("diff reports the rename");
        assert_eq!(
            changes,
            vec![ChangedPath::Renamed {
                from: "old.rs".to_string(),
                to: "café.rs".to_string(),
            }]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn display_path_prefers_head_and_falls_back_to_base() {
        assert_eq!(
            ChangedPath::Added("a.rs".to_string()).display_path(),
            "a.rs"
        );
        assert_eq!(
            ChangedPath::Deleted("a.rs".to_string()).display_path(),
            "a.rs"
        );
        assert_eq!(
            ChangedPath::Renamed {
                from: "old.rs".to_string(),
                to: "new.rs".to_string()
            }
            .display_path(),
            "new.rs"
        );
    }
}
