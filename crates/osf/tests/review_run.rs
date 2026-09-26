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
use std::path::Path;

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

/// Writes the lens overrides described at the top of this file.
fn write_lens_overrides(repo: &TempRepo) {
    repo.write(".osf/review-lenses/correctness.toml", ACTIVE_LENS_TOML);
    for name in OTHER_ALWAYS_LENS_NAMES {
        repo.write(
            &format!(".osf/review-lenses/{name}.toml"),
            &disabled_lens_toml(name),
        );
    }
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
        2,
        "{types:?}"
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

#[test]
fn only_one_family_enabled_cannot_run() {
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
    assert_eq!(
        output.status.code(),
        Some(2),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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
        started.elapsed() < std::time::Duration::from_secs(5),
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
