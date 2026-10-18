//! The one place a test runs `git`.
//!
//! Every call starts from an empty environment, so nothing the caller
//! exported (a hook's `GIT_DIR`, a login's identity, a global config) can
//! redirect it, and runs against a directory named with `-C`, which must lie
//! under the system temp directory. A test that builds a scratch repository
//! goes through here and never starts `git` itself;
//! `tests/test_git_discipline.rs` fails the build if one does.
//!
//! This file is compiled twice: as `common::scratch_git` into each
//! integration test, and by path into the library's own unit tests.
#![allow(dead_code)]

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

/// The system temp directory, resolved through any symbolic link.
pub fn temp_root() -> PathBuf {
    std::fs::canonicalize(std::env::temp_dir()) // osf: temp-dir allowed, the shared helper
        .expect("the system temp directory resolves")
}

/// Panics unless `dir` lies under [`temp_root`], with every `..` and symbolic link resolved. A directory that does not exist yet is judged by its nearest existing parent.
pub fn confine_to_temp_root(dir: &Path) -> PathBuf {
    assert!(
        dir.is_absolute() && !dir.components().any(|c| c == Component::ParentDir),
        "scratch git refuses a relative path or one with `..`: {}",
        dir.display()
    );
    let mut existing = dir;
    let mut missing = Vec::new();
    while !existing.exists() {
        missing.push(existing.file_name().expect("a path with a file name"));
        existing = existing.parent().expect("a path with a parent");
    }
    let mut resolved = std::fs::canonicalize(existing).expect("the existing part resolves");
    for part in missing.into_iter().rev() {
        resolved.push(part);
    }
    assert!(
        resolved.starts_with(temp_root()),
        "scratch git refuses a directory outside the temp root: {}",
        dir.display()
    );
    resolved
}

/// An empty folder standing in for the home directory, so no `~/.gitconfig` is read.
fn empty_home() -> PathBuf {
    let home = temp_root().join("osf-scratch-git-empty-home");
    std::fs::create_dir_all(&home).expect("the empty home creates");
    home
}

/// `git -C dir args`, run from an empty environment plus `env`, for a directory under the temp root.
///
/// The environment is `PATH` (to find `git`, and any program a hook runs), an empty `HOME`, `XDG_CONFIG_HOME` and `GIT_CONFIG_NOSYSTEM`, which leaves no global or system configuration. It is not `GIT_CONFIG_GLOBAL` or `GIT_CONFIG_SYSTEM`, because the development container's git wrapper refuses both. `env` overrides any of these.
pub fn run_with_env(dir: &Path, args: &[&str], env: &[(&str, &OsStr)]) -> Output {
    let dir = confine_to_temp_root(dir);
    let home = empty_home();
    let mut command = Command::new("git");
    // `current_dir` as well as `-C`: the unit tests change and remove the process's own directory, and a child that inherits a removed one fails to start.
    command
        .env_clear()
        .current_dir(&dir)
        .arg("-C")
        .arg(&dir)
        .args(args);
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    // Windows programs refuse to start without these.
    for key in ["SystemRoot", "SYSTEMROOT", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("XDG_CONFIG_HOME", &home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("LC_ALL", "C");
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("git runs")
}

/// [`run_with_env`] with nothing added.
pub fn run(dir: &Path, args: &[&str]) -> Output {
    run_with_env(dir, args, &[])
}

/// Runs git and returns its standard output, panicking with git's own message when it fails.
pub fn ok(dir: &Path, args: &[&str]) -> String {
    let output = run(dir, args);
    assert!(
        output.status.success(),
        "git {args:?} failed in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Creates `dir` if needed and initialises a repository on `main` in it, with a fixed identity and no signing, so a commit needs nothing from the machine.
pub fn init(dir: &Path) {
    confine_to_temp_root(dir);
    std::fs::create_dir_all(dir).expect("scratch repository directory creates");
    ok(dir, &["init", "-q", "-b", "main"]);
    ok(dir, &["config", "user.email", "test@example.com"]);
    ok(dir, &["config", "user.name", "Test"]);
    ok(dir, &["config", "commit.gpgsign", "false"]);
}

/// Creates `dir` if needed and initialises a bare repository in it.
pub fn init_bare(dir: &Path) {
    confine_to_temp_root(dir);
    std::fs::create_dir_all(dir).expect("scratch repository directory creates");
    ok(dir, &["init", "-q", "--bare", "-b", "main"]);
}
