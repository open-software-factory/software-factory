//! Integration tests for `osf hook post-tool`: moon required on `PATH`.

mod common;
use common::{isolated_home, run_osf_stdin, session_link, TempRepo};

fn state(home: &std::path::Path) -> std::path::PathBuf {
    home.join(".osf").join("state")
}

/// Every verification-event payload in every buffer file under `home`'s
/// journal state directory, across every run: a timeout writes its own
/// run id, distinct from any other event written in the same test.
fn verification_payloads(home: &std::path::Path) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let buffer = state(home).join("buffer");
    let Ok(entries) = std::fs::read_dir(&buffer) else {
        return out;
    };
    for entry in entries {
        let path = entry.expect("entry").path();
        let text = std::fs::read_to_string(&path).expect("buffer file reads");
        for line in text.lines() {
            let v: serde_json::Value = serde_json::from_str(line).expect("json");
            let is_verification =
                v.get("event_type").and_then(serde_json::Value::as_str) == Some("verification");
            if is_verification {
                if let Some(payload) = v.get("payload") {
                    out.push(payload.clone());
                }
            }
        }
    }
    out
}

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
    // I2: a timeout still journals one verification event per task the
    // checkpoint was about to run, so the record shows a check was skipped
    // rather than silently missing.
    let payloads = verification_payloads(&home);
    assert!(
        payloads.iter().any(|p| {
            p.get("result").and_then(serde_json::Value::as_str) == Some("skipped")
                && p.get("reason")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|r| r.contains("hook time limit"))
        }),
        "{payloads:?}"
    );
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

/// I3 / review focus 5, the hook half: an unwritable journal must never
/// stop the hook from reporting a real finding, and must never need
/// `OSF_FILES_FROM`'s own list file to live under the state directory
/// (ruling R21: that list goes to the OS temp directory instead, so an
/// unwritable state dir cannot block writing it).
#[test]
fn an_unwritable_journal_still_refuses_a_real_finding_and_names_the_failure() {
    let repo = TempRepo::with_moon_workspace("pt-unwritable-state");
    repo.commit("base");
    repo.write("guide.md", "Do Phase 2 next.\n");
    let home = isolated_home("pt-unwritable-state");
    std::fs::write(home.join("blocked"), "x").expect("blocked file writes");
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
        &[(
            "OSF_STATE_DIR",
            home.join("blocked").to_str().expect("utf8"),
        )],
        &["hook", "post-tool"],
        &payload,
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("chat-local-reference"), "{out:?}");
    assert!(stderr.contains("journal"), "{out:?}");
    assert!(
        !home.join("blocked").join("files").exists(),
        "no files/ folder should exist under an unwritable state dir: {out:?}"
    );
}

/// The other half of the same case: a clean file still exits 0, and the
/// journal failure is still named on standard error rather than swallowed.
#[test]
fn an_unwritable_journal_with_a_clean_file_still_exits_zero_and_names_the_failure() {
    let repo = TempRepo::with_moon_workspace("pt-unwritable-state-clean");
    repo.commit("base");
    repo.write("guide.md", "Clean.\n");
    let home = isolated_home("pt-unwritable-state-clean");
    std::fs::write(home.join("blocked"), "x").expect("blocked file writes");
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
        &[(
            "OSF_STATE_DIR",
            home.join("blocked").to_str().expect("utf8"),
        )],
        &["hook", "post-tool"],
        &payload,
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("journal"), "{out:?}");
    assert!(
        !home.join("blocked").join("files").exists(),
        "no files/ folder should exist under an unwritable state dir: {out:?}"
    );
}

/// I6: input that cannot be parsed at all is a failure to run, so it is
/// refused rather than let through silently, the same way `osf hook stop`
/// already treats its own unreadable or non-JSON input.
#[test]
fn non_json_input_refuses_with_could_not_run_wording() {
    let repo = TempRepo::with_moon_workspace("pt-non-json");
    repo.commit("base");
    let home = isolated_home("pt-non-json");
    let out = run_osf_stdin(
        &repo.dir,
        &home,
        &[],
        &["hook", "post-tool"],
        "not json at all",
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("could not run"),
        "{out:?}"
    );
}

/// No git repository at all: the hook stands down at exit 0, no moon.
#[test]
fn a_path_with_no_repository_at_all_stands_down_rather_than_refusing() {
    let dir = std::env::temp_dir().join("osf-hook-post-tool-no-repo");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("plain dir creates");
    std::fs::write(dir.join("guide.md"), "Clean.\n").expect("fixture file writes");
    let home = isolated_home("pt-no-repo");
    let payload = format!(
        r#"{{"session_id":"s","tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        dir.join("guide.md").to_string_lossy().replace('\\', "/")
    );
    let out = run_osf_stdin(
        &dir,
        &home,
        &[("OSF_MOON", "/nonexistent/moon")],
        &["hook", "post-tool"],
        &payload,
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not adopted"),
        "{out:?}"
    );
}

/// A repository with neither file also stands down, no moon or journal.
#[test]
fn a_repository_that_never_adopted_osf_stands_down_for_the_hook() {
    let repo = TempRepo::new("pt-never-adopted");
    repo.write("guide.md", "Clean.\n");
    repo.commit("base");
    let home = isolated_home("pt-never-adopted");
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
        &[("OSF_MOON", "/nonexistent/moon")],
        &["hook", "post-tool"],
        &payload,
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not adopted"),
        "{out:?}"
    );
    assert!(
        !state(&home).join("buffer").exists(),
        "no journal should open for a repository that never adopted osf"
    );
}

/// `osf.toml` without `.osf/moon.yml` refuses at exit 2, naming the file.
#[test]
fn hook_post_tool_refuses_when_osf_toml_asks_for_osf_but_moon_yml_is_missing() {
    let repo = TempRepo::new("pt-adopted-but-broken");
    repo.write("osf.toml", "");
    repo.write("guide.md", "Clean.\n");
    repo.commit("base");
    let home = isolated_home("pt-adopted-but-broken");
    let payload = format!(
        r#"{{"session_id":"s","tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        repo.dir
            .join("guide.md")
            .to_string_lossy()
            .replace('\\', "/")
    );
    let out = run_osf_stdin(&repo.dir, &home, &[], &["hook", "post-tool"], &payload);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(".osf/moon.yml"),
        "{out:?}"
    );
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
