//! Hermetic guard for the fix in `crate::git::scrub_git_env`: a test (or a
//! real git hook) that starts `git` in a throwaway directory must never be
//! redirected by an inherited `GIT_DIR`/`GIT_WORK_TREE`/`GIT_INDEX_FILE`.

mod common;
use common::{session_link, unique_dir, TempRepo};
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
        let dir = unique_dir(&format!("osf-git-env-sentinel-{name}"));
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

/// Runs `cargo test <args>` with `GIT_DIR`/`GIT_WORK_TREE` set on that one
/// child process, the way a pre-push hook exports them, and asserts it
/// passed. Also clears every inherited `OSF_*`/`MOON_*` variable first:
/// this repository's own checkpoint sets `OSF_CONFIG`, `OSF_DENYLIST` and
/// various `MOON_*` variables on `cargo test` itself, and the nested run
/// here must see this workspace's plain `osf.toml`, not whatever config or
/// task state the outer checkpoint run left behind.
fn run_cargo_test_under_sentinel_git_env(sentinel: &Sentinel, args: &[&str]) {
    let mut command = Command::new(env!("CARGO"));
    command.current_dir(workspace_root()).arg("test").args(args);
    for (key, _) in std::env::vars() {
        if key.starts_with("OSF_") || key.starts_with("MOON_") {
            command.env_remove(key);
        }
    }
    let status = command
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

/// Task 8's own regression: `risk::assess` takes an explicit folder, but
/// every git call it made ran unscrubbed, so an inherited `GIT_DIR` (the
/// same shape a real pre-push hook running `cargo test` leaves on the
/// process) redirected it to the wrong repository and the wrong tier.
#[test]
fn risk_tests_never_touch_a_sentinel_pointed_to_by_git_dir() {
    let sentinel = Sentinel::new("risk");
    let before = sentinel.config_bytes();
    run_cargo_test_under_sentinel_git_env(&sentinel, &["-p", "osf", "--test", "risk"]);
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

/// A git command in `dir`, with every inherited `GIT_*` variable scrubbed
/// first, along with `OSF_*` and `MOON_*`: this repository's own checkpoint
/// sets `OSF_CONFIG`, `OSF_DENYLIST` and various `MOON_*` variables on
/// `cargo test` itself, and a `git commit`/`git push` here can run a real
/// hook that execs the built `osf` binary, which would otherwise inherit
/// them and resolve a config path relative to the wrong directory.
fn git_in(dir: &Path, args: &[&str]) -> std::process::Output {
    let mut command = Command::new("git");
    command.current_dir(dir).args(args);
    osf::scrub_git_env(&mut command);
    for (key, _) in std::env::vars() {
        if key.starts_with("OSF_") || key.starts_with("MOON_") {
            command.env_remove(key);
        }
    }
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

    let clone_dir = unique_dir("osf-git-env-hook-clone");
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

    let bare_dir = unique_dir("osf-git-env-hook-bare.git");
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

/// A `.osf/hooks/pre-commit` script that execs the built `osf` binary.
fn write_pre_commit_hook(dir: &Path) {
    let bin = env!("CARGO_BIN_EXE_osf").replace('\\', "/");
    std::fs::create_dir_all(dir.join(".osf").join("hooks")).expect("hooks dir creates");
    let script = format!("#!/bin/sh\nexec \"{bin}\" verify --checkpoint pre-commit\n");
    let hook = dir.join(".osf").join("hooks").join("pre-commit");
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

/// A throwaway repository with one moon task, `command`, tagged for
/// `tags`, plus the `.moon/workspace.yml` moon itself needs to see it.
fn repo_with_moon_task(name: &str, task: &str, command: &str, tags: &str) -> TempRepo {
    let repo = TempRepo::new(name);
    repo.write(
        ".moon/workspace.yml",
        "projects:\n  osf: '.osf'\nvcs:\n  client: git\n  defaultBranch: main\n",
    );
    repo.write(
        ".osf/moon.yml",
        &format!(
            "language: rust\ntasks:\n  {task}:\n    command: '{command}'\n    inputs: ['/**/*']\n    tags: [{tags}]\n    options:\n      runFromWorkspaceRoot: true\n      cache: false\n      shell: false\n"
        ),
    );
    repo
}

/// Bullet 2: `git commit -a` folds a working-tree-only change into a
/// temporary index and hands the pre-commit hook `GIT_INDEX_FILE` for it,
/// a path inside this repository's own git directory (an ordinary,
/// non-worktree hook run gets no `GIT_DIR` at all for it; a hook run from a
/// linked worktree does, pointing at that worktree's own git directory —
/// see `a_real_pre_push_hook_from_a_linked_worktree_never_touches_the_main_repository`
/// below). A secret present only in that `-a`-only change must still be
/// caught by the staged-content scan, and the commit refused: proof that
/// `GIT_INDEX_FILE` survives when its own path lies inside the folder
/// named.
#[test]
fn a_secret_only_in_the_dash_a_change_is_caught_and_the_commit_is_refused() {
    let bin = env!("CARGO_BIN_EXE_osf").replace('\\', "/");
    let repo = repo_with_moon_task(
        "pre-commit-secret",
        "scan-staged",
        &format!("\"{bin}\" check scan-staged --checkpoint pre-commit --sarif-out .osf/out/scan-staged.sarif"),
        "osf-pre-commit",
    );
    repo.write("notes.md", "base\n");
    repo.commit("base");

    write_pre_commit_hook(&repo.dir);
    let hooks_path = git_in(&repo.dir, &["config", "core.hooksPath", ".osf/hooks"]);
    assert!(hooks_path.status.success(), "core.hooksPath set failed");

    // Staged, secret-free: the real index holds this.
    repo.write("notes.md", "base\nmore\n");
    repo.stage("notes.md");

    // Working-tree-only: only `git commit -a` folds this in, into a
    // temporary index, never the real one.
    let secret = session_link("dash-a-secret");
    repo.write("notes.md", &format!("base\nmore\n{secret}\n"));

    let commit = git_in(&repo.dir, &["commit", "-a", "-m", "add a secret"]);
    assert!(
        !commit.status.success(),
        "the commit should have been refused"
    );

    let head_count = git_in(&repo.dir, &["rev-list", "--count", "HEAD"]);
    assert_eq!(
        String::from_utf8_lossy(&head_count.stdout).trim(),
        "1",
        "a refused commit must not add a second one"
    );
}

/// Bullet 3: the whole pre-push checkpoint (writing, general and staged
/// secret scanning together, the way the real one is composed) still
/// passes through a real `git push --dry-run` to a local bare remote, once
/// every git call osf itself makes is scrubbed for the folder it was given.
#[test]
fn the_whole_pre_push_checkpoint_passes_through_a_real_dry_run_push() {
    let bin = env!("CARGO_BIN_EXE_osf").replace('\\', "/");
    let repo = TempRepo::new("whole-pre-push-checkpoint");
    repo.write(
        ".moon/workspace.yml",
        "projects:\n  osf: '.osf'\nvcs:\n  client: git\n  defaultBranch: main\n",
    );
    repo.write(
        ".osf/moon.yml",
        &format!(
            "language: rust\ntasks:\n  lint-writing:\n    command: '\"{bin}\" check lint-writing --checkpoint pre-push --sarif-out .osf/out/lint-writing.sarif'\n    inputs: ['/**/*.md']\n    tags: [osf-pre-push]\n    options:\n      runFromWorkspaceRoot: true\n      cache: false\n      shell: false\n  scan:\n    command: '\"{bin}\" check scan --checkpoint pre-push --sarif-out .osf/out/scan.sarif'\n    inputs: ['/**/*']\n    tags: [osf-pre-push]\n    options:\n      runFromWorkspaceRoot: true\n      cache: false\n      shell: false\n  scan-staged:\n    command: '\"{bin}\" check scan-staged --checkpoint pre-push --sarif-out .osf/out/scan-staged.sarif'\n    inputs: ['/**/*']\n    tags: [osf-pre-push]\n    options:\n      runFromWorkspaceRoot: true\n      cache: false\n      shell: false\n"
        ),
    );
    repo.write("README.md", "a clean repository\n");
    repo.commit("base");
    repo.track_origin_main();

    write_pre_push_hook(&repo.dir);
    let hooks_path = git_in(&repo.dir, &["config", "core.hooksPath", ".osf/hooks"]);
    assert!(hooks_path.status.success(), "core.hooksPath set failed");

    repo.write(
        "README.md",
        "a clean repository\nwith one more clean line\n",
    );
    repo.commit("a clean follow-up commit");

    let bare_dir = unique_dir("osf-git-env-whole-checkpoint-bare.git");
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
        &repo.dir,
        &[
            "remote",
            "add",
            "scratch",
            bare_dir.to_str().expect("utf8 path"),
        ],
    );
    assert!(remote.status.success(), "remote add failed");

    let push = git_in(&repo.dir, &["push", "--dry-run", "scratch", "main"]);
    assert!(
        push.status.success(),
        "the whole pre-push checkpoint failed: {}",
        String::from_utf8_lossy(&push.stderr)
    );

    let _ = std::fs::remove_dir_all(&bare_dir);
}

/// Item 2: a hook run from a linked worktree gets a real, non-empty
/// `GIT_DIR` from git itself (`<main>/.git/worktrees/<name>`), unlike an
/// ordinary same-directory hook run. This is how the incident that
/// motivated this fix actually happened: `cargo test` running inside this
/// very worktree's own pre-push hook inherited that `GIT_DIR`. A real
/// `git push --dry-run` from a linked worktree, through a real pre-push
/// hook running the built osf checkpoint, must still pass and must never
/// touch the main repository's own (shared) config.
#[test]
fn a_real_pre_push_hook_from_a_linked_worktree_never_touches_the_main_repository() {
    let bin = env!("CARGO_BIN_EXE_osf").replace('\\', "/");
    let main_repo = repo_with_moon_task(
        "linked-worktree-main",
        "scan",
        &format!("\"{bin}\" check scan --checkpoint pre-push --sarif-out .osf/out/scan.sarif"),
        "osf-pre-push",
    );
    main_repo.write("README.md", "a clean repository\n");
    main_repo.commit("base");

    let worktree_dir = unique_dir("osf-git-env-linked-worktree");
    let worktree_add = git_in(
        &main_repo.dir,
        &[
            "worktree",
            "add",
            worktree_dir.to_str().expect("utf8 path"),
            "-b",
            "feature",
        ],
    );
    assert!(
        worktree_add.status.success(),
        "worktree add failed: {}",
        String::from_utf8_lossy(&worktree_add.stderr)
    );

    write_pre_push_hook(&worktree_dir);
    let hooks_path = git_in(&worktree_dir, &["config", "core.hooksPath", ".osf/hooks"]);
    assert!(hooks_path.status.success(), "core.hooksPath set failed");

    std::fs::write(
        worktree_dir.join("README.md"),
        "a clean repository\nedited in the linked worktree\n",
    )
    .expect("README.md writes");
    let add = git_in(&worktree_dir, &["add", "README.md"]);
    assert!(add.status.success(), "git add failed");
    let commit = git_in(
        &worktree_dir,
        &["commit", "-q", "-m", "a clean commit made in the worktree"],
    );
    assert!(commit.status.success(), "commit failed");

    let bare_dir = unique_dir("osf-git-env-linked-worktree-bare.git");
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
        &worktree_dir,
        &[
            "remote",
            "add",
            "scratch",
            bare_dir.to_str().expect("utf8 path"),
        ],
    );
    assert!(remote.status.success(), "remote add failed");

    let main_config_before =
        std::fs::read(main_repo.dir.join(".git").join("config")).expect("main config reads");

    let push = git_in(&worktree_dir, &["push", "--dry-run", "scratch", "feature"]);
    assert!(
        push.status.success(),
        "the linked-worktree pre-push hook failed: {}",
        String::from_utf8_lossy(&push.stderr)
    );

    let main_config_after =
        std::fs::read(main_repo.dir.join(".git").join("config")).expect("main config reads");
    assert_eq!(
        main_config_before, main_config_after,
        "the main repository's shared config changed"
    );

    let _ = std::fs::remove_dir_all(&worktree_dir);
    let _ = std::fs::remove_dir_all(&bare_dir);
}
