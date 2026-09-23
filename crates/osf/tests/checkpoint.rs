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

/// A base with no committed difference from `HEAD` at all (not merely one
/// with nothing a task reads) must still report nothing to check. Observed
/// live on this repository's own `.osf/moon.yml`: with a catch-all
/// `/**/*` input (as `scan` uses) and a genuinely empty `--stdin` file
/// list, moon runs every task instead of none, so this exact case must
/// never reach `moon::run` at all.
#[test]
fn a_base_with_no_committed_difference_at_all_is_nothing_to_check() {
    let repo = TempRepo::new("cp-zero-diff");
    repo.write(
        ".moon/workspace.yml",
        "projects:\n  osf: '.osf'\nvcs:\n  client: git\n  defaultBranch: main\n",
    );
    repo.write(
        ".osf/moon.yml",
        "tasks:\n  boom:\n    script: 'exit 1'\n    inputs: ['/**/*']\n    tags: [osf-pre-push]\n    options:\n      runFromWorkspaceRoot: true\n",
    );
    let head = repo.commit("base");
    let home = isolated_home("cp-zero-diff");
    let out = run_osf(
        &repo.dir,
        &home,
        &["verify", "--checkpoint", "pre-push", "--base", &head],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("nothing to check"),
        "{out:?}"
    );
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

/// Ruling R13: a SARIF left over from an earlier run must be cleared
/// before moon runs, so a task that fails without writing one is never
/// read through a stale file from a previous pass. This fixture's `boom`
/// task always fails and never writes SARIF; without the clearing step,
/// the stale file below would be misread as this run's one finding.
#[test]
fn a_stale_sarif_from_an_earlier_run_is_cleared_before_the_task_runs() {
    let repo = TempRepo::new("cp-stale-sarif");
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
    let stale_sarif = serde_json::json!({
        "runs": [{
            "results": [{"ruleId": "stale-rule", "message": {"text": "stale finding"}}]
        }]
    });
    let out_dir = repo.dir.join(".osf").join("out");
    std::fs::create_dir_all(&out_dir).expect("out dir creates");
    std::fs::write(
        out_dir.join("boom.sarif"),
        serde_json::to_string(&stale_sarif).expect("json renders"),
    )
    .expect("stale sarif writes");
    let home = isolated_home("cp-stale-sarif");
    let out = run_osf(
        &repo.dir,
        &home,
        &["verify", "--checkpoint", "pre-push", "--base", &base],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("findings unknown"), "{out:?}");
    assert!(!stdout.contains("1 finding"), "{out:?}");
}

/// Replaces a test deleted in an earlier task: `--gate` (the pull-request
/// checkpoint's own task) ignores a suppression marker and a config file's
/// exclude list, since a change under review must not be able to loosen
/// its own gate; the pre-push task, with neither flag, honours both.
#[test]
fn the_pull_request_checkpoint_ignores_a_suppression_marker_and_the_exclude_list() {
    let repo = TempRepo::new("cp-gate-ignores");
    repo.write(
        ".moon/workspace.yml",
        "projects:\n  osf: '.osf'\nvcs:\n  client: git\n  defaultBranch: main\n",
    );
    let bin = env!("CARGO_BIN_EXE_osf").replace('\\', "/");
    repo.write(
        ".osf/moon.yml",
        &format!(
            "language: rust\ntasks:\n  lint-writing:\n    command: '\"{bin}\" check lint-writing --sarif-out .osf/out/lint-writing.sarif'\n    inputs: ['/**/*.md']\n    tags: [osf-pre-push]\n    options:\n      runFromWorkspaceRoot: true\n      cache: false\n      shell: false\n  lint-writing-gate:\n    command: '\"{bin}\" check lint-writing --gate --sarif-out .osf/out/lint-writing-gate.sarif'\n    inputs: ['/**/*.md']\n    tags: [osf-pull-request]\n    options:\n      runFromWorkspaceRoot: true\n      cache: false\n      shell: false\n"
        ),
    );
    repo.write("osf.toml", "exclude = [\"notes.md\"]\n");
    let base = repo.commit("base");
    repo.write(
        "notes.md",
        "Fixed in #125 today. <!-- osf-disable-line bare-reference -- tracked -->\n",
    );
    repo.commit("dirty");
    let home = isolated_home("cp-gate-ignores");
    let pre_push = run_osf(
        &repo.dir,
        &home,
        &["verify", "--checkpoint", "pre-push", "--base", &base],
    );
    assert_eq!(pre_push.status.code(), Some(0), "{pre_push:?}");
    let pull_request = run_osf(
        &repo.dir,
        &home,
        &["verify", "--checkpoint", "pull-request", "--base", &base],
    );
    assert_eq!(pull_request.status.code(), Some(1), "{pull_request:?}");
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
