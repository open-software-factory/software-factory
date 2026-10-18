//! The commit-message rules `CONTRIBUTING.md` states that are decided from
//! a message's structure: no full stop ends the subject, the subject
//! exists, and a reference names its repository and says what it is. Each
//! rule has a passing and a failing message, checked on the two paths that
//! enforce it: the writing lint the `commit-msg` hook runs, and the commit
//! scan that the pre-push checkpoint and the pull request check run.
//!
//! The present tense of a subject and a message that stands on its own are
//! not decided from structure, so no test here covers them.

mod common;

use common::{isolated_home, run_osf, TempRepo};
use osf::config::WritingConfig;
use osf::lints::writing::{lint_writing, COMMIT_GATE_RULES};
use osf::lints::{load_known_names, Context};
use osf::scan::{scan_commits, Rules};

/// A message with this subject ends in one of the four full stops the rule
/// reads: the ASCII, ideographic, fullwidth and halfwidth ones.
const FULL_STOPS: [char; 4] = ['.', '\u{3002}', '\u{FF0E}', '\u{FF61}'];

const PROJECT: &str = "open-software-factory/software-factory";

/// The rule ids the `commit-msg` hook's lint reports for `message`.
fn hook_rules(message: &str) -> Vec<&'static str> {
    let known = load_known_names(&[], None).expect("built-in names load");
    lint_writing(
        message,
        &known,
        &WritingConfig::default(),
        Context::Commit,
        false,
        false,
    )
    .into_iter()
    .map(|f| f.rule)
    .collect()
}

/// The rule ids the commit scan reports for a commit holding `message`.
fn scan_rules_for(name: &str, message: &str) -> Vec<&'static str> {
    let repo = TempRepo::new(name);
    repo.write("a.txt", "one\n");
    let base = repo.commit("Add the first file");
    repo.write("b.txt", "two\n");
    repo.git(&["add", "-A"]);
    repo.git(&[
        "commit",
        "-q",
        "--allow-empty-message",
        "--cleanup=verbatim",
        "-m",
        message,
    ]);
    let rules = Rules::build(&repo.dir, &osf::config::ScanConfig::default()).expect("rules build");
    scan_commits(&repo.dir, &format!("{base}..HEAD"), &rules)
        .expect("commit scan runs")
        .into_iter()
        .flat_map(|(_, findings)| findings.into_iter().map(|f| f.rule))
        .collect()
}

/// Asserts `rule` is reported for `message` by both paths.
fn assert_flagged(name: &str, message: &str, rule: &str) {
    assert!(hook_rules(message).contains(&rule), "hook: {message:?}");
    assert!(
        scan_rules_for(name, message).contains(&rule),
        "scan: {message:?}"
    );
}

/// Asserts `rule` is reported for `message` by neither path.
fn assert_clean(name: &str, message: &str, rule: &str) {
    assert!(!hook_rules(message).contains(&rule), "hook: {message:?}");
    assert!(
        !scan_rules_for(name, message).contains(&rule),
        "scan: {message:?}"
    );
}

#[test]
fn a_subject_ending_in_any_full_stop_is_flagged() {
    for (index, stop) in FULL_STOPS.iter().enumerate() {
        let message = format!("Add a status subcommand{stop}\n\nThe body explains why.\n");
        assert_flagged(
            &format!("stop-{index}"),
            &message,
            "commit-subject-full-stop",
        );
    }
}

#[test]
fn a_subject_without_a_full_stop_passes() {
    let message = "Add a status subcommand\n\nThe body ends with a stop.\n";
    assert_clean("no-stop", message, "commit-subject-full-stop");
}

#[test]
fn a_full_stop_in_the_body_is_not_the_subjects() {
    let message = "Add a status subcommand\n\nThe subject above has none. This body has.\n";
    assert_clean("body-stop", message, "commit-subject-full-stop");
}

#[test]
fn the_subject_is_the_first_line_that_is_not_a_comment() {
    let message = "# a comment git strips\n\nAdd a status subcommand.\n";
    assert!(hook_rules(message).contains(&"commit-subject-full-stop"));
    let clean = "# a comment git strips.\nAdd a status subcommand\n";
    assert!(!hook_rules(clean).contains(&"commit-subject-full-stop"));
}

#[test]
fn a_bare_number_is_flagged_and_a_named_reference_passes() {
    assert_flagged(
        "bare",
        "Fix the parser\n\nFixes #12 in the parser.\n",
        "bare-reference",
    );
    assert_flagged("bare-subject", "Fix the parser (#12)\n", "bare-reference");
    let named = format!("Fix the parser\n\nSee {PROJECT}#12 (the crash) for it.\n");
    assert_clean("named", &named, "bare-reference");
}

#[test]
fn a_reference_without_a_label_is_flagged_and_a_labelled_one_passes() {
    let unlabelled = format!("Fix the parser\n\nSee {PROJECT}#12 for the crash.\n");
    assert_flagged("unlabelled", &unlabelled, "reference-without-label");
    let labelled = format!("Fix the parser\n\nSee {PROJECT}#12 (the crash) for it.\n");
    assert_clean("labelled", &labelled, "reference-without-label");
}

#[test]
fn a_message_the_forge_generates_passes() {
    let merge = format!("Merge pull request #272 from {PROJECT}/fix/review-fails-open\n");
    let found = scan_rules_for("forge-merge", &merge);
    assert!(
        found.iter().all(|rule| !COMMIT_GATE_RULES.contains(rule)),
        "{found:?}"
    );
    assert!(hook_rules(&merge).is_empty(), "{merge:?}");
}

#[test]
fn a_dependency_bot_message_passes() {
    let bump = "Bump serde_json from 1.0.151 to 1.0.154\n\n\
        Bumps [serde_json](https://example.com/serde-rs/json) from 1.0.151 to 1.0.154.\n\
        - [Release notes](https://example.com/serde-rs/json/releases)\n\n\
        ---\nupdated-dependencies:\n- dependency-name: serde_json\n...\n\n\
        Signed-off-by: dependabot[bot] <support@github.com>\n";
    let found = scan_rules_for("bot", bump);
    assert!(
        found.iter().all(|rule| !COMMIT_GATE_RULES.contains(rule)),
        "{found:?}"
    );
    assert_eq!(hook_rules(bump), Vec::<&str>::new());
}

#[test]
fn the_scan_command_fails_a_range_with_a_bad_subject_and_passes_a_good_one() {
    let repo = TempRepo::new("scan-exit");
    repo.write("a.txt", "one\n");
    let base = repo.commit("Add the first file");
    let home = isolated_home("scan-exit");
    let range = format!("{base}..HEAD");

    repo.write("b.txt", "two\n");
    repo.commit("Add the second file");
    let clean = run_osf(&repo.dir, &home, &["scan", "--commits", &range]);
    assert_eq!(clean.status.code(), Some(0), "{clean:?}");

    repo.write("c.txt", "three\n");
    repo.commit("Add the third file.");
    let dirty = run_osf(&repo.dir, &home, &["scan", "--commits", &range]);
    assert_eq!(dirty.status.code(), Some(1), "{dirty:?}");
    assert!(String::from_utf8_lossy(&dirty.stdout).contains("commit-subject-full-stop"));
}

#[test]
fn the_hook_command_fails_a_bad_subject_and_passes_a_good_one() {
    let repo = TempRepo::new("hook-exit");
    let home = isolated_home("hook-exit");
    repo.write("good.txt", "Add a status subcommand\n");
    repo.write("bad.txt", "Add a status subcommand.\n");
    for (file, code) in [("good.txt", 0), ("bad.txt", 1)] {
        let output = run_osf(
            &repo.dir,
            &home,
            &[
                "lint",
                "writing",
                "--context",
                "commit",
                "--format",
                "human",
                file,
            ],
        );
        assert_eq!(output.status.code(), Some(code), "{file}: {output:?}");
    }
}
