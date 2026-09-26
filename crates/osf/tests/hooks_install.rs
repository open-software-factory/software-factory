//! `osf hooks install`: writes osf's own hook scripts outside a scratch
//! repository and points that repository's git config at them, so a plain
//! shell gets the same checks a coding agent's own hook event does.

mod common;
use common::{git_with_hook_env, isolated_home, run_osf_with_env, unique_dir, TempRepo};
use std::path::{Path, PathBuf};
use std::process::Command;

/// `core.hooksPath`, read straight from the repository's own local config,
/// `None` when it is not set. Never asserts: a test needs both outcomes.
fn hooks_path_config(dir: &Path) -> Option<String> {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .args(["config", "--local", "--get", "core.hooksPath"]);
    osf::scrub_git_env(&mut command);
    let output = command.output().expect("git runs");
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The hook scripts `osf hooks install` writes.
const SCRIPT_NAMES: &[&str] = &["pre-commit", "commit-msg", "pre-push"];

#[test]
fn install_writes_the_scripts_and_points_core_hooks_path_at_them() {
    let home = isolated_home("hooks-install-basic");
    let repo = TempRepo::new("hooks-install-basic");

    let install = run_osf_with_env(&repo.dir, &home, &[], &["hooks", "install"]);
    assert!(
        install.status.success(),
        "osf hooks install failed: {}",
        String::from_utf8_lossy(&install.stderr)
    );

    let hooks_dir = home.join(".osf").join("githooks");
    for name in SCRIPT_NAMES {
        let path = hooks_dir.join(name);
        assert!(path.is_file(), "{} was not written", path.display());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&path)
                .expect("metadata")
                .permissions()
                .mode();
            assert!(mode & 0o111 != 0, "{} is not executable", path.display());
        }
    }

    let configured = hooks_path_config(&repo.dir).expect("core.hooksPath is set");
    assert_eq!(
        Path::new(&configured),
        hooks_dir,
        "core.hooksPath does not point at osf's own folder"
    );
}

#[test]
fn install_is_safe_to_run_twice() {
    let home = isolated_home("hooks-install-twice");
    let repo = TempRepo::new("hooks-install-twice");

    let first = run_osf_with_env(&repo.dir, &home, &[], &["hooks", "install"]);
    assert!(first.status.success());
    let hooks_dir = home.join(".osf").join("githooks");
    let before: Vec<(PathBuf, Vec<u8>)> = SCRIPT_NAMES
        .iter()
        .map(|n| {
            let p = hooks_dir.join(n);
            let bytes = std::fs::read(&p).expect("script reads");
            (p, bytes)
        })
        .collect();
    let path_before = hooks_path_config(&repo.dir).expect("set after first install");

    let second = run_osf_with_env(&repo.dir, &home, &[], &["hooks", "install"]);
    assert!(
        second.status.success(),
        "second install failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );

    for (path, bytes) in &before {
        let after = std::fs::read(path).expect("script still reads");
        assert_eq!(
            bytes,
            &after,
            "{} changed on a second install",
            path.display()
        );
    }
    let path_after = hooks_path_config(&repo.dir).expect("still set after second install");
    assert_eq!(
        path_before, path_after,
        "core.hooksPath changed on a second install"
    );
}

#[test]
fn check_fails_before_install_and_passes_after() {
    let home = isolated_home("hooks-install-check");
    let repo = TempRepo::new("hooks-install-check");

    let before = run_osf_with_env(&repo.dir, &home, &[], &["hooks", "install", "--check"]);
    assert!(
        !before.status.success(),
        "--check should fail before install"
    );

    let install = run_osf_with_env(&repo.dir, &home, &[], &["hooks", "install"]);
    assert!(install.status.success());

    let after = run_osf_with_env(&repo.dir, &home, &[], &["hooks", "install", "--check"]);
    assert!(
        after.status.success(),
        "--check should pass after install: {}",
        String::from_utf8_lossy(&after.stdout)
    );
}

/// A repository whose `core.hooksPath` still names the tracked `.osf/hooks`
/// folder this project retired gets a clear message, not just a bare
/// failure.
#[test]
fn check_names_the_retired_tracked_hooks_path() {
    let home = isolated_home("hooks-install-retired");
    let repo = TempRepo::new("hooks-install-retired");
    repo.git(&["config", "core.hooksPath", ".osf/hooks"]);

    let check = run_osf_with_env(&repo.dir, &home, &[], &["hooks", "install", "--check"]);
    assert!(!check.status.success());
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(
        stdout.contains(".osf/hooks"),
        "expected a message naming the retired path, got: {stdout}"
    );
}

/// A real `git commit`, once `osf hooks install` has run, invokes the
/// pre-commit checkpoint: proven by the journal entry it writes, not by
/// guessing at the scan's own findings.
#[test]
fn a_real_git_commit_runs_the_pre_commit_checkpoint() {
    let home = isolated_home("hooks-install-commit");
    let repo = TempRepo::with_moon_workspace("hooks-install-commit");
    repo.write("README.md", "a clean repository\n");
    repo.commit("base");

    let install = run_osf_with_env(&repo.dir, &home, &[], &["hooks", "install"]);
    assert!(
        install.status.success(),
        "osf hooks install failed: {}",
        String::from_utf8_lossy(&install.stderr)
    );

    repo.write("README.md", "a clean repository\nwith one more line\n");
    let output = git_with_hook_env(
        &repo.dir,
        &home,
        &["commit", "-a", "-m", "a clean follow-up commit"],
    );
    assert!(
        output.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        journal_has_run_prefixed("pre-commit-", &home),
        "expected a pre-commit checkpoint journal entry"
    );
}

/// Whether any journal buffer file under `home`'s state directory belongs to
/// a run whose id starts with `prefix`: proof a checkpoint actually ran,
/// rather than guessing at a check's own findings.
fn journal_has_run_prefixed(prefix: &str, home: &Path) -> bool {
    let buffer_dir = home.join(".osf").join("state").join("buffer");
    std::fs::read_dir(&buffer_dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", buffer_dir.display()))
        .any(|entry| {
            entry
                .expect("dir entry reads")
                .file_name()
                .to_string_lossy()
                .starts_with(prefix)
        })
}

/// A real `git push`, once `osf hooks install` has run, invokes the
/// pre-push checkpoint: proven the same way as the pre-commit test, by the
/// journal entry it writes.
#[test]
fn a_real_git_push_runs_the_pre_push_checkpoint() {
    let home = isolated_home("hooks-install-push");
    let repo = TempRepo::with_moon_workspace("hooks-install-push");
    repo.write("README.md", "a clean repository\n");
    repo.commit("base");
    repo.track_origin_main();

    let install = run_osf_with_env(&repo.dir, &home, &[], &["hooks", "install"]);
    assert!(
        install.status.success(),
        "osf hooks install failed: {}",
        String::from_utf8_lossy(&install.stderr)
    );

    repo.write("README.md", "a clean repository\nwith one more line\n");
    repo.commit("a clean follow-up commit");

    let bare_dir = unique_dir("osf-hooks-install-push-bare.git");
    let mut bare_init = Command::new("git");
    bare_init.args(["init", "-q", "--bare"]).arg(&bare_dir);
    osf::scrub_git_env(&mut bare_init);
    assert!(
        bare_init.status().expect("git init runs").success(),
        "bare remote init failed"
    );
    repo.git(&[
        "remote",
        "add",
        "scratch",
        bare_dir.to_str().expect("utf8 path"),
    ]);

    let push = git_with_hook_env(&repo.dir, &home, &["push", "scratch", "main"]);
    assert!(
        push.status.success(),
        "git push failed: {}",
        String::from_utf8_lossy(&push.stderr)
    );

    assert!(
        journal_has_run_prefixed("pre-push-", &home),
        "expected a pre-push checkpoint journal entry"
    );

    let _ = std::fs::remove_dir_all(&bare_dir);
}

/// A global `core.hooksPath`, with nothing set locally in the repository
/// itself, must not satisfy `--check`: this project reads only the
/// repository's own choice, never one inherited from the user's global
/// config, so a stray global setting can never make `--check` lie.
#[test]
fn check_ignores_a_global_hooks_path_with_no_local_setting() {
    let home = isolated_home("hooks-install-global-only");
    std::fs::write(
        home.join(".gitconfig"),
        "[core]\n\thooksPath = /somewhere/global-only-hooks\n",
    )
    .expect(".gitconfig writes");
    let repo = TempRepo::new("hooks-install-global-only");

    let check = run_osf_with_env(&repo.dir, &home, &[], &["hooks", "install", "--check"]);
    assert!(!check.status.success());
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(
        stdout.contains("core.hooksPath is not set"),
        "expected a not-set message, got: {stdout}"
    );
}
