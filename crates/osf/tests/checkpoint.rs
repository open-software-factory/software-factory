//! Integration tests for `osf verify --checkpoint`: moon required on `PATH`.

mod common;
use common::{isolated_home, run_osf, run_osf_with_env, session_link, TempRepo};

fn state(home: &std::path::Path) -> std::path::PathBuf {
    home.join(".osf").join("state")
}

#[test]
fn pre_push_runs_the_tagged_tasks_and_writes_one_event_per_task_plus_one() {
    let repo = TempRepo::with_moon_workspace("cp-pre-push");
    let base = repo.commit("base");
    repo.write("guide.md", "Do Phase 2 next.\n");
    repo.commit("dirty");
    let home = isolated_home("cp-pre-push");
    let out = run_osf(
        &repo.dir,
        &home,
        &["verify", "--checkpoint", "pre-push", "--base", &base],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let buffer = std::fs::read_dir(state(&home).join("buffer"))
        .expect("buffer")
        .next()
        .expect("one file")
        .expect("entry")
        .path();
    let lines: Vec<serde_json::Value> = std::fs::read_to_string(buffer)
        .expect("read")
        .lines()
        .map(|l| serde_json::from_str(l).expect("json"))
        .collect();
    assert_eq!(
        lines
            .iter()
            .filter(|e| e["event_type"] == "verification")
            .count(),
        2
    );
    assert_eq!(
        lines.last().expect("last")["event_type"],
        "checkpoint-complete"
    );
}

#[test]
fn a_change_no_task_reads_is_nothing_to_check_and_exits_zero() {
    let repo = TempRepo::with_moon_workspace("cp-nothing");
    let base = repo.commit("base");
    repo.write_bytes("logo.png", &[0x89, b'P', b'N', b'G']);
    repo.commit("image only");
    let home = isolated_home("cp-nothing");
    let out = run_osf(
        &repo.dir,
        &home,
        &["verify", "--checkpoint", "pre-push", "--base", &base],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("nothing to check"));
}

#[test]
fn a_staged_file_with_unstaged_edits_is_named_on_stderr() {
    let repo = TempRepo::with_moon_workspace("cp-partial");
    repo.commit("base");
    repo.write("notes.md", "Staged text.\n");
    repo.stage("notes.md");
    repo.write("notes.md", "Different working text.\n");
    let home = isolated_home("cp-partial");
    let out = run_osf(&repo.dir, &home, &["verify", "--checkpoint", "pre-commit"]);
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("notes.md"),
        "{out:?}"
    );
}

#[test]
fn an_unwritable_journal_fails_pre_push_with_exit_two() {
    let repo = TempRepo::with_moon_workspace("cp-journal");
    let base = repo.commit("base");
    repo.write("a.md", "Clean.\n");
    repo.commit("clean");
    let home = isolated_home("cp-journal");
    std::fs::write(home.join("blocked"), "x").expect("file");
    let out = run_osf_with_env(
        &repo.dir,
        &home,
        &[(
            "OSF_STATE_DIR",
            home.join("blocked").to_str().expect("utf8"),
        )],
        &["verify", "--checkpoint", "pre-push", "--base", &base],
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
}

#[test]
fn the_old_stage_flag_still_works() {
    let repo = TempRepo::with_moon_workspace("cp-alias");
    let base = repo.commit("base");
    repo.write("a.md", "Clean.\n");
    repo.commit("clean");
    let home = isolated_home("cp-alias");
    let out = run_osf(
        &repo.dir,
        &home,
        &["verify", "--stage", "pre-push", "--base", &base],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

#[test]
fn a_leak_in_a_session_link_fails_pre_commit() {
    let repo = TempRepo::with_moon_workspace("cp-leak");
    repo.commit("base");
    repo.write(
        "leak.md",
        &format!("See {} here.\n", session_link("abc123")),
    );
    repo.stage("leak.md");
    let home = isolated_home("cp-leak");
    let out = run_osf(&repo.dir, &home, &["verify", "--checkpoint", "pre-commit"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
}

/// Ruling R12: a task that failed and left no readable SARIF has unknown
/// findings, never a silent zero. This fixture defines its own single
/// task, tagged `osf-pre-push`, that fails without ever writing SARIF.
#[test]
fn a_failed_task_with_no_sarif_reports_unknown_findings() {
    let repo = TempRepo::new("cp-no-sarif");
    repo.write(
        ".moon/workspace.yml",
        "projects:\n  osf: '.osf'\nvcs:\n  client: git\n  defaultBranch: main\n",
    );
    repo.write(
        ".osf/moon.yml",
        "tasks:\n  boom:\n    script: 'exit 1'\n    inputs: ['/**/*.md']\n    tags: [osf-pre-push]\n    options:\n      runFromWorkspaceRoot: true\n",
    );
    let base = repo.commit("base");
    repo.write("guide.md", "Hello.\n");
    repo.commit("dirty");
    let home = isolated_home("cp-no-sarif");
    let out = run_osf(
        &repo.dir,
        &home,
        &["verify", "--checkpoint", "pre-push", "--base", &base],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("findings unknown"),
        "{out:?}"
    );
    let buffer = std::fs::read_dir(state(&home).join("buffer"))
        .expect("buffer")
        .next()
        .expect("one file")
        .expect("entry")
        .path();
    let lines: Vec<serde_json::Value> = std::fs::read_to_string(buffer)
        .expect("read")
        .lines()
        .map(|l| serde_json::from_str(l).expect("json"))
        .collect();
    let verification = lines
        .iter()
        .find(|e| e["event_type"] == "verification")
        .expect("a verification event");
    let reason = verification
        .get("payload")
        .and_then(|p| p.get("reason"))
        .and_then(serde_json::Value::as_str)
        .expect("reason is a string");
    assert!(reason.contains("no findings file"), "{reason}");
}

/// Moved from `tests/verify.rs`: an unresolvable base is a failure to run,
/// never a clean pass, in the checkpoint form too.
#[test]
fn a_pre_push_with_an_unresolvable_base_exits_two() {
    let repo = TempRepo::with_moon_workspace("cp-bad-base");
    repo.commit("base");
    let home = isolated_home("cp-bad-base");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "verify",
            "--checkpoint",
            "pre-push",
            "--base",
            "not-a-real-ref",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
}
