//! Integration tests for `osf hook post-tool`: moon required on `PATH`.

mod common;
use common::{isolated_home, run_osf_stdin, TempRepo};

#[test]
fn a_payload_with_no_file_path_exits_zero_without_starting_moon() {
    let repo = TempRepo::with_moon_workspace("pt-none");
    repo.commit("base");
    let home = isolated_home("pt-none");
    let out = run_osf_stdin(
        &repo.dir,
        &home,
        &[("OSF_MOON", "/nonexistent/moon")],
        &["hook", "post-tool"],
        r#"{"session_id":"s","tool_name":"Bash","tool_input":{"command":"ls"}}"#,
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

#[test]
fn a_written_markdown_file_with_an_error_is_refused_with_the_rule_named() {
    let repo = TempRepo::with_moon_workspace("pt-error");
    repo.commit("base");
    repo.write("guide.md", "Do Phase 2 next.\n");
    let home = isolated_home("pt-error");
    let payload = format!(
        r#"{{"session_id":"s","tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        repo.dir
            .join("guide.md")
            .to_string_lossy()
            .replace('\\', "/")
    );
    let out = run_osf_stdin(&repo.dir, &home, &[], &["hook", "post-tool"], &payload);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("chat-local-reference"));
}

// A `--timeout-secs 0` run is not a reliable way to force a timeout here:
// the whole `moon run` round trip (spawn, evaluate the affected task, spawn
// and reap its own child) can complete inside that razor-thin window, so
// whether it counts as a timeout is a race rather than a fact about the
// code under test. This test instead sizes its own task to sleep for
// longer than a small, still-fast `--timeout-secs 2`, the same way
// `crates/osf/tests/moon_adapter.rs`'s `a_timed_out_run_kills_the_whole_process_tree`
// forces a deterministic timeout.
#[test]
fn a_timeout_reports_skipped_and_lets_the_edit_stand() {
    let repo = TempRepo::new("pt-timeout");
    repo.write(
        ".moon/workspace.yml",
        "projects:\n  osf: '.osf'\nvcs:\n  client: git\n  defaultBranch: main\n",
    );
    let sleep = if cfg!(windows) {
        "Start-Sleep -Seconds 6"
    } else {
        "sleep 6"
    };
    repo.write(
        ".osf/moon.yml",
        &format!(
            "tasks:\n  slow:\n    script: '{sleep}'\n    inputs: ['/**/*.md']\n    tags: [osf-hook]\n    options:\n      runFromWorkspaceRoot: true\n"
        ),
    );
    repo.commit("base");
    repo.write("guide.md", "Clean.\n");
    let home = isolated_home("pt-timeout");
    let payload = format!(
        r#"{{"session_id":"s","tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        repo.dir
            .join("guide.md")
            .to_string_lossy()
            .replace('\\', "/")
    );
    let out = run_osf_stdin(
        &repo.dir,
        &home,
        &[],
        &["hook", "post-tool", "--timeout-secs", "2"],
        &payload,
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("skipped"));
}
