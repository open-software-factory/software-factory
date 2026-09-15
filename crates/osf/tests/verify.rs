//! Integration tests for `osf verify`: each stage exercised against a
//! throwaway git repository, including the difference between a stage that
//! ran and found nothing and a stage that could not run at all.

mod common;

use common::{isolated_home, run_osf, TempRepo};
use osf::config::Config;
use osf::verify::{run, Options, Stage};

fn opts<'a>(dir: &'a std::path::Path, config: &'a Config) -> Options<'a> {
    Options {
        dir,
        base: None,
        message_file: None,
        config,
    }
}

#[test]
fn pre_commit_with_nothing_staged_and_no_message_says_so_and_exits_clean() {
    let repo = TempRepo::new("pc-nothing");
    repo.write("a.md", "Clean.\n");
    repo.commit("add a file");

    let config = Config::default();
    let report = run(Stage::PreCommit, &opts(&repo.dir, &config)).expect("pre-commit runs");
    assert_eq!(report.total_errors(), 0);
    let summary = report.render_summary("pre-commit");
    assert!(summary.contains("scan: nothing to check"), "{summary}");
    assert!(
        summary.contains("lint writing (commit message): nothing to check"),
        "{summary}"
    );
    assert!(summary.contains("total: nothing to check"), "{summary}");
}

#[test]
fn pre_commit_scans_staged_content_not_the_working_tree() {
    let repo = TempRepo::new("pc-staged");
    repo.write("notes.md", "Clean for now.\n");
    repo.commit("add a clean file");
    repo.write(
        "notes.md",
        "See https://claude.ai/code/session_abc123 here.\n",
    );
    repo.stage("notes.md");

    let config = Config::default();
    let report = run(Stage::PreCommit, &opts(&repo.dir, &config)).expect("pre-commit runs");
    assert_eq!(
        report.total_errors(),
        1,
        "{}",
        report.render_summary("pre-commit")
    );
}

#[test]
fn pre_commit_lints_the_message_file_when_one_is_given() {
    let repo = TempRepo::new("pc-message");
    repo.write("a.md", "Clean.\n");
    repo.commit("add a file");

    let message_path = repo.dir.join("MSG");
    std::fs::write(&message_path, "Do Phase 2 next.\n").expect("message file writes");

    let config = Config::default();
    let options = Options {
        dir: &repo.dir,
        base: None,
        message_file: Some(&message_path),
        config: &config,
    };
    let report = run(Stage::PreCommit, &options).expect("pre-commit runs");
    assert_eq!(
        report.total_errors(),
        1,
        "{}",
        report.render_summary("pre-commit")
    );
}

/// A base commit, then a second commit ahead of it that trips every
/// pre-push check at once: a scanned file, a linted markdown file, a
/// linted skill folder, and a scanned commit message.
fn push_fixture(name: &str) -> (TempRepo, String) {
    let repo = TempRepo::new(name);
    repo.write("base.md", "The base commit is clean.\n");
    let base = repo.commit("base commit");

    repo.write(
        "leak.md",
        "See https://claude.ai/code/session_abc123 here.\n",
    );
    repo.write("guide.md", "Do Phase 2 next.\n");
    repo.write(
        "skills/demo/SKILL.md",
        "---\nname: demo\ndescription: Checks a folder for problems.\n---\n\n1. Run the check.\n",
    );
    repo.commit("Fix the bug.\n\nCo-Authored-By: Someone <someone@example.com>\n");

    (repo, base)
}

#[test]
fn pre_push_runs_every_check_against_the_base() {
    let (repo, base) = push_fixture("pp-every-check");
    let config = Config::default();
    let options = Options {
        dir: &repo.dir,
        base: Some(base),
        message_file: None,
        config: &config,
    };
    let report = run(Stage::PrePush, &options).expect("pre-push runs");
    let summary = report.render_summary("pre-push");
    assert!(report.total_errors() >= 4, "{summary}");
    let checks: Vec<&str> = report.findings().map(|(check, _, _)| check).collect();
    assert!(checks.contains(&"scan"), "{checks:?}");
    assert!(checks.contains(&"lint writing"), "{checks:?}");
    assert!(checks.contains(&"lint skill"), "{checks:?}");
    assert!(checks.contains(&"scan (commits)"), "{checks:?}");
}

#[test]
fn pre_push_with_no_changes_against_its_own_head_says_so() {
    let repo = TempRepo::new("pp-nothing");
    repo.write("base.md", "Clean.\n");
    let head = repo.commit("only commit");

    let config = Config::default();
    let options = Options {
        dir: &repo.dir,
        base: Some(head),
        message_file: None,
        config: &config,
    };
    let report = run(Stage::PrePush, &options).expect("pre-push runs");
    assert_eq!(report.total_errors(), 0);
    let summary = report.render_summary("pre-push");
    assert!(summary.contains("total: nothing to check"), "{summary}");
}

/// The same suppressed finding is silent at `pre-push` and loud at `ci`:
/// exactly the difference the `ci` stage exists to add.
#[test]
fn pre_push_honours_a_suppression_marker_that_ci_ignores() {
    let repo = TempRepo::new("suppression-difference");
    repo.write("base.md", "Clean.\n");
    let base = repo.commit("base commit");
    repo.write(
        "notes.md",
        "Fixed in #125 today. <!-- osf-disable-line bare-reference -- tracked -->\n",
    );
    repo.commit("add a suppressed finding");

    let config = Config::default();
    let options = Options {
        dir: &repo.dir,
        base: Some(base.clone()),
        message_file: None,
        config: &config,
    };
    let pre_push_report = run(Stage::PrePush, &options).expect("pre-push runs");
    assert_eq!(
        pre_push_report.total_errors(),
        0,
        "{}",
        pre_push_report.render_summary("pre-push")
    );

    let ci_report = run(Stage::Ci, &options).expect("ci runs");
    assert!(
        ci_report.total_errors() > 0,
        "{}",
        ci_report.render_summary("ci")
    );
}

#[test]
fn a_base_that_does_not_resolve_is_an_error_not_a_clean_report() {
    let repo = TempRepo::new("bad-base");
    repo.write("a.md", "Clean.\n");
    repo.commit("add a file");

    let config = Config::default();
    let options = Options {
        dir: &repo.dir,
        base: Some("not-a-real-ref".to_string()),
        message_file: None,
        config: &config,
    };
    let result = run(Stage::PrePush, &options);
    assert!(
        result.is_err(),
        "a base git cannot resolve must fail the run, not report zero errors"
    );
}

/// Exit code 0: `osf verify` ran every check and none of them found a problem.
#[test]
fn a_clean_pre_push_exits_zero() {
    let repo = TempRepo::new("cli-exit-clean");
    repo.write("a.md", "Clean.\n");
    let base = repo.commit("only commit");
    let home = isolated_home("cli-exit-clean");

    let output = run_osf(
        &repo.dir,
        &home,
        &["verify", "--stage", "pre-push", "--base", &base],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

/// Exit code 1: `osf verify` ran fine and found something to fix.
#[test]
fn a_dirty_pre_push_exits_one() {
    let repo = TempRepo::new("cli-exit-dirty");
    repo.write("base.md", "Clean.\n");
    let base = repo.commit("base commit");
    repo.write(
        "leak.md",
        "See https://claude.ai/code/session_abc123 here.\n",
    );
    repo.commit("add a leak");
    let home = isolated_home("cli-exit-dirty");

    let output = run_osf(
        &repo.dir,
        &home,
        &["verify", "--stage", "pre-push", "--base", &base],
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

/// Exit code 2: the tool itself could not run. Never confused with 0.
#[test]
fn a_pre_push_with_an_unresolvable_base_exits_two() {
    let repo = TempRepo::new("cli-exit-broken");
    repo.write("a.md", "Clean.\n");
    repo.commit("add a file");
    let home = isolated_home("cli-exit-broken");

    let output = run_osf(
        &repo.dir,
        &home,
        &["verify", "--stage", "pre-push", "--base", "not-a-real-ref"],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}
