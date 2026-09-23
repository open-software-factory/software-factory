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

    /// Points a remote-tracking `origin/main` at the current `HEAD`, the
    /// way a fresh clone of a real remote would already have one, without
    /// this repository needing an actual remote to fetch from.
    pub fn track_origin_main(&self) {
        self.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    }

    /// Writes `count` numbered lines to `path`, each newline-terminated,
    /// creating any parent directory it needs.
    pub fn write_lines(&self, path: &str, count: usize) {
        use std::fmt::Write as _;
        let mut content = String::new();
        for n in 1..=count {
            writeln!(content, "line {n}").expect("writing to a string never fails");
        }
        self.write(path, &content);
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
    run_osf_with_env(dir, home, &[], args)
}

/// The same as [`run_osf`], with extra environment variables set for this
/// one run, such as `OSF_FILES_FROM` or `OSF_BASE`.
pub fn run_osf_with_env(
    dir: &std::path::Path,
    home: &std::path::Path,
    env: &[(&str, &str)],
    args: &[&str],
) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_osf"));
    cmd.current_dir(dir)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env_remove("OSF_CONFIG")
        .env_remove("OSF_DENYLIST")
        .args(args);
    for (key, value) in env {
        cmd.env(key, value);
    }
    cmd.output().expect("osf runs")
}

/// Builds a session-link-shaped string at run time for one agent, from the
/// host and path the agent list holds apart. A scan rule fixture needs the
/// real shape to prove the rule fires, but this repository's own `osf scan`
/// run has no exclusion for a test file, so the shape must never sit in
/// any source text as one contiguous literal.
pub fn session_link_for(host: &str, path: &str, id: &str) -> String {
    format!("https://{host}{path}{id}")
}

/// A session link for the first agent in the list that has hosted
/// sessions, for tests that need any one link.
pub fn session_link(id: &str) -> String {
    let (host, path) = osf::agents::AGENTS
        .iter()
        .find_map(osf::agents::Agent::hosted)
        .expect("at least one agent has hosted sessions");
    session_link_for(host, path, id)
}

/// A path into one agent's state directory, at one of the places that
/// agent keeps its sessions.
pub fn agent_state_path(state_dir: &str, session_path: &str) -> String {
    let home = "~";
    format!("{home}/{state_dir}/{session_path}/2026-09-17-abc.jsonl")
}

/// A scan configuration that states the repository is public, so a test
/// never depends on a network lookup of the real visibility.
pub fn public_config() -> osf::config::ScanConfig {
    osf::config::ScanConfig {
        repository_visibility: "public".to_string(),
        ..osf::config::ScanConfig::default()
    }
}

/// A throwaway repository with an `origin` remote under `owner`, and the
/// configuration that calls it public.
pub fn public_repo(name: &str, owner: &str) -> (TempRepo, osf::config::ScanConfig) {
    let repo = TempRepo::new(name);
    repo.git(&[
        "remote",
        "add",
        "origin",
        &format!("https://github.com/{owner}/tools.git"),
    ]);
    (repo, public_config())
}

/// A `file:` address for a path, built at run time for the same reason as
/// [`session_link_for`].
pub fn file_url(path: &str) -> String {
    let scheme = "file://";
    if path.as_bytes().get(1) == Some(&b':') {
        format!("{scheme}/{}", path.replace('\\', "/"))
    } else {
        format!("{scheme}{path}")
    }
}

/// Builds a Windows user-path-shaped string at run time, for the same
/// reason as [`session_link_for`].
pub fn windows_user_path(user: &str) -> String {
    let drive = "D:";
    let marker = r"\Users\";
    format!(r"{drive}{marker}{user}\work\notes.md")
}

/// The same Windows path written with forward slashes, as a shell or a
/// hook file often spells it.
pub fn windows_forward_path(user: &str) -> String {
    let drive = "C:";
    let marker = "/Users";
    format!("{drive}{marker}/{user}/work/notes.md")
}

/// A Windows network share path.
pub fn network_share_path(server: &str, share: &str) -> String {
    let sep = "\\";
    format!("{sep}{sep}{server}{sep}{share}{sep}notes.md")
}

/// Builds a Linux home-directory-path-shaped string at run time, for the
/// same reason as [`session_link_for`].
pub fn home_path(user: &str) -> String {
    let prefix = "/home";
    format!("{prefix}/{user}/work/notes.md")
}

/// A macOS home-directory path.
pub fn mac_home_path(user: &str) -> String {
    let prefix = "/Users";
    format!("{prefix}/{user}/work/notes.md")
}

/// The root account's home on Linux.
pub fn root_home_path() -> String {
    format!("/{}/work/notes.md", "root")
}

/// A Windows user folder seen through the Windows Subsystem for Linux.
pub fn wsl_user_path(user: &str) -> String {
    let mount = "/mnt/c";
    let users = "Users";
    format!("{mount}/{users}/{user}/work/notes.md")
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
