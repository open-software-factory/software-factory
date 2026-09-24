//! Integration tests for `osf verify --checkpoint`: moon required on `PATH`.

mod common;
use common::{
    isolated_home, run_osf, run_osf_with_env, session_link, write_fake_moon,
    write_fake_moon_report, BareRepo, TempRepo,
};

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

/// Moved from `tests/verify.rs` (M5): nothing staged is nothing to check,
/// never a silent pass mistaken for a clean run.
#[test]
fn pre_commit_with_nothing_staged_is_nothing_to_check() {
    let repo = TempRepo::with_moon_workspace("cp-pc-nothing");
    repo.write("a.md", "Clean.\n");
    repo.commit("add a file");
    let home = isolated_home("cp-pc-nothing");
    let out = run_osf(&repo.dir, &home, &["verify", "--checkpoint", "pre-commit"]);
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
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("staged with unstaged edits, so both versions were checked: notes.md"),
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
    assert!(
        reason.contains("no findings file: .osf/out/boom.sarif"),
        "{reason}"
    );
}

/// M2: a suppressed result never counts toward a task's finding total, and
/// `error_findings` (what the hook or the pull-request checkpoint's own
/// refusal text shows) lists error-level results only, so a warning never
/// reads back as a reason to refuse.
#[test]
fn a_suppressed_error_is_excluded_from_the_count_and_the_refusal_text() {
    let repo = TempRepo::new("cp-suppressed");
    repo.write(
        ".moon/workspace.yml",
        "projects:\n  osf: '.osf'\nvcs:\n  client: git\n  defaultBranch: main\n",
    );
    // Base64 of a SARIF holding one suppressed error and one (unsuppressed)
    // warning, so the task can write it with no quoting trouble in this
    // YAML scalar: {"runs":[{"results":[
    //   {"ruleId":"suppressed-rule","level":"error",
    //    "message":{"text":"suppressed message"},
    //    "suppressions":[{"kind":"inSource"}]},
    //   {"ruleId":"warn-rule","level":"warning",
    //    "message":{"text":"warn message"}}]}]}
    let payload = "eyJydW5zIjpbeyJyZXN1bHRzIjpbeyJydWxlSWQiOiJzdXBwcmVzc2VkLXJ1bGUiLCJsZXZlbCI6ImVycm9yIiwibWVzc2FnZSI6eyJ0ZXh0Ijoic3VwcHJlc3NlZCBtZXNzYWdlIn0sInN1cHByZXNzaW9ucyI6W3sia2luZCI6ImluU291cmNlIn1dfSx7InJ1bGVJZCI6Indhcm4tcnVsZSIsImxldmVsIjoid2FybmluZyIsIm1lc3NhZ2UiOnsidGV4dCI6Indhcm4gbWVzc2FnZSJ9fV19XX0=";
    let script = if cfg!(windows) {
        format!(
            "New-Item -ItemType Directory -Force .osf/out | Out-Null; [IO.File]::WriteAllText(\".osf/out/boom.sarif\", [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String(\"{payload}\"))); exit 1"
        )
    } else {
        format!("mkdir -p .osf/out && echo {payload} | base64 -d > .osf/out/boom.sarif && exit 1")
    };
    repo.write(
        ".osf/moon.yml",
        &format!(
            "tasks:\n  boom:\n    script: '{script}'\n    inputs: ['/**/*.md']\n    tags: [osf-pre-push]\n    options:\n      runFromWorkspaceRoot: true\n"
        ),
    );
    let base = repo.commit("base");
    repo.write("guide.md", "Hello.\n");
    repo.commit("dirty");
    let home = isolated_home("cp-suppressed");
    let out = run_osf(
        &repo.dir,
        &home,
        &["verify", "--checkpoint", "pre-push", "--base", &base],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("1 finding(s)"), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("suppressed-rule"), "{out:?}");
    assert!(!stderr.contains("suppressed message"), "{out:?}");
    assert!(!stderr.contains("warn-rule"), "{out:?}");
}

/// Ruling R16: both of moon's own captured streams must reach stderr under
/// their own cap, since a real failure (cargo's failing test names on
/// stdout, its "test failed" summary on stderr) can put what an agent needs
/// on either one. This fixture's task writes one stdout marker, then a
/// stderr flood past the 80-line cap: a single combined-and-capped tail
/// would drop the stdout marker entirely, since the stderr flood alone
/// already fills the cap.
#[test]
fn a_failed_task_s_stdout_and_stderr_are_both_printed() {
    let repo = TempRepo::new("cp-task-output");
    repo.write(
        ".moon/workspace.yml",
        "projects:\n  osf: '.osf'\nvcs:\n  client: git\n  defaultBranch: main\n",
    );
    // PowerShell has no `1>&2` redirection (the operator is reserved); a
    // POSIX shell has no `[Console]::Error.WriteLine`.
    let script = if cfg!(windows) {
        "Write-Output ''OSF_STDOUT_MARKER_7f3a1''; 1..100 | ForEach-Object { [Console]::Error.WriteLine(\"filler line $_\") }; exit 1"
    } else {
        "echo OSF_STDOUT_MARKER_7f3a1; for i in $(seq 1 100); do echo \"filler line $i\" 1>&2; done; exit 1"
    };
    repo.write(
        ".osf/moon.yml",
        &format!(
            "tasks:\n  boom:\n    script: '{script}'\n    inputs: ['/**/*.md']\n    tags: [osf-pre-push]\n    options:\n      runFromWorkspaceRoot: true\n"
        ),
    );
    let base = repo.commit("base");
    repo.write("guide.md", "Hello.\n");
    repo.commit("dirty");
    let home = isolated_home("cp-task-output");
    let out = run_osf(
        &repo.dir,
        &home,
        &["verify", "--checkpoint", "pre-push", "--base", &base],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("OSF_STDOUT_MARKER_7f3a1"),
        "stdout marker missing under a large stderr: {out:?}"
    );
    assert!(stderr.contains("filler line 100"), "{out:?}");
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

/// C1: `.osf/moon.yml`'s own `scan-commits` task had no `inputs` at all,
/// so moon's own default (`**/*`, relative to the `.osf/` project) made it
/// affected only by a change under `.osf/` — never by an ordinary changed
/// file. This documents that default directly: a task with an explicit
/// `inputs: ['/**/*']` runs for a change that touches only `docs/x.txt`;
/// a task with no `inputs` at all does not.
#[test]
fn a_task_with_no_inputs_is_affected_only_by_a_change_under_its_own_project() {
    let repo = TempRepo::new("cp-inputs-default");
    repo.write(
        ".moon/workspace.yml",
        "projects:\n  osf: '.osf'\nvcs:\n  client: git\n  defaultBranch: main\n",
    );
    repo.write(
        ".osf/moon.yml",
        "tasks:\n  wide:\n    script: 'exit 0'\n    inputs: ['/**/*']\n    tags: [osf-pre-push]\n    options:\n      runFromWorkspaceRoot: true\n  narrow-default:\n    script: 'exit 0'\n    tags: [osf-pre-push]\n    options:\n      runFromWorkspaceRoot: true\n",
    );
    let base = repo.commit("base");
    repo.write("docs/x.txt", "hello\n");
    repo.commit("touch an unrelated doc file");
    let home = isolated_home("cp-inputs-default");
    let out = run_osf(
        &repo.dir,
        &home,
        &["verify", "--checkpoint", "pre-push", "--base", &base],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("osf:wide"), "{out:?}");
    assert!(!stdout.contains("osf:narrow-default"), "{out:?}");
}

/// I5: `lint-skill-gate`, the pull-request checkpoint's own task, ignores
/// a config file's exclude list, the same way `lint-writing-gate` and
/// `scan-gate` already do; `lint-skill`, the pre-push task, honours it.
#[test]
fn the_pull_request_checkpoint_s_lint_skill_gate_ignores_the_exclude_list() {
    let repo = TempRepo::new("cp-skill-gate");
    repo.write(
        ".moon/workspace.yml",
        "projects:\n  osf: '.osf'\nvcs:\n  client: git\n  defaultBranch: main\n",
    );
    let bin = env!("CARGO_BIN_EXE_osf").replace('\\', "/");
    repo.write(
        ".osf/moon.yml",
        &format!(
            "language: rust\ntasks:\n  lint-skill:\n    command: '\"{bin}\" check lint-skill --sarif-out .osf/out/lint-skill.sarif'\n    inputs: ['/**/SKILL.md', '/**/skills/**/*']\n    tags: [osf-pre-push]\n    options:\n      runFromWorkspaceRoot: true\n      cache: false\n      shell: false\n  lint-skill-gate:\n    command: '\"{bin}\" check lint-skill --gate --sarif-out .osf/out/lint-skill-gate.sarif'\n    inputs: ['/**/SKILL.md', '/**/skills/**/*']\n    tags: [osf-pull-request]\n    options:\n      runFromWorkspaceRoot: true\n      cache: false\n      shell: false\n"
        ),
    );
    repo.write("osf.toml", "exclude = [\"skills/demo\"]\n");
    let base = repo.commit("base");
    repo.write(
        "skills/demo/SKILL.md",
        "---\nname: demo\ndescription: Checks a folder for problems.\n---\n\n1. Run the check.\n",
    );
    repo.commit("dirty");
    let home = isolated_home("cp-skill-gate");
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

/// I2: when moon itself cannot run, the checkpoint still journals one
/// verification event per task it was about to run, with the moon error
/// as the reason, rather than leaving the failure silent in the journal.
#[test]
fn a_missing_moon_journals_a_could_not_run_verification_per_task() {
    let repo = TempRepo::with_moon_workspace("cp-missing-moon");
    let base = repo.commit("base");
    repo.write("a.md", "Hello.\n");
    repo.commit("dirty");
    let home = isolated_home("cp-missing-moon");
    let out = run_osf_with_env(
        &repo.dir,
        &home,
        &[("OSF_MOON", "/nonexistent/moon")],
        &["verify", "--checkpoint", "pre-push", "--base", &base],
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
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
    let is_verification = |e: &&serde_json::Value| {
        e.get("event_type").and_then(serde_json::Value::as_str) == Some("verification")
    };
    let verifications: Vec<&serde_json::Value> = lines.iter().filter(is_verification).collect();
    assert!(!verifications.is_empty(), "{lines:?}");
    assert!(
        verifications.iter().all(|v| v
            .get("payload")
            .and_then(|p| p.get("result"))
            .and_then(serde_json::Value::as_str)
            == Some("could-not-run")),
        "{lines:?}"
    );
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

/// Bullet 1: every selected task came back skipped, so this is could-not-run.
#[test]
fn an_all_skipped_run_at_pre_push_is_could_not_run_and_names_the_reason() {
    let repo = TempRepo::new("cp-all-skipped");
    repo.write(".osf/moon.yml", "tasks: {}\n");
    repo.write("README.md", "init\n");
    let base = repo.commit("base");
    repo.write("guide.md", "Hello.\n");
    repo.commit("dirty");
    let moon = write_fake_moon(&repo);
    write_fake_moon_report(
        &repo,
        r#""app:one":{"state":"skipped"},"app:two":{"state":"skipped"}"#,
    );
    let home = isolated_home("cp-all-skipped");
    let out = run_osf_with_env(
        &repo.dir,
        &home,
        &[("OSF_MOON", moon.to_str().expect("utf8 path"))],
        &["verify", "--checkpoint", "pre-push", "--base", &base],
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("could not run"),
        "{out:?}"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("every selected task was skipped"),
        "{out:?}"
    );
}

/// Bullet 3: moon's own `invalid` status is could-not-run even amid passes.
#[test]
fn a_task_with_moon_s_invalid_status_is_could_not_run() {
    let repo = TempRepo::new("cp-invalid-task");
    repo.write(".osf/moon.yml", "tasks: {}\n");
    repo.write("README.md", "init\n");
    let base = repo.commit("base");
    repo.write("guide.md", "Hello.\n");
    repo.commit("dirty");
    let moon = write_fake_moon(&repo);
    write_fake_moon_report(
        &repo,
        r#""app:one":{"state":"passed","hash":"abc"},"app:two":{"state":"invalid"}"#,
    );
    let home = isolated_home("cp-invalid-task");
    let out = run_osf_with_env(
        &repo.dir,
        &home,
        &[("OSF_MOON", moon.to_str().expect("utf8 path"))],
        &["verify", "--checkpoint", "pull-request", "--base", &base],
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("invalid status for: app:two"),
        "{out:?}"
    );
}

/// No git repository at all: `osf verify` stands down at exit 0, no moon.
#[test]
fn verify_stands_down_with_no_git_repository_at_all() {
    let dir = std::env::temp_dir().join("osf-verify-test-no-repo-at-all");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("plain dir creates");
    let home = isolated_home("verify-no-repo-at-all");
    let out = run_osf_with_env(
        &dir,
        &home,
        &[("OSF_MOON", "/nonexistent/moon")],
        &["verify", "--checkpoint", "hook"],
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("not adopted"),
        "{out:?}"
    );
    assert!(
        !state(&home).join("buffer").exists(),
        "no journal should open for a folder that never adopted osf"
    );
}

/// A repository with neither file has also not adopted osf.
#[test]
fn verify_stands_down_in_a_repository_that_never_adopted_osf() {
    let repo = TempRepo::new("verify-never-adopted");
    repo.write("README.md", "init\n");
    repo.commit("base");
    let home = isolated_home("verify-never-adopted");
    let out = run_osf_with_env(
        &repo.dir,
        &home,
        &[("OSF_MOON", "/nonexistent/moon")],
        &["verify", "--checkpoint", "pre-commit"],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("not adopted"),
        "{out:?}"
    );
}

/// `osf.toml` without `.osf/moon.yml` refuses at exit 2, naming the file.
#[test]
fn verify_refuses_when_osf_toml_asks_for_osf_but_moon_yml_is_missing() {
    let repo = TempRepo::new("verify-adopted-but-broken");
    repo.write("osf.toml", "");
    repo.commit("base");
    let home = isolated_home("verify-adopted-but-broken");
    let out = run_osf(&repo.dir, &home, &["verify", "--checkpoint", "pre-commit"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains(".osf/moon.yml"),
        "{out:?}"
    );
}

/// A bare repository is could-not-run, never not-adopted: it is a git
/// repository, just one with no work tree for osf to check.
#[test]
fn verify_refuses_with_a_bare_repository() {
    let bare = BareRepo::new("verify-bare-repo");
    let home = isolated_home("verify-bare-repo");
    let out = run_osf(&bare.dir, &home, &["verify", "--checkpoint", "pre-commit"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("not adopted"),
        "{out:?}"
    );
}

/// When git itself cannot start, that is also could-not-run, never
/// not-adopted. `PATH` here names a directory with no `git` binary, set
/// only on this one child process.
#[test]
fn verify_refuses_when_git_cannot_run() {
    let repo = TempRepo::new("verify-git-cannot-run");
    repo.write("README.md", "init\n");
    repo.commit("base");
    let home = isolated_home("verify-git-cannot-run");
    let empty_path = std::env::temp_dir().join("osf-verify-test-empty-path");
    std::fs::create_dir_all(&empty_path).expect("empty PATH dir creates");
    let out = run_osf_with_env(
        &repo.dir,
        &home,
        &[("PATH", empty_path.to_str().expect("utf8 path"))],
        &["verify", "--checkpoint", "pre-commit"],
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
}

/// Bullet 2: a run report naming no task at all is could-not-run.
#[test]
fn a_run_report_with_no_task_at_all_is_could_not_run() {
    let repo = TempRepo::new("cp-empty-report");
    repo.write(".osf/moon.yml", "tasks: {}\n");
    repo.write("README.md", "init\n");
    let base = repo.commit("base");
    repo.write("guide.md", "Hello.\n");
    repo.commit("dirty");
    let moon = write_fake_moon(&repo);
    write_fake_moon_report(&repo, "");
    let home = isolated_home("cp-empty-report");
    let out = run_osf_with_env(
        &repo.dir,
        &home,
        &[("OSF_MOON", moon.to_str().expect("utf8 path"))],
        &["verify", "--checkpoint", "schedule", "--base", &base],
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("no task"),
        "{out:?}"
    );
}
