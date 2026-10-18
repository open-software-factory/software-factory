//! The shared scratch-git helper refuses what it must and runs git from an
//! empty environment. None of these tests sets or reads a process-wide
//! variable.

mod common;
use common::{scratch_git, unique_dir};
use std::path::Path;

/// The top of the filesystem, which is not under the temp root.
fn filesystem_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .last()
        .expect("a path has a root")
        .to_path_buf()
}

#[test]
fn it_initialises_a_repository_with_a_fixed_identity_and_no_global_config() {
    let dir = unique_dir("osf-scratch-git-init");
    scratch_git::init(&dir);
    let listing = scratch_git::ok(&dir, &["config", "--list", "--show-origin"]);
    for line in listing.lines() {
        let origin = line.split('\t').next().unwrap_or_default();
        assert!(
            origin == "file:.git/config" || origin.starts_with("command line:"),
            "a setting came from outside the repository's own config: {line}"
        );
    }
    assert_eq!(
        scratch_git::ok(&dir, &["config", "user.name"]).trim(),
        "Test"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_hook_it_triggers_sees_only_the_cleared_environment() {
    let dir = unique_dir("osf-scratch-git-hook-env");
    scratch_git::init(&dir);
    let hooks = dir.join(".git").join("hooks");
    let out = dir.join("hook-env.txt");
    let script = format!("#!/bin/sh\nenv > '{}'\n", out.display());
    std::fs::write(hooks.join("pre-commit"), script).expect("hook writes");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(
            hooks.join("pre-commit"),
            std::fs::Permissions::from_mode(0o755),
        )
        .expect("hook chmod");
    }
    std::fs::write(dir.join("a.txt"), "a\n").expect("file writes");
    scratch_git::ok(&dir, &["add", "a.txt"]);
    scratch_git::ok(&dir, &["commit", "-q", "-m", "base"]);
    let seen = std::fs::read_to_string(&out).expect("the hook ran and wrote its environment");
    // `cargo test` sets this one on this very process, so the hook only lacks it when the environment was cleared.
    assert!(
        !seen.contains("CARGO_MANIFEST_DIR"),
        "the hook inherited the caller's environment:\n{seen}"
    );
    assert!(
        seen.contains("osf-scratch-git-empty-home"),
        "the hook did not get the empty home:\n{seen}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[should_panic(expected = "outside the temp root")]
fn it_refuses_a_directory_outside_the_temp_root() {
    scratch_git::run(&filesystem_root(), &["status"]);
}

#[test]
#[should_panic(expected = "relative path or one with `..`")]
fn it_refuses_a_relative_directory() {
    scratch_git::run(Path::new("."), &["status"]);
}

#[test]
#[should_panic(expected = "relative path or one with `..`")]
fn it_refuses_a_path_that_climbs_out_of_the_temp_root() {
    let climbing = scratch_git::temp_root().join("..").join("..");
    scratch_git::run(&climbing, &["status"]);
}

#[test]
#[should_panic(expected = "outside the temp root")]
fn it_refuses_a_missing_directory_whose_nearest_parent_is_outside_the_temp_root() {
    let missing = filesystem_root().join("no-such-scratch-folder");
    scratch_git::init(&missing);
}
