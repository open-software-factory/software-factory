//! Shared test plumbing: a throwaway git repository for `scan` and
//! `verify` integration tests, so neither reimplements it.
//! Each integration test binary compiles this module on its own; a helper
//! only one of them calls is not dead code, just unused in this one.
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Command;

/// A throwaway git repository under the system temp directory, named
/// uniquely so parallel tests never collide. Removed on drop.
pub struct TempRepo {
    pub dir: PathBuf,
}

impl TempRepo {
    pub fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("osf-verify-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp repo dir creates");
        let repo = TempRepo { dir };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "test@example.com"]);
        repo.git(&["config", "user.name", "Test"]);
        repo
    }

    pub fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(&self.dir)
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    pub fn write(&self, path: &str, content: &str) {
        let full = self.dir.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("fixture parent dir creates");
        }
        std::fs::write(&full, content).expect("fixture file writes");
    }

    pub fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
        self.git(&["rev-parse", "HEAD"]).trim().to_string()
    }

    /// Stages a path with `git add`, without committing it.
    pub fn stage(&self, path: &str) {
        self.git(&["add", path]);
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A fresh, empty directory to stand in for `HOME`, so a spawned `osf`
/// never picks up this machine's real `~/.osf/config.toml`.
pub fn isolated_home(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("osf-verify-test-home-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("isolated home dir creates");
    dir
}

/// Runs the compiled `osf` binary in `dir`, with `home` standing in for
/// `HOME`/`USERPROFILE` so it never reads this machine's real config.
pub fn run_osf(
    dir: &std::path::Path,
    home: &std::path::Path,
    args: &[&str],
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_osf"))
        .current_dir(dir)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env_remove("OSF_CONFIG")
        .env_remove("OSF_DENYLIST")
        .args(args)
        .output()
        .expect("osf runs")
}

/// Builds a session-link-shaped string at run time. A scan rule fixture
/// needs the real shape to prove the rule fires, but this repository's own
/// `osf scan` run has no exclusion for a test file any more, so the shape
/// must never sit in this file's source text as one contiguous literal.
pub fn session_link(id: &str) -> String {
    let host = "claude.ai";
    let path_prefix = "code/session_";
    format!("https://{host}/{path_prefix}{id}")
}

/// Builds a Windows user-path-shaped string at run time, for the same
/// reason as [`session_link`].
pub fn windows_user_path(user: &str) -> String {
    let drive = "D:";
    let marker = r"\Users\";
    format!(r"{drive}{marker}{user}\work\notes.md")
}

/// Builds a home-directory-path-shaped string at run time, for the same
/// reason as [`session_link`].
pub fn home_path(user: &str) -> String {
    let prefix = "/home";
    format!("{prefix}/{user}/work/notes.md")
}

/// Builds a co-author trailer line at run time, for the same reason as
/// [`session_link`].
pub fn coauthor_trailer(name: &str, email: &str) -> String {
    let label = ["Co", "Authored", "By"].join("-");
    format!("{label}: {name} <{email}>")
}

/// Builds a foreign cross-repository reference at run time, for the same
/// reason as [`session_link`]: once `osf.toml` sets a real project owner,
/// any other owner named next to a `#<digits>` reference reads as foreign.
pub fn foreign_reference(owner: &str, repo: &str, number: u32) -> String {
    format!("{owner}/{repo}#{number}")
}
