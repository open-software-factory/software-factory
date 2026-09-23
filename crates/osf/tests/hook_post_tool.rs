//! Integration tests for `osf hook post-tool`: moon required on `PATH`.

mod common;
use common::{isolated_home, run_osf_stdin, session_link, TempRepo};

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

// R14: at the hook checkpoint every check reads the file as it is on disk,
// not at its last commit, because the hook exists to check what the agent
// just wrote. `scan` reading `HEAD` here would miss a secret typed into an
// already-committed file, which is exactly the case this test covers.
#[test]
fn a_secret_written_into_an_existing_committed_file_is_refused_by_the_scan_check() {
    let repo = TempRepo::with_moon_workspace("pt-secret");
    repo.commit("base");
    repo.write("notes.md", "Clean.\n");
    repo.commit("clean");
    repo.write(
        "notes.md",
        &format!("See {} here.\n", session_link("abc123")),
    );
    let home = isolated_home("pt-secret");
    let payload = format!(
        r#"{{"session_id":"s","tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        repo.dir
            .join("notes.md")
            .to_string_lossy()
            .replace('\\', "/")
    );
    let out = run_osf_stdin(&repo.dir, &home, &[], &["hook", "post-tool"], &payload);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("scan-session-link"));
}

// A relative written path is resolved against the repository root and its
// `.`/`..` components collapse lexically first, so `../outside.md` cannot
// walk out of the root and reach a file the hook has no business touching.
#[test]
fn a_relative_path_that_walks_outside_the_repository_is_reported_and_skipped() {
    let repo = TempRepo::with_moon_workspace("pt-outside-rel");
    repo.commit("base");
    let home = isolated_home("pt-outside-rel");
    for raw in ["../outside.md", "sub/../../outside.md"] {
        let payload = format!(
            r#"{{"session_id":"s","tool_name":"Write","tool_input":{{"file_path":"{raw}"}}}}"#
        );
        let out = run_osf_stdin(
            &repo.dir,
            &home,
            &[("OSF_MOON", "/nonexistent/moon")],
            &["hook", "post-tool"],
            &payload,
        );
        assert_eq!(out.status.code(), Some(0), "{raw}: {out:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("outside the repository"),
            "{raw}: {out:?}"
        );
    }
}

#[test]
fn an_absolute_path_outside_the_repository_is_reported_and_skipped() {
    let repo = TempRepo::with_moon_workspace("pt-outside-abs");
    repo.commit("base");
    let home = isolated_home("pt-outside-abs");
    let outside = repo
        .dir
        .parent()
        .expect("the temp repo dir has a parent")
        .join("pt-outside-abs-sibling.md");
    let payload = format!(
        r#"{{"session_id":"s","tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        outside.to_string_lossy().replace('\\', "/")
    );
    let out = run_osf_stdin(
        &repo.dir,
        &home,
        &[("OSF_MOON", "/nonexistent/moon")],
        &["hook", "post-tool"],
        &payload,
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("outside the repository"),
        "{out:?}"
    );
}
