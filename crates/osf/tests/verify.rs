//! Integration tests for `osf verify`: each stage exercised against a
//! throwaway git repository, including the difference between a stage that
//! ran and found nothing and a stage that could not run at all.

mod common;

use common::{coauthor_trailer, isolated_home, run_osf, session_link, TempRepo};
use osf::config::Config;
use osf::exclude::Excluder;
use osf::verify::{run, Options, Stage};

fn opts<'a>(dir: &'a std::path::Path, config: &'a Config, excluder: &'a Excluder) -> Options<'a> {
    Options {
        dir,
        base: None,
        message_file: None,
        config,
        excluder,
    }
}

#[test]
fn pre_commit_with_nothing_staged_and_no_message_says_so_and_exits_clean() {
    let repo = TempRepo::new("pc-nothing");
    repo.write("a.md", "Clean.\n");
    repo.commit("add a file");

    let config = Config::default();
    let excluder = Excluder::none();
    let report =
        run(Stage::PreCommit, &opts(&repo.dir, &config, &excluder)).expect("pre-commit runs");
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
        &format!("See {} here.\n", session_link("abc123")),
    );
    repo.stage("notes.md");

    let config = Config::default();
    let excluder = Excluder::none();
    let report =
        run(Stage::PreCommit, &opts(&repo.dir, &config, &excluder)).expect("pre-commit runs");
    assert_eq!(
        report.total_errors(),
        1,
        "{}",
        report.render_summary("pre-commit")
    );
}

/// A file modified, staged, then deleted from disk (the deletion itself
/// not staged) is still in the index: `pre-commit` must still find its
/// staged leak, never report the gate itself as unable to run.
#[test]
fn pre_commit_scans_staged_content_deleted_from_the_working_tree() {
    let repo = TempRepo::new("pc-staged-deleted");
    repo.write("notes.md", "Clean for now.\n");
    repo.commit("add a clean file");
    repo.write(
        "notes.md",
        &format!("See {} here.\n", session_link("abc123")),
    );
    repo.stage("notes.md");
    std::fs::remove_file(repo.dir.join("notes.md")).expect("file removes");

    let config = Config::default();
    let excluder = Excluder::none();
    let report =
        run(Stage::PreCommit, &opts(&repo.dir, &config, &excluder)).expect("pre-commit runs");
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
    let excluder = Excluder::none();
    let options = Options {
        dir: &repo.dir,
        base: None,
        message_file: Some(&message_path),
        config: &config,
        excluder: &excluder,
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
        &format!("See {} here.\n", session_link("abc123")),
    );
    repo.write("guide.md", "Do Phase 2 next.\n");
    repo.write(
        "skills/demo/SKILL.md",
        "---\nname: demo\ndescription: Checks a folder for problems.\n---\n\n1. Run the check.\n",
    );
    repo.commit(&format!(
        "Fix the bug.\n\n{}\n",
        coauthor_trailer("Someone", "someone@example.com")
    ));

    (repo, base)
}

#[test]
fn pre_push_runs_every_check_against_the_base() {
    let (repo, base) = push_fixture("pp-every-check");
    let config = Config::default();
    let excluder = Excluder::none();
    let options = Options {
        dir: &repo.dir,
        base: Some(base),
        message_file: None,
        config: &config,
        excluder: &excluder,
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
    let excluder = Excluder::none();
    let options = Options {
        dir: &repo.dir,
        base: Some(head),
        message_file: None,
        config: &config,
        excluder: &excluder,
    };
    let report = run(Stage::PrePush, &options).expect("pre-push runs");
    assert_eq!(report.total_errors(), 0);
    let summary = report.render_summary("pre-push");
    assert!(summary.contains("total: nothing to check"), "{summary}");
}

/// The compiled default exclude list is `target/**` only, so a changed file
/// under a vendor path is not excluded unless the caller asks for it.
#[test]
fn a_changed_path_matching_the_excluder_is_dropped_and_counted() {
    let (repo, base) = push_fixture("pp-exclude");

    let config = Config::default();
    let excluder = Excluder::build(&["leak.md".to_string()]).expect("one pattern builds");
    let options = Options {
        dir: &repo.dir,
        base: Some(base),
        message_file: None,
        config: &config,
        excluder: &excluder,
    };
    let report = run(Stage::PrePush, &options).expect("pre-push runs");
    let summary = report.render_summary("pre-push");
    assert!(summary.contains("scan: 0 error(s)"), "{summary}");
    assert!(summary.contains("1 excluded"), "{summary}");
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
    let excluder = Excluder::none();
    let options = Options {
        dir: &repo.dir,
        base: Some(base.clone()),
        message_file: None,
        config: &config,
        excluder: &excluder,
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
    let excluder = Excluder::none();
    let options = Options {
        dir: &repo.dir,
        base: Some("not-a-real-ref".to_string()),
        message_file: None,
        config: &config,
        excluder: &excluder,
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
        &format!("See {} here.\n", session_link("abc123")),
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

/// `--gate` forces the exclude list back to the compiled defaults, ignoring
/// a config file's own exclude setting, so a change under review cannot
/// loosen the check it is being checked against.
#[test]
fn gate_ignores_a_config_files_exclude_list() {
    let repo = TempRepo::new("verify-gate-ignores-config-file");
    repo.write("base.md", "Clean.\n");
    let base = repo.commit("base commit");
    repo.write(
        "leak.md",
        &format!("See {} here.\n", session_link("abc123")),
    );
    repo.write("osf.toml", "exclude = [\"leak.md\"]\n");
    repo.commit("add a leak and a loosened config");
    let home = isolated_home("verify-gate-ignores-config-file");

    let without_gate = run_osf(
        &repo.dir,
        &home,
        &[
            "--config", "osf.toml", "verify", "--stage", "pre-push", "--base", &base,
        ],
    );
    assert_eq!(without_gate.status.code(), Some(0), "{without_gate:?}");

    let with_gate = run_osf(
        &repo.dir,
        &home,
        &[
            "--config", "osf.toml", "verify", "--stage", "pre-push", "--base", &base, "--gate",
        ],
    );
    assert_eq!(with_gate.status.code(), Some(1), "{with_gate:?}");
}

/// A deliberately bad writing fixture under `tests/fixtures` is checked
/// against its own `osf-expect` declaration here, the same way the
/// `osf lint writing` command line already treats one and the same way a
/// skill fixture already is. Without this, adding a fixture for a new
/// writing rule would fail every later `ci` run on this repository.
#[test]
fn a_matching_writing_fixture_declaration_produces_no_findings() {
    let repo = TempRepo::new("writing-fixture-declared");
    repo.write("base.md", "Clean.\n");
    let base = repo.commit("base commit");
    repo.write(
        "crates/osf/tests/fixtures/writing/demo.md",
        "A thing — another thing.\n\n<!-- osf-expect\nem-dash\n-->\n",
    );
    repo.commit("add a declared writing fixture");

    let config = Config::default();
    let excluder = Excluder::none();
    let options = Options {
        dir: &repo.dir,
        base: Some(base),
        message_file: None,
        config: &config,
        excluder: &excluder,
    };
    let report = run(Stage::PrePush, &options).expect("pre-push runs");
    assert_eq!(
        report.total_errors(),
        0,
        "{}",
        report.render_summary("pre-push")
    );
}

/// The mirror case: a writing fixture that no longer produces a rule it
/// declares is a failure here too, the alarm for a rule that silently
/// stopped firing.
#[test]
fn a_writing_fixture_missing_a_declared_rule_fails() {
    let repo = TempRepo::new("writing-fixture-missing");
    repo.write("base.md", "Clean.\n");
    let base = repo.commit("base commit");
    repo.write(
        "crates/osf/tests/fixtures/writing/demo.md",
        "Nothing wrong here.\n\n<!-- osf-expect\nem-dash\n-->\n",
    );
    repo.commit("add a fixture missing its declared rule");

    let config = Config::default();
    let excluder = Excluder::none();
    let options = Options {
        dir: &repo.dir,
        base: Some(base),
        message_file: None,
        config: &config,
        excluder: &excluder,
    };
    let report = run(Stage::PrePush, &options).expect("pre-push runs");
    let summary = report.render_summary("pre-push");
    assert_eq!(report.total_errors(), 1, "{summary}");
    let rules: Vec<&str> = report.findings().map(|(_, _, f)| f.rule).collect();
    assert!(rules.contains(&"expectation-missing"), "{rules:?}");
}
