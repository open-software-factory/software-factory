//! Hermetic guard for the fix in `crate::git::scrub_git_env`: a test (or a
//! real git hook) that starts `git` in a throwaway directory must never be
//! redirected by an inherited `GIT_DIR`/`GIT_WORK_TREE`/`GIT_INDEX_FILE`.

mod common;
use common::TempRepo;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The workspace root, two directories above this crate's manifest.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates directory")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

/// A throwaway repository standing in for a real repository a caller's own hook exported `GIT_DIR` for.
struct Sentinel {
    dir: PathBuf,
}

impl Sentinel {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("osf-git-env-sentinel-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("sentinel dir creates");
        let mut command = Command::new("git");
        command.current_dir(&dir).args(["init", "-q", "-b", "main"]);
        osf::scrub_git_env(&mut command);
        let status = command.status().expect("git init runs");
        assert!(status.success(), "sentinel git init failed");
        Sentinel { dir }
    }

    fn git_dir(&self) -> PathBuf {
        self.dir.join(".git")
    }

    fn config_bytes(&self) -> Vec<u8> {
        std::fs::read(self.git_dir().join("config")).expect("sentinel config reads")
    }
}

impl Drop for Sentinel {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Runs `cargo test <args>` with `GIT_DIR`/`GIT_WORK_TREE` set on that one child process, the way a pre-push hook exports them, and asserts it passed.
fn run_cargo_test_under_sentinel_git_env(sentinel: &Sentinel, args: &[&str]) {
    let status = Command::new(env!("CARGO"))
        .current_dir(workspace_root())
        .arg("test")
        .args(args)
        .env("GIT_DIR", sentinel.git_dir())
        .env("GIT_WORK_TREE", &sentinel.dir)
        .status()
        .expect("cargo test runs");
    assert!(
        status.success(),
        "cargo test {args:?} failed under a sentinel GIT_DIR"
    );
}

/// Bullet 1: `cargo test -p osf --lib config`, run with `GIT_DIR`/`GIT_WORK_TREE` set the way a pre-push hook sets them, must leave the sentinel byte-identical and pass every test.
#[test]
fn config_unit_tests_never_touch_a_sentinel_pointed_to_by_git_dir() {
    let sentinel = Sentinel::new("config");
    let before = sentinel.config_bytes();
    run_cargo_test_under_sentinel_git_env(&sentinel, &["-p", "osf", "--lib", "config"]);
    let after = sentinel.config_bytes();
    assert_eq!(before, after, "the sentinel's config changed");
}

/// Bullet 1's named integration test file: `checkpoint.rs` runs the real
/// pre-push checkpoint (moon included) through `run_osf`, spawning the
/// built `osf` binary the same way a real hook would.
#[test]
fn checkpoint_tests_never_touch_a_sentinel_pointed_to_by_git_dir() {
    let sentinel = Sentinel::new("checkpoint");
    let before = sentinel.config_bytes();
    run_cargo_test_under_sentinel_git_env(&sentinel, &["-p", "osf", "--test", "checkpoint"]);
    let after = sentinel.config_bytes();
    assert_eq!(before, after, "the sentinel's config changed");
}

/// A `.osf/hooks/pre-push` script that execs the built `osf` binary.
fn write_pre_push_hook(dir: &Path) {
    let bin = env!("CARGO_BIN_EXE_osf").replace('\\', "/");
    std::fs::create_dir_all(dir.join(".osf").join("hooks")).expect("hooks dir creates");
    let script = format!("#!/bin/sh\nexec \"{bin}\" verify --checkpoint pre-push\n");
    let hook = dir.join(".osf").join("hooks").join("pre-push");
    std::fs::write(&hook, script).expect("hook writes");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut perms = std::fs::metadata(&hook)
            .expect("hook metadata")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&hook, perms).expect("hook chmod");
    }
}

/// A git command in `dir`, with every inherited `GIT_*` variable scrubbed first.
fn git_in(dir: &Path, args: &[&str]) -> std::process::Output {
    let mut command = Command::new("git");
    command.current_dir(dir).args(args);
    osf::scrub_git_env(&mut command);
    command.output().expect("git runs")
}

/// `core.bare`, read fresh from `dir`'s own config, with every inherited `GIT_*` variable scrubbed first.
fn core_bare(dir: &Path) -> String {
    let out = git_in(dir, &["config", "--get", "core.bare"]);
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Bullet 2: a real `git push --dry-run` through a hooked scratch clone, with `GIT_DIR`/`GIT_WORK_TREE`/`GIT_INDEX_FILE` set by git itself for the hook, must pass and touch neither the sentinel nor the clone.
#[test]
fn a_real_pre_push_hook_run_touches_neither_the_sentinel_nor_the_clone() {
    let sentinel = TempRepo::with_moon_workspace("hook-path-sentinel");
    sentinel.commit("base");
    let sentinel_config_before =
        std::fs::read(sentinel.dir.join(".git").join("config")).expect("sentinel config reads");

    let clone_dir = std::env::temp_dir().join("osf-git-env-hook-clone");
    let _ = std::fs::remove_dir_all(&clone_dir);
    let clone = git_in(
        Path::new("."),
        &[
            "clone",
            "-q",
            sentinel.dir.to_str().expect("utf8 path"),
            clone_dir.to_str().expect("utf8 path"),
        ],
    );
    assert!(clone.status.success(), "clone failed: {clone:?}");
    assert_eq!(core_bare(&clone_dir), "false", "the clone is bare already");

    write_pre_push_hook(&clone_dir);
    let hooks_path = git_in(&clone_dir, &["config", "core.hooksPath", ".osf/hooks"]);
    assert!(hooks_path.status.success(), "core.hooksPath set failed");

    let bare_dir = std::env::temp_dir().join("osf-git-env-hook-bare.git");
    let _ = std::fs::remove_dir_all(&bare_dir);
    let bare_init = git_in(
        Path::new("."),
        &[
            "init",
            "-q",
            "--bare",
            bare_dir.to_str().expect("utf8 path"),
        ],
    );
    assert!(bare_init.status.success(), "bare remote init failed");
    let remote = git_in(
        &clone_dir,
        &[
            "remote",
            "add",
            "scratch",
            bare_dir.to_str().expect("utf8 path"),
        ],
    );
    assert!(remote.status.success(), "remote add failed");

    let push = git_in(&clone_dir, &["push", "--dry-run", "scratch", "main"]);
    assert!(
        push.status.success(),
        "pre-push hook run failed: {}",
        String::from_utf8_lossy(&push.stderr)
    );

    let sentinel_config_after =
        std::fs::read(sentinel.dir.join(".git").join("config")).expect("sentinel config reads");
    assert_eq!(
        sentinel_config_before, sentinel_config_after,
        "the sentinel's config changed"
    );
    assert_eq!(
        core_bare(&clone_dir),
        "false",
        "the clone was reinitialised bare"
    );

    let _ = std::fs::remove_dir_all(&clone_dir);
    let _ = std::fs::remove_dir_all(&bare_dir);
}
