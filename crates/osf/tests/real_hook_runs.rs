//! Real git hook runs against throwaway repositories. Git itself exports
//! `GIT_DIR`, `GIT_INDEX_FILE` and related variables to a hook it starts;
//! these tests check osf's checkpoint behaves when it is started that way,
//! and never touches the repository that started the test run. They do not
//! set any git variable themselves: every git call goes through
//! `common::scratch_git`.

mod common;
use common::{
    git_with_hook_env, isolated_home, run_osf_with_env, session_link, unique_dir, TempRepo,
};
use std::path::{Path, PathBuf};

/// A git command in `dir` through the shared scratch helper.
fn git_in(dir: &Path, args: &[&str]) -> std::process::Output {
    common::scratch_git::run(dir, args)
}

/// `core.bare`, read fresh from `dir`'s own config.
fn core_bare(dir: &Path) -> String {
    let out = git_in(dir, &["config", "--get", "core.bare"]);
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A real `git push --dry-run` through a hooked scratch clone, with `GIT_DIR`/`GIT_WORK_TREE`/`GIT_INDEX_FILE` set by git itself for the hook, must pass and touch neither the sentinel nor the clone.
#[test]
fn a_real_pre_push_hook_run_touches_neither_the_sentinel_nor_the_clone() {
    let sentinel = TempRepo::with_moon_workspace("hook-path-sentinel");
    sentinel.commit("base");
    let sentinel_config_before =
        std::fs::read(sentinel.dir.join(".git").join("config")).expect("sentinel config reads");

    let clone_dir = unique_dir("osf-git-env-hook-clone");
    let clone = git_in(
        &sentinel.dir,
        &[
            "clone",
            "-q",
            sentinel.dir.to_str().expect("utf8 path"),
            clone_dir.to_str().expect("utf8 path"),
        ],
    );
    assert!(clone.status.success(), "clone failed: {clone:?}");
    assert_eq!(core_bare(&clone_dir), "false", "the clone is bare already");

    let home = isolated_home("hook-clone");
    let install = run_osf_with_env(&clone_dir, &home, &[], &["hooks", "install"]);
    assert!(
        install.status.success(),
        "osf hooks install failed: {}",
        String::from_utf8_lossy(&install.stderr)
    );

    let bare_dir = unique_dir("osf-git-env-hook-bare.git");
    common::scratch_git::init_bare(&bare_dir);
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

    let push = git_with_hook_env(&clone_dir, &home, &["push", "--dry-run", "scratch", "main"]);
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

/// A pre-commit hook script in a folder outside the repository, the way
/// `osf hooks install` places one, that execs the built `osf` binary.
/// Returns the folder so the caller can point `core.hooksPath` at it and
/// remove it once the test is done.
fn write_pre_commit_hook(name: &str) -> PathBuf {
    let bin = env!("CARGO_BIN_EXE_osf").replace('\\', "/");
    let hooks_dir = unique_dir(&format!("osf-git-env-pre-commit-hooks-{name}"));
    std::fs::create_dir_all(&hooks_dir).expect("hooks dir creates");
    let script = format!("#!/bin/sh\nexec \"{bin}\" verify --checkpoint pre-commit\n");
    let hook = hooks_dir.join("pre-commit");
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
    hooks_dir
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

/// `git commit -a` folds a working-tree-only change into a
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

    let hooks_dir = write_pre_commit_hook("secret");
    let hooks_path = git_in(
        &repo.dir,
        &[
            "config",
            "core.hooksPath",
            hooks_dir.to_str().expect("utf8 path"),
        ],
    );
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

    let _ = std::fs::remove_dir_all(&hooks_dir);
}

/// The whole pre-push checkpoint (writing, general and staged
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

    repo.write(
        "README.md",
        "a clean repository\nwith one more clean line\n",
    );
    repo.commit("a clean follow-up commit");

    // Installed only now, after every commit, so its pre-commit script never fires against the real machine's own osf and HOME.
    let home = isolated_home("whole-checkpoint");
    let install = run_osf_with_env(&repo.dir, &home, &[], &["hooks", "install"]);
    assert!(
        install.status.success(),
        "osf hooks install failed: {}",
        String::from_utf8_lossy(&install.stderr)
    );

    let bare_dir = unique_dir("osf-git-env-whole-checkpoint-bare.git");
    common::scratch_git::init_bare(&bare_dir);
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

    let push = git_with_hook_env(&repo.dir, &home, &["push", "--dry-run", "scratch", "main"]);
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
    // A first push with no remote ref checks every file, and the task's own binary path is not scannable.
    main_repo.track_origin_main();

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

    // Installed only now, after the commit above, so its pre-commit script never fires against the real machine's own osf and HOME.
    let home = isolated_home("linked-worktree");
    let install = run_osf_with_env(&worktree_dir, &home, &[], &["hooks", "install"]);
    assert!(
        install.status.success(),
        "osf hooks install failed: {}",
        String::from_utf8_lossy(&install.stderr)
    );

    let bare_dir = unique_dir("osf-git-env-linked-worktree-bare.git");
    common::scratch_git::init_bare(&bare_dir);
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

    let push = git_with_hook_env(
        &worktree_dir,
        &home,
        &["push", "--dry-run", "scratch", "feature"],
    );
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
