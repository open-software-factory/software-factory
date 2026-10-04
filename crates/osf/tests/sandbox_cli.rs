//! CLI-level cases for `osf sandbox run`: the dry-run argv, the two folders,
//! and the refusals that must land before any docker call.

mod common;

use common::{isolated_home, run_osf, run_osf_with_env, TempDir};
use serde_json::Value;
use std::path::Path;

/// The argv array under `key`, as plain strings.
fn argv(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .expect("an argv array")
        .iter()
        .map(|part| part.as_str().expect("an argv word").to_string())
        .collect()
}

/// The `source=` path of a `--mount=type=bind,...` mount value.
fn mount_source(mount: &str) -> &str {
    let mount = mount.strip_prefix("--mount=").unwrap_or(mount);
    mount
        .strip_prefix("type=bind,source=")
        .and_then(|rest| rest.split(",target=").next())
        .expect("a bind mount carries a source")
}

/// The `--network` value in `create`.
fn network_of(create: &[String]) -> Option<&str> {
    let at = create.iter().position(|part| part == "--network")?;
    create.get(at + 1).map(String::as_str)
}

/// The one mount value ending in `,target=<target>`.
fn mount_for<'a>(create: &'a [String], target: &str) -> &'a String {
    let suffix = format!(",target={target}");
    create
        .iter()
        .find(|part| part.starts_with("--mount=type=bind,") && part.ends_with(&suffix))
        .unwrap_or_else(|| panic!("a mount for {target}: {create:?}"))
}

#[test]
fn dry_run_prints_the_pinned_argv_for_both_folders() {
    let repo = TempDir::new("osf-sandbox-cli-repo");
    let state = TempDir::new("osf-sandbox-cli-state");
    let repo_arg = repo.to_string_lossy().into_owned();
    let state_arg = state.to_string_lossy().into_owned();
    let home = isolated_home("osf-sandbox-cli-dry");

    let output = run_osf(
        &repo,
        &home,
        &[
            "sandbox",
            "run",
            "--image",
            "example/base:1",
            "--repo",
            &repo_arg,
            "--state",
            &state_arg,
            "--name",
            "osf-sandbox-cli",
            "--dry-run",
            "--",
            "sh",
            "-c",
            "echo hi",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).expect("one JSON object");

    let create = argv(&value, "create");
    assert_eq!(network_of(&create), Some("none"), "{create:?}");
    let workspace = mount_for(&create, "/workspace");
    assert_eq!(
        Path::new(mount_source(workspace)).file_name(),
        repo.file_name(),
        "{workspace}"
    );
    let state_mount = mount_for(&create, "/state");
    assert_eq!(
        Path::new(mount_source(state_mount)).file_name(),
        state.file_name(),
        "{state_mount}"
    );

    let exec = argv(&value, "exec");
    assert!(
        exec.ends_with(&["sh".to_string(), "-c".to_string(), "echo hi".to_string()]),
        "{exec:?}"
    );
    assert_eq!(argv(&value, "start"), vec!["start", "--", "<id>"]);
    assert_eq!(argv(&value, "remove"), vec!["rm", "--force", "--", "<id>"]);
}

#[test]
fn dry_run_network_open_names_the_bridge() {
    let repo = TempDir::new("osf-sandbox-cli-open-repo");
    let state = TempDir::new("osf-sandbox-cli-open-state");
    let repo_arg = repo.to_string_lossy().into_owned();
    let state_arg = state.to_string_lossy().into_owned();
    let home = isolated_home("osf-sandbox-cli-open");
    let output = run_osf(
        &repo,
        &home,
        &[
            "sandbox",
            "run",
            "--image",
            "example/base:1",
            "--repo",
            &repo_arg,
            "--state",
            &state_arg,
            "--network",
            "open",
            "--dry-run",
            "--",
            "true",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).expect("one JSON object");
    assert_eq!(network_of(&argv(&value, "create")), Some("bridge"));
}

#[test]
fn an_unpinned_image_is_refused_in_dry_run() {
    let repo = TempDir::new("osf-sandbox-cli-unpinned-repo");
    let state = TempDir::new("osf-sandbox-cli-unpinned-state");
    let repo_arg = repo.to_string_lossy().into_owned();
    let state_arg = state.to_string_lossy().into_owned();
    let home = isolated_home("osf-sandbox-cli-unpinned");
    let output = run_osf(
        &repo,
        &home,
        &[
            "sandbox",
            "run",
            "--image",
            "example/base:latest",
            "--repo",
            &repo_arg,
            "--state",
            &state_arg,
            "--dry-run",
            "--",
            "true",
        ],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("must be pinned"), "{stderr}");
}

#[test]
fn a_root_user_is_refused() {
    let repo = TempDir::new("osf-sandbox-cli-root-repo");
    let state = TempDir::new("osf-sandbox-cli-root-state");
    let repo_arg = repo.to_string_lossy().into_owned();
    let state_arg = state.to_string_lossy().into_owned();
    let home = isolated_home("osf-sandbox-cli-root");
    let output = run_osf(
        &repo,
        &home,
        &[
            "sandbox",
            "run",
            "--image",
            "example/base:1",
            "--repo",
            &repo_arg,
            "--state",
            &state_arg,
            "--user",
            "root",
            "--dry-run",
            "--",
            "true",
        ],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("must not be root"), "{stderr}");
}

#[test]
fn a_zero_spelled_user_is_refused() {
    let repo = TempDir::new("osf-sandbox-cli-zero-repo");
    let state = TempDir::new("osf-sandbox-cli-zero-state");
    let repo_arg = repo.to_string_lossy().into_owned();
    let state_arg = state.to_string_lossy().into_owned();
    let home = isolated_home("osf-sandbox-cli-zero");
    for user in ["00", "+0", "00:1"] {
        let output = run_osf(
            &repo,
            &home,
            &[
                "sandbox",
                "run",
                "--image",
                "example/base:1",
                "--repo",
                &repo_arg,
                "--state",
                &state_arg,
                "--user",
                user,
                "--dry-run",
                "--",
                "true",
            ],
        );
        assert_eq!(output.status.code(), Some(2), "{user}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("user"), "{user}: {stderr}");
    }
}

#[test]
fn a_missing_repo_folder_is_refused_and_named() {
    let repo = TempDir::new("osf-sandbox-cli-missing-repo");
    let state = TempDir::new("osf-sandbox-cli-missing-state");
    let missing = repo.join("does-not-exist");
    let missing_arg = missing.to_string_lossy().into_owned();
    let state_arg = state.to_string_lossy().into_owned();
    let home = isolated_home("osf-sandbox-cli-missing");
    let output = run_osf(
        &repo,
        &home,
        &[
            "sandbox",
            "run",
            "--image",
            "example/base:1",
            "--repo",
            &missing_arg,
            "--state",
            &state_arg,
            "--dry-run",
            "--",
            "true",
        ],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--repo"), "{stderr}");
}

#[test]
fn an_unpinned_image_is_refused_before_any_docker_call() {
    let repo = TempDir::new("osf-sandbox-cli-path-repo");
    let state = TempDir::new("osf-sandbox-cli-path-state");
    let empty = TempDir::new("osf-sandbox-cli-empty-path");
    let repo_arg = repo.to_string_lossy().into_owned();
    let state_arg = state.to_string_lossy().into_owned();
    let empty_arg = empty.to_str().expect("utf8 path");
    let home = isolated_home("osf-sandbox-cli-path");
    let output = run_osf_with_env(
        &repo,
        &home,
        &[("PATH", empty_arg)],
        &[
            "sandbox",
            "run",
            "--image",
            "example/base:latest",
            "--repo",
            &repo_arg,
            "--state",
            &state_arg,
            "--",
            "true",
        ],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("must be pinned"), "{stderr}");
}

#[test]
fn a_command_with_no_words_is_a_clap_error() {
    let repo = TempDir::new("osf-sandbox-cli-no-words-repo");
    let state = TempDir::new("osf-sandbox-cli-no-words-state");
    let repo_arg = repo.to_string_lossy().into_owned();
    let state_arg = state.to_string_lossy().into_owned();
    let home = isolated_home("osf-sandbox-cli-no-words");
    let output = run_osf(
        &repo,
        &home,
        &[
            "sandbox",
            "run",
            "--image",
            "example/base:1",
            "--repo",
            &repo_arg,
            "--state",
            &state_arg,
        ],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}

#[test]
fn a_flag_shaped_image_is_refused_in_dry_run() {
    let repo = TempDir::new("osf-sandbox-cli-flag-repo");
    let state = TempDir::new("osf-sandbox-cli-flag-state");
    let repo_arg = repo.to_string_lossy().into_owned();
    let state_arg = state.to_string_lossy().into_owned();
    let home = isolated_home("osf-sandbox-cli-flag");
    let cases: [&[&str]; 3] = [
        &["--image=--user=0:1"],
        &["--image=-x"],
        &["--image", "sleep"],
    ];
    for image_args in cases {
        let mut args: Vec<&str> = vec!["sandbox", "run"];
        args.extend_from_slice(image_args);
        args.extend_from_slice(&[
            "--repo",
            repo_arg.as_str(),
            "--state",
            state_arg.as_str(),
            "--dry-run",
            "--",
            "true",
        ]);
        let output = run_osf(&repo, &home, &args);
        assert_eq!(output.status.code(), Some(2), "{image_args:?}: {output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("image"), "{image_args:?}: {stderr}");
    }
}

#[test]
fn a_dashed_command_stays_after_the_exec_separator() {
    let repo = TempDir::new("osf-sandbox-cli-dashed-repo");
    let state = TempDir::new("osf-sandbox-cli-dashed-state");
    let repo_arg = repo.to_string_lossy().into_owned();
    let state_arg = state.to_string_lossy().into_owned();
    let home = isolated_home("osf-sandbox-cli-dashed");
    let output = run_osf(
        &repo,
        &home,
        &[
            "sandbox",
            "run",
            "--image",
            "example/base:1",
            "--repo",
            &repo_arg,
            "--state",
            &state_arg,
            "--dry-run",
            "--",
            "--user=0:0",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).expect("one JSON object");
    let exec = argv(&value, "exec");
    assert_eq!(exec, ["exec", "--", "<id>", "--user=0:0"]);
}
