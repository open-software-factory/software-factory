//! `osf review run`, end to end, through the fake harness so no test ever
//! calls a real model.
//!
//! Every lens the shipped catalogue runs on every change is overridden
//! under `.osf/review-lenses/` for these tests: the six that always run,
//! plus the two that run on every change at the high tier, are all
//! redefined as `triggered` lenses with no trigger, so none of them ever
//! selects itself here. One of the six, `correctness`, is instead
//! redefined as the lens these tests actually exercise, with the same
//! `c1`/`c2` criteria the fake harness answers already use, and an empty
//! `context` list, so it needs no diff, no work item and no decision
//! record to build. That leaves exactly one lens selected for every
//! change these tests make, whatever its size or tier.

mod common;

use common::{TempDir, TempRepo};
use std::path::{Path, PathBuf};

/// The lens names the shipped catalogue always runs, other than
/// `correctness`, which these tests redefine as their one active lens
/// instead of disabling.
const OTHER_ALWAYS_LENS_NAMES: &[&str] = &[
    "data-migration-and-compatibility",
    "privacy-and-data-protection",
    "security",
    "spec-and-acceptance",
    "test-quality",
    "architecture-adherence",
    "duplication-and-reuse",
];

fn fixture(name: &str) -> String {
    format!(
        "{}/tests/fixtures/review/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// A lens file that never selects itself: `triggered`, with no trigger
/// paths or signals at all.
fn disabled_lens_toml(name: &str) -> String {
    format!(
        "name = \"{name}\"\n\
         summary = \"disabled for this test\"\n\
         weight = 1.0\n\
         runs = \"triggered\"\n\
         criteria = []\n\n\
         [severity_guide]\n\
         blocker = \"n/a\"\n\
         major = \"n/a\"\n\
         minor = \"n/a\"\n"
    )
}

/// The one lens these tests exercise: `correctness`, redefined with the
/// `c1`/`c2` criteria the fake harness's canned answers already carry
/// scores for, and no declared context, so it builds with no diff, no work
/// item and nothing else the test repository would otherwise need to hold.
const ACTIVE_LENS_TOML: &str = "name = \"correctness\"\n\
    summary = \"the one lens these review-run tests exercise\"\n\
    weight = 1.0\n\
    runs = \"always\"\n\
    criteria = [\n  { id = \"c1\", question = \"q1\" },\n  { id = \"c2\", question = \"q2\" },\n]\n\n\
    [severity_guide]\n\
    blocker = \"loses data\"\n\
    major = \"a wrong behaviour a user can hit\"\n\
    minor = \"a small nit\"\n";

/// Writes the lens overrides described at the top of this file, under
/// `dir` directly rather than through a [`TempRepo`], so a plain trusted
/// config directory (no git repository of its own) can hold them too.
fn write_lens_overrides_to(dir: &Path) {
    let write = |rel: &str, content: &str| {
        let full = dir.join(rel);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("fixture parent dir creates");
        }
        std::fs::write(&full, content).expect("fixture file writes");
    };
    write(".osf/review-lenses/correctness.toml", ACTIVE_LENS_TOML);
    for name in OTHER_ALWAYS_LENS_NAMES {
        write(
            &format!(".osf/review-lenses/{name}.toml"),
            &disabled_lens_toml(name),
        );
    }
}

/// Writes the lens overrides described at the top of this file.
fn write_lens_overrides(repo: &TempRepo) {
    write_lens_overrides_to(&repo.dir);
}

/// A repository with the lens overrides and `osf.toml` committed as part
/// of its base commit, `origin/main` tracking that commit, and one further
/// committed change ready to diff against it: only `README.md`, so the
/// reviewed diff itself never adds a file or leaves one untracked, which
/// would otherwise earn the `new-file` signal and select a triggered lens
/// these tests do not account for. `src/lib.rs` is real, so a blocker
/// finding has real text at a real line to quote.
fn review_repo(name: &str, osf_toml: &str) -> TempRepo {
    let repo = TempRepo::new(name);
    write_lens_overrides(&repo);
    repo.write("osf.toml", osf_toml);
    repo.write("src/lib.rs", "fn one() {}\nfn broken() {}\n");
    repo.write("README.md", "base\n");
    repo.commit("base");
    repo.track_origin_main();
    repo.write("README.md", "base\nplus a change to review\n");
    repo.commit("a small change to review");
    repo
}

/// The same as [`review_repo`], except the reviewed commit carries a
/// `Code-Generator:` trailer naming `trailer_model`, so `osf review run`
/// detects a builder family for it.
fn review_repo_built_by(name: &str, osf_toml: &str, trailer_model: &str) -> TempRepo {
    let repo = TempRepo::new(name);
    write_lens_overrides(&repo);
    repo.write("osf.toml", osf_toml);
    repo.write("src/lib.rs", "fn one() {}\nfn broken() {}\n");
    repo.write("README.md", "base\n");
    repo.commit("base");
    repo.track_origin_main();
    repo.write("README.md", "base\nplus a change to review\n");
    repo.commit(&format!(
        "a small change to review\n\nCode-Generator: {trailer_model} <noreply@example.com>\n"
    ));
    repo
}

/// The fake harness, invoked with `answer_path`'s content wired to its own
/// `OSF_FAKE_ANSWER`, as a single command line so each roster entry can
/// carry its own canned answer independently of process-wide environment
/// state.
#[cfg(unix)]
fn fake_reviewer_command(answer_path: &str) -> Vec<String> {
    vec![
        "sh".to_string(),
        "-c".to_string(),
        format!(
            "OSF_FAKE_ANSWER={} {}",
            answer_path,
            fixture("fake-harness.sh")
        ),
    ]
}

#[cfg(windows)]
fn fake_reviewer_command(answer_path: &str) -> Vec<String> {
    vec![
        "powershell".to_string(),
        "-NoProfile".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-Command".to_string(),
        format!(
            "$env:OSF_FAKE_ANSWER='{}'; & '{}'",
            answer_path,
            fixture("fake-harness.ps1")
        ),
    ]
}

/// A reviewer whose harness sleeps for `sleep_secs` before answering, to
/// prove a configured timeout, not just the default, governs how long it
/// may run.
#[cfg(unix)]
fn slow_reviewer_command(answer_path: &str, sleep_secs: u64) -> Vec<String> {
    vec![
        "sh".to_string(),
        "-c".to_string(),
        format!(
            "OSF_FAKE_ANSWER={answer_path} OSF_FAKE_SLEEP_SECS={sleep_secs} {}",
            fixture("fake-harness.sh")
        ),
    ]
}

#[cfg(windows)]
fn slow_reviewer_command(answer_path: &str, sleep_secs: u64) -> Vec<String> {
    vec![
        "powershell".to_string(),
        "-NoProfile".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-Command".to_string(),
        format!(
            "$env:OSF_FAKE_ANSWER='{answer_path}'; $env:OSF_FAKE_SLEEP_SECS='{sleep_secs}'; & '{}'",
            fixture("fake-harness.ps1")
        ),
    ]
}

/// One `[[review.roster]]` entry, as TOML, with an arbitrary `command`.
fn raw_roster_entry_toml(name: &str, family: &str, command: &[String], enabled: bool) -> String {
    let command_toml = command
        .iter()
        .map(|arg| format!("{arg:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "[[review.roster]]\n\
         name = \"{name}\"\n\
         harness = \"fake\"\n\
         family = \"{family}\"\n\
         command = [{command_toml}]\n\
         enabled = {enabled}\n\n"
    )
}

/// One `[[review.roster]]` entry, as TOML, naming `answer_path`'s content
/// as this reviewer's whole answer.
fn roster_entry_toml(name: &str, family: &str, answer_path: &str, enabled: bool) -> String {
    raw_roster_entry_toml(name, family, &fake_reviewer_command(answer_path), enabled)
}

/// A reviewer whose harness writes `secret` to its own standard error and
/// exits non-zero, standing in for a broken or hostile coding-agent tool.
#[cfg(unix)]
fn failing_reviewer_command(secret: &str) -> Vec<String> {
    vec![
        "sh".to_string(),
        "-c".to_string(),
        format!("echo '{secret}' 1>&2; exit 9"),
    ]
}

#[cfg(windows)]
fn failing_reviewer_command(secret: &str) -> Vec<String> {
    vec![
        "powershell".to_string(),
        "-NoProfile".to_string(),
        "-Command".to_string(),
        format!("[Console]::Error.WriteLine('{secret}'); exit 9"),
    ]
}

/// Writes `content` to `name` under `dir`, and returns its absolute path as a string.
fn write_answer_file(dir: &TempDir, name: &str, content: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, content).expect("answer fixture writes");
    path.to_string_lossy().into_owned()
}

/// Every `event_type` value found in a journal buffer file's own lines.
fn journal_event_types(home: &Path) -> Vec<String> {
    let buffer_dir = home.join(".osf/state/buffer");
    let mut types = Vec::new();
    let Ok(entries) = std::fs::read_dir(&buffer_dir) else {
        return types;
    };
    for entry in entries {
        let entry = entry.expect("dir entry reads");
        let text = std::fs::read_to_string(entry.path()).expect("journal buffer reads");
        for line in text.lines() {
            let value: serde_json::Value =
                serde_json::from_str(line).expect("journal line is JSON");
            if let Some(event_type) = value.get("event_type").and_then(serde_json::Value::as_str) {
                types.push(event_type.to_string());
            }
        }
    }
    types
}

/// The payload of the single `review-decision` event in the journal buffer
/// under `home`.
fn journal_review_decision(home: &Path) -> serde_json::Value {
    let buffer_dir = home.join(".osf/state/buffer");
    let entries = std::fs::read_dir(&buffer_dir).expect("journal buffer dir reads");
    let mut found = None;
    for entry in entries {
        let entry = entry.expect("dir entry reads");
        let text = std::fs::read_to_string(entry.path()).expect("journal buffer reads");
        for line in text.lines() {
            let value: serde_json::Value =
                serde_json::from_str(line).expect("journal line is JSON");
            if value.get("event_type").and_then(serde_json::Value::as_str)
                == Some("review-decision")
            {
                found = value.get("payload").cloned();
            }
        }
    }
    found.expect("a review-decision event in the journal")
}

/// The whole content of every journal buffer file under `home`, concatenated.
fn journal_text(home: &Path) -> String {
    let buffer_dir = home.join(".osf/state/buffer");
    let mut text = String::new();
    let Ok(entries) = std::fs::read_dir(&buffer_dir) else {
        return text;
    };
    for entry in entries {
        let entry = entry.expect("dir entry reads");
        text.push_str(&std::fs::read_to_string(entry.path()).expect("journal buffer reads"));
    }
    text
}

/// Asserts `secret` is nowhere in `output`'s standard output or standard
/// error, nor in any journal buffer file under `home`, nor (when given) in
/// the file at `sarif_out`.
fn assert_no_leak(
    secret: &str,
    output: &std::process::Output,
    home: &Path,
    sarif_out: Option<&Path>,
) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stdout.contains(secret),
        "stdout leaked the secret: {stdout}"
    );
    assert!(
        !stderr.contains(secret),
        "stderr leaked the secret: {stderr}"
    );
    let journal = journal_text(home);
    assert!(
        !journal.contains(secret),
        "the journal leaked the secret: {journal}"
    );
    if let Some(path) = sarif_out {
        let sarif = std::fs::read_to_string(path).expect("sarif file reads");
        assert!(!sarif.contains(secret), "SARIF leaked the secret: {sarif}");
    }
}

/// The payload of the first `review-answer` event in the journal buffer
/// under `home`.
fn journal_first_review_answer(home: &Path) -> serde_json::Value {
    let buffer_dir = home.join(".osf/state/buffer");
    let entries = std::fs::read_dir(&buffer_dir).expect("journal buffer dir reads");
    for entry in entries {
        let entry = entry.expect("dir entry reads");
        let text = std::fs::read_to_string(entry.path()).expect("journal buffer reads");
        for line in text.lines() {
            let value: serde_json::Value =
                serde_json::from_str(line).expect("journal line is JSON");
            if value.get("event_type").and_then(serde_json::Value::as_str) == Some("review-answer")
            {
                return value
                    .get("payload")
                    .cloned()
                    .expect("review-answer payload");
            }
        }
    }
    panic!("no review-answer event in the journal");
}

/// Decisions 0005 and 0016 require every finding and summary to carry an
/// evidence grade: a reviewer's judgment is reported, not measured, so both
/// journal event kinds this module writes must say so.
#[test]
fn review_answer_and_review_decision_events_carry_the_reported_evidence_grade() {
    let osf_toml = format!(
        "{}{}",
        roster_entry_toml("fake-a", "family-a", &fixture("valid.json"), true),
        roster_entry_toml("fake-b", "family-b", &fixture("valid.json"), true),
    );
    let repo = review_repo("evidence-grade", &osf_toml);
    let home = common::isolated_home("review-run-evidence-grade");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main"],
    );
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let answer = journal_first_review_answer(&home);
    assert_eq!(
        answer.get("grade").and_then(serde_json::Value::as_str),
        Some("reported")
    );
    let decision = journal_review_decision(&home);
    assert_eq!(
        decision.get("grade").and_then(serde_json::Value::as_str),
        Some("reported")
    );
}

#[test]
fn two_families_that_both_answer_validly_pass_and_write_the_journal_events() {
    let osf_toml = format!(
        "{}{}",
        roster_entry_toml("fake-a", "family-a", &fixture("valid.json"), true),
        roster_entry_toml("fake-b", "family-b", &fixture("valid.json"), true),
    );
    let repo = review_repo("both-pass", &osf_toml);
    let home = common::isolated_home("review-run-both-pass");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main"],
    );
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let types = journal_event_types(&home);
    assert_eq!(
        types.iter().filter(|t| *t == "review-answer").count(),
        4,
        "two rounds each for the two families: {types:?}"
    );
    assert_eq!(
        types.iter().filter(|t| *t == "review-decision").count(),
        1,
        "{types:?}"
    );
}

#[test]
fn a_verified_blocker_finding_fails_the_review() {
    let osf_toml = format!(
        "{}{}",
        roster_entry_toml("fake-a", "family-a", &fixture("blocker.json"), true),
        roster_entry_toml("fake-b", "family-b", &fixture("valid.json"), true),
    );
    let repo = review_repo("blocker-fails", &osf_toml);
    let home = common::isolated_home("review-run-blocker");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main"],
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Two roster entries that share one family are still only one family: the
/// interim policy lets it decide the lens through its own extra critical
/// round, rather than blocking the review on a second family nobody has
/// configured.
#[test]
fn only_one_family_enabled_passes_under_the_interim_policy() {
    let osf_toml = format!(
        "{}{}",
        roster_entry_toml("fake-a", "same-family", &fixture("valid.json"), true),
        roster_entry_toml("fake-b", "same-family", &fixture("valid.json"), true),
    );
    let repo = review_repo("one-family", &osf_toml);
    let home = common::isolated_home("review-run-one-family");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main"],
    );
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("interim policy"), "{stdout}");
}

#[test]
fn every_reviewer_disabled_cannot_run_and_names_no_reviewer() {
    let repo = review_repo("all-disabled", "");
    let home = common::isolated_home("review-run-all-disabled");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main"],
    );
    assert_eq!(
        output.status.code(),
        Some(2),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("no reviewer"), "{stdout}");
}

#[test]
fn a_could_not_run_verdict_reports_no_score_or_threshold() {
    let repo = review_repo("could-not-run-no-score", "");
    let home = common::isolated_home("review-run-could-not-run-no-score");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main"],
    );
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let verdict_line = stdout
        .lines()
        .find(|line| line.starts_with("verdict:"))
        .expect("a verdict line is printed");
    assert_eq!(verdict_line, "verdict: could-not-run", "{stdout}");
    assert!(!verdict_line.contains("score"), "{verdict_line}");
    let decision = journal_review_decision(&home);
    assert_eq!(decision.get("score"), Some(&serde_json::Value::Null));
    assert_eq!(decision.get("threshold"), Some(&serde_json::Value::Null));
}

#[test]
fn a_configured_timeout_governs_how_long_a_reviewer_may_run() {
    let osf_toml = format!(
        "[review]\ntimeout_seconds = 1\n\n{}",
        raw_roster_entry_toml(
            "fake-a",
            "family-a",
            &slow_reviewer_command(&fixture("valid.json"), 6),
            true,
        ),
    );
    let repo = review_repo("configured-timeout", &osf_toml);
    let home = common::isolated_home("review-run-configured-timeout");
    let started = std::time::Instant::now();
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main"],
    );
    assert!(
        // One family enabled earns the interim policy's extra critical
        // round: up to three 1-second timeouts in a row, plus process
        // overhead, well under the 300-second default this guards against.
        started.elapsed() < std::time::Duration::from_secs(15),
        "took {:?}: the configured 1-second timeout should have governed \
         this run, not the 300-second default; stdout: {}\nstderr: {}",
        started.elapsed(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let journal = journal_text(&home);
    assert!(journal.contains("timed out"), "{journal}");
}

#[test]
fn a_bad_lens_file_cannot_configure_and_names_the_file() {
    let repo = TempRepo::new("bad-lens-file");
    repo.write("README.md", "base\n");
    repo.commit("base");
    repo.track_origin_main();
    repo.write(
        ".osf/review-lenses/broken.toml",
        "name = \"broken\"\nweight = 1.0\nruns = \"always\"\nnotAField = true\n",
    );
    repo.write("README.md", "base\nplus a change\n");
    repo.commit("a small change");
    let home = common::isolated_home("review-run-bad-lens");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main"],
    );
    assert_eq!(
        output.status.code(),
        Some(2),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("broken.toml"), "{stderr}");
}

#[test]
fn a_secret_in_reviewer_stderr_never_reaches_the_journal_or_output() {
    let secret = common::fake_provider_key("sk-");
    let osf_toml = raw_roster_entry_toml(
        "fake-a",
        "family-a",
        &failing_reviewer_command(&secret),
        true,
    );
    let repo = review_repo("secret-stderr", &osf_toml);
    let home = common::isolated_home("review-run-secret-stderr");
    let sarif_out = repo.dir.join("out.sarif");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &[
            "review",
            "run",
            "--base",
            "origin/main",
            "--sarif-out",
            &sarif_out.to_string_lossy(),
        ],
    );
    assert_no_leak(&secret, &output, &home, Some(&sarif_out));
}

/// A `Code-Generator: Claude ...` trailer on the reviewed commit makes
/// `anthropic` this change's builder family. With codex (openai),
/// claude-code (anthropic) and dsh (deepseek) all enabled, claude-code is
/// left out and the other two still answer and reach quorum.
#[test]
fn a_reviewer_whose_family_built_the_change_is_left_out_and_two_others_still_pass() {
    let osf_toml = format!(
        "{}{}{}",
        roster_entry_toml("codex", "openai", &fixture("valid.json"), true),
        roster_entry_toml("claude-code", "anthropic", &fixture("valid.json"), true),
        roster_entry_toml("dsh", "deepseek", &fixture("valid.json"), true),
    );
    let repo = review_repo_built_by("builder-family-skip", &osf_toml, "Claude Sonnet 5");
    let home = common::isolated_home("review-run-builder-family-skip");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main"],
    );
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let types = journal_event_types(&home);
    assert_eq!(
        types.iter().filter(|t| *t == "review-answer").count(),
        5,
        "two rounds each for codex and dsh, plus one skipped record for claude-code: {types:?}"
    );
    let decision = journal_review_decision(&home);
    let builder_families: Vec<&str> = decision
        .get("builder_families")
        .and_then(serde_json::Value::as_array)
        .expect("builder_families is an array")
        .iter()
        .map(|v| v.as_str().expect("a family name"))
        .collect();
    assert_eq!(builder_families, vec!["anthropic"]);
}

/// With only codex (openai) and claude-code (anthropic) enabled, and the
/// change built by Claude, claude-code is left out and only one family
/// (openai) is left to try: the interim policy runs codex for one extra
/// critical round and lets it decide the lens on its own, naming the
/// policy rather than blocking the review on the family the builder used.
#[test]
fn one_non_builder_family_passes_under_the_interim_policy() {
    let osf_toml = format!(
        "{}{}",
        roster_entry_toml("codex", "openai", &fixture("valid.json"), true),
        roster_entry_toml("claude-code", "anthropic", &fixture("valid.json"), true),
    );
    let repo = review_repo_built_by("builder-family-interim", &osf_toml, "Claude Sonnet 5");
    let home = common::isolated_home("review-run-builder-family-interim");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main"],
    );
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("interim policy"), "{stdout}");
    let decision = journal_review_decision(&home);
    let builder_families: Vec<&str> = decision
        .get("builder_families")
        .and_then(serde_json::Value::as_array)
        .expect("builder_families is an array")
        .iter()
        .map(|v| v.as_str().expect("a family name"))
        .collect();
    assert_eq!(builder_families, vec!["anthropic"]);
}

/// No `Code-Generator:` trailer at all on the reviewed commit records the
/// builder family as `unknown`, and every enabled reviewer still runs, the
/// same as before this module knew about builder families at all.
#[test]
fn no_builder_family_detected_records_unknown_and_runs_every_reviewer() {
    let osf_toml = format!(
        "{}{}",
        roster_entry_toml("fake-a", "family-a", &fixture("valid.json"), true),
        roster_entry_toml("fake-b", "family-b", &fixture("valid.json"), true),
    );
    let repo = review_repo("no-builder-family", &osf_toml);
    let home = common::isolated_home("review-run-no-builder-family");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main"],
    );
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let decision = journal_review_decision(&home);
    let builder_families: Vec<&str> = decision
        .get("builder_families")
        .and_then(serde_json::Value::as_array)
        .expect("builder_families is an array")
        .iter()
        .map(|v| v.as_str().expect("a family name"))
        .collect();
    assert_eq!(builder_families, vec!["unknown"]);
}

/// `--builder-family` overrides detection entirely: the family it names is
/// recorded in the decision, whatever the reviewed commits' own trailers say.
#[test]
fn the_builder_family_flag_overrides_detection() {
    let osf_toml = format!(
        "{}{}{}",
        roster_entry_toml("codex", "openai", &fixture("valid.json"), true),
        roster_entry_toml("claude-code", "anthropic", &fixture("valid.json"), true),
        roster_entry_toml("dsh", "deepseek", &fixture("valid.json"), true),
    );
    let repo = review_repo("builder-family-flag-override", &osf_toml);
    let home = common::isolated_home("review-run-builder-family-flag-override");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &[
            "review",
            "run",
            "--base",
            "origin/main",
            "--builder-family",
            "openai",
        ],
    );
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let decision = journal_review_decision(&home);
    let builder_families: Vec<&str> = decision
        .get("builder_families")
        .and_then(serde_json::Value::as_array)
        .expect("builder_families is an array")
        .iter()
        .map(|v| v.as_str().expect("a family name"))
        .collect();
    assert_eq!(builder_families, vec!["openai"]);
}

/// `[review] hot_paths` must not make `osf review run` could-not-configure:
/// `risk::assess` reads it straight from TOML, but `config::review_config`
/// also parses the same `[review]` table with `deny_unknown_fields`, so a
/// hot-path test that only calls `risk::assess` directly can pass while the
/// full command still fails on the field `review_config` does not know.
#[test]
fn hot_paths_in_osf_toml_does_not_fail_the_full_review_run_command() {
    // A glob that never matches the reviewed change: this test is only
    // about `hot_paths` parsing, not about earning the "high-traffic path"
    // signal, which would select lenses beyond the one this test's fixture
    // repository is set up to answer for.
    let osf_toml = format!(
        "[review]\nhot_paths = [\"never/matches/anything.rs\"]\n\n{}{}",
        roster_entry_toml("fake-a", "family-a", &fixture("valid.json"), true),
        roster_entry_toml("fake-b", "family-b", &fixture("valid.json"), true),
    );
    let repo = review_repo("hot-paths-full-command", &osf_toml);
    let home = common::isolated_home("review-run-hot-paths-full-command");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main"],
    );
    assert!(
        output.status.success(),
        "a documented [review] hot_paths setting must not make the whole command \
         could-not-configure; stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `--config-root` names the trusted tree: the lens catalogue and the
/// `[review]` table (roster, threshold, timeout, cost ceiling) come from
/// there, never from the repository under review. A pull request that
/// lowers its own threshold to zero and swaps in a harmless roster must
/// still fail, because the base tree's own high threshold and its
/// blocker-returning roster are what actually run.
#[test]
fn a_pull_request_tree_cannot_loosen_review_via_its_own_config_root() {
    let config_root = TempDir::new("review-run-config-root-base");
    write_lens_overrides_to(&config_root);
    let base_osf_toml = format!(
        "[review]\nthreshold = 0.9\n\n{}{}",
        roster_entry_toml("fake-a", "family-a", &fixture("blocker.json"), true),
        roster_entry_toml("fake-b", "family-b", &fixture("valid.json"), true),
    );
    std::fs::write(config_root.join("osf.toml"), base_osf_toml).expect("base osf.toml writes");

    // The reviewed tree carries its own low threshold and a roster that
    // would only ever answer clean. If either of these were read instead
    // of the base tree's, the review would pass.
    let pr_osf_toml = format!(
        "[review]\nthreshold = 0.0\n\n{}",
        roster_entry_toml("fake-only", "family-only", &fixture("valid.json"), true),
    );
    let repo = review_repo("config-root-pr-tree", &pr_osf_toml);
    let home = common::isolated_home("review-run-config-root");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &[
            "review",
            "run",
            "--base",
            "origin/main",
            "--config-root",
            &config_root.to_string_lossy(),
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "the pull request's own lowered threshold and harmless roster must have no effect \
         when --config-root points at the base tree; stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// With no `--config-root`, the reviewed repository's own `osf.toml` still
/// governs, exactly as before this flag existed.
#[test]
fn omitting_config_root_reads_configuration_from_the_repository_under_review() {
    let osf_toml = format!(
        "[review]\nthreshold = 0.0\n\n{}{}",
        roster_entry_toml("fake-a", "family-a", &fixture("valid.json"), true),
        roster_entry_toml("fake-b", "family-b", &fixture("valid.json"), true),
    );
    let repo = review_repo("no-config-root", &osf_toml);
    let home = common::isolated_home("review-run-no-config-root");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main"],
    );
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `--warn-only` still runs the review and still prints the verdict, but
/// the process always exits 0, whatever that verdict is.
#[test]
fn warn_only_prints_the_verdict_but_never_fails() {
    let osf_toml = format!(
        "{}{}",
        roster_entry_toml("fake-a", "family-a", &fixture("blocker.json"), true),
        roster_entry_toml("fake-b", "family-b", &fixture("valid.json"), true),
    );
    let repo = review_repo("warn-only-blocker", &osf_toml);
    let home = common::isolated_home("review-run-warn-only-blocker");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main", "--warn-only"],
    );
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("verdict: fail"), "{stdout}");
}

/// `--warn-only` also swallows a could-not-run verdict, the same as a fail.
#[test]
fn warn_only_never_fails_on_could_not_run_either() {
    let repo = review_repo("warn-only-could-not-run", "");
    let home = common::isolated_home("review-run-warn-only-could-not-run");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main", "--warn-only"],
    );
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A fake `gh`, standing in for the real one so `--post-to` can be tested
/// with no network: `pr view` answers a fixed head commit, and `api -X
/// POST` answers success or one of the failures `osf review post` already
/// knows how to react to, chosen by the `FAKE_GH_MODE` environment
/// variable a test sets on the child.
#[cfg(unix)]
fn write_fake_gh(dir: &Path, head_sha: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let path = dir.join("gh");
    let script = format!(
        "#!/bin/sh\n\
         case \"$1 $2\" in\n\
         \x20\x20'pr view') echo '{{\"headRefOid\":\"{head_sha}\"}}' ;;\n\
         \x20\x20'api -X')\n\
         \x20\x20\x20\x20cat > /dev/null\n\
         \x20\x20\x20\x20if [ \"$FAKE_GH_MODE\" = 'refuse' ]; then\n\
         \x20\x20\x20\x20\x20\x20echo 'gh: Review cannot be requested on your own pull \
         request (HTTP 422)' 1>&2\n\
         \x20\x20\x20\x20\x20\x20exit 1\n\
         \x20\x20\x20\x20fi\n\
         \x20\x20\x20\x20if [ \"$FAKE_GH_MODE\" = 'boom' ]; then\n\
         \x20\x20\x20\x20\x20\x20echo 'gh: some other failure (HTTP 500)' 1>&2\n\
         \x20\x20\x20\x20\x20\x20exit 1\n\
         \x20\x20\x20\x20fi\n\
         \x20\x20\x20\x20echo '{{\"id\":1,\"state\":\"CHANGES_REQUESTED\",\"html_url\":\
         \"https://example.invalid/1\"}}'\n\
         \x20\x20\x20\x20;;\n\
         \x20\x20*) exit 1 ;;\n\
         esac\n"
    );
    std::fs::write(&path, script).expect("fake gh writes");
    let mut perms = std::fs::metadata(&path)
        .expect("fake gh metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).expect("fake gh chmod");
    dir.to_path_buf()
}

/// `PATH`, with `dir` prepended, so a spawned `osf` resolves `gh` to the
/// fake in `dir` before any real `gh` on this machine, while still finding
/// git and everything else on the ambient `PATH`.
#[cfg(unix)]
fn path_with_dir_first(dir: &Path) -> String {
    let ambient = std::env::var("PATH").unwrap_or_default();
    format!("{}:{ambient}", dir.display())
}

/// `--post-to` posts the kept findings by reusing `osf review post`'s own
/// machinery: a blocker finding still fails the run's own exit code, and
/// the fake `gh` records that a review landed with the right verdict.
#[test]
#[cfg(unix)]
fn post_to_posts_kept_findings_reusing_review_post_machinery() {
    let osf_toml = format!(
        "{}{}",
        roster_entry_toml("fake-a", "family-a", &fixture("blocker.json"), true),
        roster_entry_toml("fake-b", "family-b", &fixture("valid.json"), true),
    );
    let repo = review_repo("post-to-blocker", &osf_toml);
    let home = common::isolated_home("review-run-post-to-blocker");
    let gh_holder = TempDir::new("review-run-post-to-blocker-gh");
    let gh_dir = write_fake_gh(&gh_holder, "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef");
    let path = path_with_dir_first(&gh_dir);
    let output = common::run_osf_with_env(
        &repo.dir,
        &home,
        &[("PATH", &path)],
        &[
            "review",
            "run",
            "--base",
            "origin/main",
            "--post-to",
            "open-software-factory/widgets#7",
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "a blocker finding still fails the run's own exit code; stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("posted review 1: CHANGES_REQUESTED"),
        "{stdout}"
    );
    assert!(stdout.contains("REQUEST_CHANGES"), "{stdout}");
}

/// A failed post leaves the branch-protection gate with no review evidence
/// behind it, so the run counts as could-not-run even though the verdict
/// itself (a blocker finding) was decided fine.
#[test]
#[cfg(unix)]
fn post_to_failing_makes_the_run_could_not_run() {
    let osf_toml = format!(
        "{}{}",
        roster_entry_toml("fake-a", "family-a", &fixture("blocker.json"), true),
        roster_entry_toml("fake-b", "family-b", &fixture("valid.json"), true),
    );
    let repo = review_repo("post-to-boom", &osf_toml);
    let home = common::isolated_home("review-run-post-to-boom");
    let gh_holder = TempDir::new("review-run-post-to-boom-gh");
    let gh_dir = write_fake_gh(&gh_holder, "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef");
    let path = path_with_dir_first(&gh_dir);
    let output = common::run_osf_with_env(
        &repo.dir,
        &home,
        &[("PATH", &path), ("FAKE_GH_MODE", "boom")],
        &[
            "review",
            "run",
            "--base",
            "origin/main",
            "--post-to",
            "open-software-factory/widgets#7",
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(2),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("some other failure"), "{stderr}");
}

/// A journal that cannot be opened (an unwritable state directory) leaves no
/// record of the answers or the decision it claims to have made, so the run
/// counts as could-not-run even on an otherwise clean pass.
#[test]
fn an_unwritable_journal_makes_the_run_could_not_run_even_on_a_pass() {
    let osf_toml = format!(
        "{}{}",
        roster_entry_toml("fake-a", "family-a", &fixture("valid.json"), true),
        roster_entry_toml("fake-b", "family-b", &fixture("valid.json"), true),
    );
    let repo = review_repo("journal-unwritable", &osf_toml);
    let home = common::isolated_home("review-run-journal-unwritable");
    let blocked_state_dir = home.join("state-is-a-file");
    std::fs::write(&blocked_state_dir, "not a directory").expect("blocked state dir file writes");
    let state_dir_arg = blocked_state_dir.to_string_lossy().into_owned();
    let output = common::run_osf_with_env(
        &repo.dir,
        &home,
        &[("OSF_STATE_DIR", &state_dir_arg)],
        &["review", "run", "--base", "origin/main"],
    );
    assert_eq!(
        output.status.code(),
        Some(2),
        "a journal that cannot be opened must not report a clean pass; stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("verdict: pass"), "{stdout}");
}

#[test]
fn a_secret_in_a_verified_findings_body_is_redacted_in_sarif() {
    let secret = common::fake_provider_key("sk-");
    let payload = serde_json::json!({
        "lens": "correctness",
        "scores": {"c1": 0.9, "c2": 0.9},
        "findings": [{
            "path": "src/lib.rs",
            "line": 2,
            "quote": "fn broken() {}",
            "severity": "minor",
            "action": "justify",
            "body": secret
        }]
    })
    .to_string();
    let payloads = TempDir::new("review-run-secret-body-payload");
    let answer_path = write_answer_file(&payloads, "answer.json", &payload);
    let osf_toml = roster_entry_toml("fake-a", "family-a", &answer_path, true);
    let repo = review_repo("secret-body", &osf_toml);
    let home = common::isolated_home("review-run-secret-body");
    let sarif_out = repo.dir.join("out.sarif");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &[
            "review",
            "run",
            "--base",
            "origin/main",
            "--sarif-out",
            &sarif_out.to_string_lossy(),
        ],
    );
    assert_no_leak(&secret, &output, &home, Some(&sarif_out));
    let sarif = std::fs::read_to_string(&sarif_out).expect("sarif file reads");
    assert!(sarif.contains("redacted"), "{sarif}");
}

#[test]
fn a_secret_as_the_answers_lens_field_never_reaches_the_journal_or_output() {
    let secret = common::fake_provider_key("sk-");
    let payload = serde_json::json!({
        "lens": secret,
        "scores": {"c1": 0.9, "c2": 0.9},
        "findings": []
    })
    .to_string();
    let payloads = TempDir::new("review-run-secret-lens-payload");
    let answer_path = write_answer_file(&payloads, "answer.json", &payload);
    let osf_toml = roster_entry_toml("fake-a", "family-a", &answer_path, true);
    let repo = review_repo("secret-lens", &osf_toml);
    let home = common::isolated_home("review-run-secret-lens");
    let sarif_out = repo.dir.join("out.sarif");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &[
            "review",
            "run",
            "--base",
            "origin/main",
            "--sarif-out",
            &sarif_out.to_string_lossy(),
        ],
    );
    assert_no_leak(&secret, &output, &home, Some(&sarif_out));
}

#[test]
fn a_secret_as_an_invalid_severity_value_never_reaches_the_journal_or_output() {
    let secret = common::fake_provider_key("sk-");
    let payload = serde_json::json!({
        "lens": "correctness",
        "scores": {"c1": 0.9, "c2": 0.9},
        "findings": [{
            "path": "src/lib.rs",
            "line": 1,
            "quote": "fn one() {}",
            "severity": secret,
            "action": "must-fix",
            "body": "x"
        }]
    })
    .to_string();
    let payloads = TempDir::new("review-run-secret-severity-payload");
    let answer_path = write_answer_file(&payloads, "answer.json", &payload);
    let osf_toml = roster_entry_toml("fake-a", "family-a", &answer_path, true);
    let repo = review_repo("secret-severity", &osf_toml);
    let home = common::isolated_home("review-run-secret-severity");
    let sarif_out = repo.dir.join("out.sarif");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &[
            "review",
            "run",
            "--base",
            "origin/main",
            "--sarif-out",
            &sarif_out.to_string_lossy(),
        ],
    );
    assert_no_leak(&secret, &output, &home, Some(&sarif_out));
}

#[test]
fn a_secret_as_an_unexpected_extra_field_name_never_reaches_the_journal_or_output() {
    let secret = common::fake_provider_key("sk-");
    let mut payload = serde_json::json!({
        "lens": "correctness",
        "scores": {"c1": 0.9, "c2": 0.9},
        "findings": []
    });
    payload
        .as_object_mut()
        .expect("payload is a JSON object")
        .insert(secret.clone(), serde_json::json!(true));
    let payloads = TempDir::new("review-run-secret-extra-field-payload");
    let answer_path = write_answer_file(&payloads, "answer.json", &payload.to_string());
    let osf_toml = roster_entry_toml("fake-a", "family-a", &answer_path, true);
    let repo = review_repo("secret-extra-field", &osf_toml);
    let home = common::isolated_home("review-run-secret-extra-field");
    let sarif_out = repo.dir.join("out.sarif");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &[
            "review",
            "run",
            "--base",
            "origin/main",
            "--sarif-out",
            &sarif_out.to_string_lossy(),
        ],
    );
    assert_no_leak(&secret, &output, &home, Some(&sarif_out));
}

#[test]
fn if_enabled_with_no_reviewer_enabled_exits_zero_and_opens_no_journal() {
    let repo = review_repo("if-enabled-off", "");
    let home = common::isolated_home("review-run-if-enabled-off");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--if-enabled", "--base", "origin/main"],
    );
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("slot off"), "{stdout}");
    assert!(
        journal_event_types(&home).is_empty(),
        "the off path must never open the journal"
    );
}

/// The `--if-enabled` slot-off path still writes a valid, empty SARIF file
/// when `--sarif-out` is given, the same way `osf check lint-writing` writes
/// one when it has nothing to check: a moon task that declares this file as
/// an output must find it there even when no reviewer ran.
#[test]
fn if_enabled_with_no_reviewer_enabled_still_writes_an_empty_sarif() {
    let repo = review_repo("if-enabled-off-sarif", "");
    let home = common::isolated_home("review-run-if-enabled-off-sarif");
    let sarif_out = repo.dir.join("out.sarif");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &[
            "review",
            "run",
            "--if-enabled",
            "--base",
            "origin/main",
            "--sarif-out",
            &sarif_out.to_string_lossy(),
        ],
    );
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let sarif_text = std::fs::read_to_string(&sarif_out).expect("sarif file was written");
    let sarif: serde_json::Value =
        serde_json::from_str(&sarif_text).expect("sarif file is valid JSON");
    let results = sarif
        .pointer("/runs/0/results")
        .and_then(serde_json::Value::as_array)
        .expect("a runs[0].results array");
    assert!(results.is_empty(), "{sarif_text}");
}

/// `--if-enabled` only ever decides whether the slot is on at all; once it
/// is, the same interim policy applies as any other run with one enabled
/// family, and still journals every answer.
#[test]
fn if_enabled_with_one_reviewer_enabled_passes_under_the_interim_policy() {
    let osf_toml = roster_entry_toml("fake-a", "family-a", &fixture("valid.json"), true);
    let repo = review_repo("if-enabled-on-interim", &osf_toml);
    let home = common::isolated_home("review-run-if-enabled-on");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &["review", "run", "--if-enabled", "--base", "origin/main"],
    );
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !journal_event_types(&home).is_empty(),
        "the on path must still journal"
    );
}

#[test]
fn omitting_base_falls_back_to_the_osf_base_environment_variable() {
    let osf_toml = format!(
        "{}{}",
        roster_entry_toml("fake-a", "family-a", &fixture("valid.json"), true),
        roster_entry_toml("fake-b", "family-b", &fixture("valid.json"), true),
    );
    let repo = review_repo("base-from-env", &osf_toml);
    let home = common::isolated_home("review-run-base-from-env");
    let output = common::run_osf_with_env(
        &repo.dir,
        &home,
        &[("OSF_BASE", "origin/main")],
        &["review", "run"],
    );
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_secret_as_an_unresolvable_findings_path_is_dropped_not_leaked() {
    let secret = common::fake_provider_key("sk-");
    let payload = serde_json::json!({
        "lens": "correctness",
        "scores": {"c1": 0.9, "c2": 0.9},
        "findings": [{
            "path": secret,
            "line": 1,
            "quote": "this quote cannot verify against a file that does not exist",
            "severity": "minor",
            "action": "justify",
            "body": "x"
        }]
    })
    .to_string();
    let payloads = TempDir::new("review-run-secret-path-payload");
    let answer_path = write_answer_file(&payloads, "answer.json", &payload);
    let osf_toml = format!(
        "{}{}",
        roster_entry_toml("fake-a", "family-a", &answer_path, true),
        roster_entry_toml("fake-b", "family-b", &fixture("valid.json"), true),
    );
    let repo = review_repo("secret-path", &osf_toml);
    let home = common::isolated_home("review-run-secret-path");
    let sarif_out = repo.dir.join("out.sarif");
    let output = common::run_osf(
        &repo.dir,
        &home,
        &[
            "review",
            "run",
            "--base",
            "origin/main",
            "--sarif-out",
            &sarif_out.to_string_lossy(),
        ],
    );
    assert_no_leak(&secret, &output, &home, Some(&sarif_out));
}
