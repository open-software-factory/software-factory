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
#[cfg(unix)]
use std::path::PathBuf;

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
    review_repo_with(name, osf_toml, &[])
}

/// [`review_repo`], with `extra` files committed in the base commit too.
fn review_repo_with(name: &str, osf_toml: &str, extra: &[(&str, &str)]) -> TempRepo {
    let repo = TempRepo::new(name);
    write_lens_overrides(&repo);
    for (path, content) in extra {
        repo.write(path, content);
    }
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

/// How long a reviewer may run in the timeout tests. PowerShell alone takes
/// about a second to start, so Windows gets a longer limit.
#[cfg(unix)]
const TIMEOUT_SECS: u64 = 1;
#[cfg(windows)]
const TIMEOUT_SECS: u64 = 5;

/// How long a slow fake sleeps: well past the timeout.
const SLOW_SECS: u64 = TIMEOUT_SECS + 5;

/// What one fake agent program does when osf starts it as a reviewer.
enum Fake<'a> {
    /// Prints the answer file's text.
    Answers(&'a str),
    /// Sleeps, then prints the answer file's text.
    Slow(&'a str, u64),
    /// Writes a secret to standard error and exits non-zero, as a broken agent would.
    Fails(&'a str),
    /// Prints the answer file's text, and records its prompt, arguments and
    /// environment under the folder, plus a `started` file.
    Records(&'a str, &'a Path),
}

/// `osf.toml` text that selects `reviewers` from the agent list, after `prefix`.
fn agents_toml(prefix: &str, reviewers: &[&str]) -> String {
    let names: Vec<String> = reviewers.iter().map(|n| format!("\"{n}\"")).collect();
    format!("{prefix}[agents]\nreviewers = [{}]\n", names.join(", "))
}

/// Writes the fake program for `agent` into `bin`: a script that runs the
/// fake harness the way `fake` says.
#[cfg(unix)]
fn write_fake_agent(bin: &Path, agent: &str, fake: &Fake) {
    use std::os::unix::fs::PermissionsExt as _;
    let harness = fixture("fake-harness.sh");
    let body = match fake {
        Fake::Answers(answer) => format!("OSF_FAKE_ANSWER='{answer}' exec '{harness}'"),
        Fake::Slow(answer, secs) => {
            format!("OSF_FAKE_ANSWER='{answer}' OSF_FAKE_SLEEP_SECS={secs} exec '{harness}'")
        }
        Fake::Fails(secret) => format!("echo '{secret}' 1>&2\nexit 9"),
        Fake::Records(answer, dir) => format!(
            "env > '{d}/env'\nOSF_FAKE_ANSWER='{answer}' OSF_FAKE_PROMPT_CAPTURE='{d}/prompt' \
             OSF_FAKE_ARGS_CAPTURE='{d}/args' OSF_FAKE_HARNESS_LOG='{d}/started' OSF_FAKE_CHANGE_CAPTURE='{d}/change' \
             exec '{harness}' \"$@\"",
            d = dir.display()
        ),
    };
    let program = bin.join(agent);
    std::fs::write(&program, format!("#!/bin/sh\n{body}\n")).expect("fake agent writes");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
        .expect("fake agent chmod");
}

/// The Windows form: a `.cmd` file that runs `fake-harness.ps1`. The dsh agent
/// is started through `sh -c`, so a fake `sh.cmd` stands in for it too.
#[cfg(windows)]
fn write_fake_agent(bin: &Path, agent: &str, fake: &Fake) {
    let harness = fixture("fake-harness.ps1");
    let run = |answer: &str, sleep: &str| {
        format!(
            "@echo off\r\nset \"OSF_FAKE_ANSWER={answer}\"\r\n{sleep}\
             powershell -NoProfile -ExecutionPolicy Bypass -File \"{harness}\"\r\n"
        )
    };
    let body = match fake {
        Fake::Answers(answer) | Fake::Records(answer, _) => run(answer, ""),
        Fake::Slow(answer, secs) => run(answer, &format!("set \"OSF_FAKE_SLEEP_SECS={secs}\"\r\n")),
        Fake::Fails(secret) => format!("@echo off\r\necho {secret} 1>&2\r\nexit /b 9\r\n"),
    };
    std::fs::write(bin.join(format!("{agent}.cmd")), &body).expect("fake agent writes");
    if agent == "dsh" {
        std::fs::write(bin.join("sh.cmd"), &body).expect("fake sh writes");
    }
}

/// A folder of fake agent programs, one per reviewer and named as the real
/// agent is, plus the `osf.toml` text that selects them. Put on `PATH`, they
/// stand in for the real agents so no test ever calls a real model.
struct Fakes {
    bin: TempDir,
    osf_toml: String,
}

impl Fakes {
    fn new(prefix: &str, reviewers: &[(&str, Fake)]) -> Fakes {
        let bin = TempDir::new("review-run-fake-agents");
        for (agent, fake) in reviewers {
            write_fake_agent(&bin, agent, fake);
        }
        let names: Vec<&str> = reviewers.iter().map(|(agent, _)| *agent).collect();
        Fakes {
            bin,
            osf_toml: agents_toml(prefix, &names),
        }
    }

    /// `osf.toml` text that also pins opencode to a qwen model, so its family is known.
    fn osf_toml_with_qwen_opencode(&self) -> String {
        format!(
            "{}[agents.models]\nopencode = \"openrouter/qwen/qwen3-coder-next\"\n",
            self.osf_toml
        )
    }

    fn run(&self, dir: &Path, home: &Path, args: &[&str]) -> std::process::Output {
        self.run_with_env(dir, home, &[], args)
    }

    /// Runs `osf` with the fake agents first on `PATH`, ahead of the `PATH`
    /// in `env` when it names one.
    fn run_with_env(
        &self,
        dir: &Path,
        home: &Path,
        env: &[(&str, &str)],
        args: &[&str],
    ) -> std::process::Output {
        let base = env.iter().find(|(k, _)| *k == "PATH").map_or_else(
            || std::env::var("PATH").unwrap_or_default(),
            |(_, v)| (*v).to_string(),
        );
        let dirs = std::iter::once(self.bin.to_path_buf()).chain(std::env::split_paths(&base));
        let path = std::env::join_paths(dirs)
            .expect("PATH joins")
            .to_string_lossy()
            .into_owned();
        let mut all: Vec<(&str, &str)> =
            env.iter().copied().filter(|(k, _)| *k != "PATH").collect();
        all.push(("PATH", &path));
        common::run_osf_with_env(dir, home, &all, args)
    }
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
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("evidence-grade", &fakes.osf_toml);
    let home = common::isolated_home("review-run-evidence-grade");
    let output = fakes.run(
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
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("both-pass", &fakes.osf_toml);
    let home = common::isolated_home("review-run-both-pass");
    let output = fakes.run(
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
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("blocker.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("blocker-fails", &fakes.osf_toml);
    let home = common::isolated_home("review-run-blocker");
    let output = fakes.run(
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

/// One family alone: the interim policy lets it decide the lens through its
/// own extra critical round, rather than blocking the review on a second
/// family nobody has configured.
#[test]
fn only_one_family_enabled_passes_under_the_interim_policy() {
    let fakes = Fakes::new("", &[("codex", Fake::Answers(&fixture("valid.json")))]);
    let repo = review_repo("one-family", &fakes.osf_toml);
    let home = common::isolated_home("review-run-one-family");
    let output = fakes.run(
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

/// Two families are configured but one times out on every round: the
/// roster offers two families, so one answer is no quorum and the lens
/// could not run.
#[test]
fn a_family_that_times_out_leaves_the_lens_could_not_run() {
    let fakes = Fakes::new(
        &format!("[review]\ntimeout_seconds = {TIMEOUT_SECS}\n\n"),
        &[
            ("codex", Fake::Slow(&fixture("valid.json"), SLOW_SECS)),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("one-family-times-out", &fakes.osf_toml);
    let home = common::isolated_home("review-run-one-family-times-out");
    let output = fakes.run(
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
    assert!(stdout.contains("only one family answered"), "{stdout}");
    let journal = journal_text(&home);
    let answered = journal
        .lines()
        .filter(|l| l.contains("\"reviewer\":\"claude\"") && l.contains("\"result\":\"answered\""))
        .count();
    assert_eq!(answered, 2, "two rounds, no critical round: {journal}");
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
    let fakes = Fakes::new(
        &format!("[review]\ntimeout_seconds = {TIMEOUT_SECS}\n\n"),
        &[("codex", Fake::Slow(&fixture("valid.json"), SLOW_SECS))],
    );
    let repo = review_repo("configured-timeout", &fakes.osf_toml);
    let home = common::isolated_home("review-run-configured-timeout");
    let started = std::time::Instant::now();
    let output = fakes.run(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main"],
    );
    assert!(
        // One family enabled earns the interim policy's extra critical
        // round: up to three 1-second timeouts in a row, plus process
        // overhead, well under the 300-second default this guards against.
        started.elapsed() < std::time::Duration::from_secs(3 * TIMEOUT_SECS + 12),
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
    let fakes = Fakes::new("", &[("codex", Fake::Fails(&secret))]);
    let repo = review_repo("secret-stderr", &fakes.osf_toml);
    let home = common::isolated_home("review-run-secret-stderr");
    let sarif_out = repo.dir.join("out.sarif");
    let output = fakes.run(
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
/// claude (anthropic) and opencode (qwen) all enabled, claude is
/// left out and the other two still answer and reach quorum.
#[test]
fn a_reviewer_whose_family_built_the_change_is_left_out_and_two_others_still_pass() {
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
            ("opencode", Fake::Answers(&fixture("valid.json"))),
        ],
    );
    let repo = review_repo_built_by(
        "builder-family-skip",
        &fakes.osf_toml_with_qwen_opencode(),
        "Claude Sonnet 5",
    );
    let home = common::isolated_home("review-run-builder-family-skip");
    let output = fakes.run(
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
        "two rounds each for codex and opencode, plus one skipped record for claude: {types:?}"
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

/// With only codex (openai) and claude (anthropic) enabled, and the
/// change built by Claude, claude is left out and only one family
/// (openai) is left to try: the interim policy runs codex for one extra
/// critical round and lets it decide the lens on its own, naming the
/// policy rather than blocking the review on the family the builder used.
#[test]
fn one_non_builder_family_passes_under_the_interim_policy() {
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo_built_by("builder-family-interim", &fakes.osf_toml, "Claude Sonnet 5");
    let home = common::isolated_home("review-run-builder-family-interim");
    let output = fakes.run(
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

/// The journal lines that name `reviewer`, joined.
fn journal_lines_of(home: &Path, reviewer: &str) -> String {
    journal_text(home)
        .lines()
        .filter(|l| l.contains(&format!("\"reviewer\":\"{reviewer}\"")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// opencode runs models from any family, so its family comes from its model:
/// here a Claude model, which is anthropic, the same as the builder's. It
/// sits the change out, and codex alone decides the lens.
#[test]
fn opencode_on_a_claude_model_is_left_out_of_a_claude_built_change() {
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("opencode", Fake::Answers(&fixture("valid.json"))),
        ],
    );
    let osf_toml = format!(
        "{}[agents.models]\nopencode = \"openrouter/anthropic/claude-sonnet-5\"\n",
        fakes.osf_toml
    );
    let repo = review_repo_built_by("opencode-claude-model", &osf_toml, "Claude Sonnet 5");
    let home = common::isolated_home("review-run-opencode-claude-model");
    let output = fakes.run(
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
    let opencode = journal_lines_of(&home, "opencode");
    assert!(opencode.contains("\"result\":\"skipped\""), "{opencode}");
    assert!(opencode.contains("\"family\":\"anthropic\""), "{opencode}");
    assert!(!opencode.contains("\"result\":\"answered\""), "{opencode}");
}

/// The same agent on a qwen model is a different family from an anthropic
/// builder, so it runs and counts as the second family.
#[test]
fn opencode_on_a_qwen_model_runs_for_a_claude_built_change() {
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("opencode", Fake::Answers(&fixture("valid.json"))),
        ],
    );
    let osf_toml = format!(
        "{}[agents.models]\nopencode = \"openrouter/qwen/qwen3-coder-next\"\n",
        fakes.osf_toml
    );
    let repo = review_repo_built_by("opencode-qwen-model", &osf_toml, "Claude Sonnet 5");
    let home = common::isolated_home("review-run-opencode-qwen-model");
    let output = fakes.run(
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
    let opencode = journal_lines_of(&home, "opencode");
    assert!(opencode.contains("\"family\":\"qwen\""), "{opencode}");
    assert!(opencode.contains("\"result\":\"answered\""), "{opencode}");
}

/// A model that names no known family cannot be checked against the
/// builder's family, so that reviewer never runs: could-not-run, with the
/// reason in the journal. Without a second family the lens could not run.
#[test]
fn opencode_on_an_unknown_model_is_could_not_run_and_never_started() {
    let fakes = Fakes::new("", &[("opencode", Fake::Answers(&fixture("valid.json")))]);
    let osf_toml = format!(
        "{}[agents.models]\nopencode = \"acme/frobnicator-1\"\n",
        fakes.osf_toml
    );
    let repo = review_repo_built_by("opencode-unknown-model", &osf_toml, "Claude Sonnet 5");
    let home = common::isolated_home("review-run-opencode-unknown-model");
    let output = fakes.run(
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
    let opencode = journal_lines_of(&home, "opencode");
    assert!(
        opencode.contains("\"result\":\"could-not-run\""),
        "{opencode}"
    );
    assert!(opencode.contains("acme/frobnicator-1"), "{opencode}");
    assert!(!opencode.contains("\"result\":\"answered\""), "{opencode}");
}

/// No `Code-Generator:` trailer at all on the reviewed commit records the
/// builder family as `unknown`, and every enabled reviewer still runs, the
/// same as before this module knew about builder families at all.
#[test]
fn no_builder_family_detected_records_unknown_and_runs_every_reviewer() {
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("no-builder-family", &fakes.osf_toml);
    let home = common::isolated_home("review-run-no-builder-family");
    let output = fakes.run(
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
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
            ("opencode", Fake::Answers(&fixture("valid.json"))),
        ],
    );
    let repo = review_repo(
        "builder-family-flag-override",
        &fakes.osf_toml_with_qwen_opencode(),
    );
    let home = common::isolated_home("review-run-builder-family-flag-override");
    let output = fakes.run(
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
/// `changeset_risk::assess` reads it straight from TOML, but `config::review_config`
/// also parses the same `[review]` table with `deny_unknown_fields`, so a
/// hot-path test that only calls `changeset_risk::assess` directly can pass while the
/// full command still fails on the field `review_config` does not know.
#[test]
fn hot_paths_in_osf_toml_does_not_fail_the_full_review_run_command() {
    // A glob that never matches the reviewed change: this test is only
    // about `hot_paths` parsing, not about earning the "high-traffic path"
    // signal, which would select lenses beyond the one this test's fixture
    // repository is set up to answer for.
    let fakes = Fakes::new(
        "[review]\nhot_paths = [\"never/matches/anything.rs\"]\n\n",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("hot-paths-full-command", &fakes.osf_toml);
    let home = common::isolated_home("review-run-hot-paths-full-command");
    let output = fakes.run(
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
/// `[review]` and `[agents]` tables (reviewers, threshold, timeout, cost ceiling) come from
/// there, never from the repository under review. A pull request that
/// lowers its own threshold to zero and swaps in a harmless roster must
/// still fail, because the base tree's own high threshold and its
/// blocker-returning roster are what actually run.
#[test]
fn a_pull_request_tree_cannot_loosen_review_via_its_own_config_root() {
    let config_root = TempDir::new("review-run-config-root-base");
    write_lens_overrides_to(&config_root);
    let base = Fakes::new(
        "[review]\nthreshold = 0.9\n\n",
        &[
            ("codex", Fake::Answers(&fixture("blocker.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    std::fs::write(config_root.join("osf.toml"), &base.osf_toml).expect("base osf.toml writes");

    // The reviewed tree carries its own low threshold and a roster that
    // would only ever answer clean. If either of these were read instead
    // of the base tree's, the review would pass.
    let pr_osf_toml = agents_toml("[review]\nthreshold = 0.0\n\n", &["omp"]);
    let repo = review_repo("config-root-pr-tree", &pr_osf_toml);
    let home = common::isolated_home("review-run-config-root");
    let output = base.run(
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
    let fakes = Fakes::new(
        "[review]\nthreshold = 0.0\n\n",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("no-config-root", &fakes.osf_toml);
    let home = common::isolated_home("review-run-no-config-root");
    let output = fakes.run(
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
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("blocker.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("warn-only-blocker", &fakes.osf_toml);
    let home = common::isolated_home("review-run-warn-only-blocker");
    let output = fakes.run(
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
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("blocker.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("post-to-blocker", &fakes.osf_toml);
    let home = common::isolated_home("review-run-post-to-blocker");
    let gh_holder = TempDir::new("review-run-post-to-blocker-gh");
    let gh_dir = write_fake_gh(&gh_holder, "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef");
    let path = path_with_dir_first(&gh_dir);
    let output = fakes.run_with_env(
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
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("blocker.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("post-to-boom", &fakes.osf_toml);
    let home = common::isolated_home("review-run-post-to-boom");
    let gh_holder = TempDir::new("review-run-post-to-boom-gh");
    let gh_dir = write_fake_gh(&gh_holder, "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef");
    let path = path_with_dir_first(&gh_dir);
    let output = fakes.run_with_env(
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
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("journal-unwritable", &fakes.osf_toml);
    let home = common::isolated_home("review-run-journal-unwritable");
    let blocked_state_dir = home.join("state-is-a-file");
    std::fs::write(&blocked_state_dir, "not a directory").expect("blocked state dir file writes");
    let state_dir_arg = blocked_state_dir.to_string_lossy().into_owned();
    let output = fakes.run_with_env(
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
    let fakes = Fakes::new("", &[("codex", Fake::Answers(&answer_path))]);
    let repo = review_repo("secret-body", &fakes.osf_toml);
    let home = common::isolated_home("review-run-secret-body");
    let sarif_out = repo.dir.join("out.sarif");
    let output = fakes.run(
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
    let fakes = Fakes::new("", &[("codex", Fake::Answers(&answer_path))]);
    let repo = review_repo("secret-lens", &fakes.osf_toml);
    let home = common::isolated_home("review-run-secret-lens");
    let sarif_out = repo.dir.join("out.sarif");
    let output = fakes.run(
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
    let fakes = Fakes::new("", &[("codex", Fake::Answers(&answer_path))]);
    let repo = review_repo("secret-severity", &fakes.osf_toml);
    let home = common::isolated_home("review-run-secret-severity");
    let sarif_out = repo.dir.join("out.sarif");
    let output = fakes.run(
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
    let fakes = Fakes::new("", &[("codex", Fake::Answers(&answer_path))]);
    let repo = review_repo("secret-extra-field", &fakes.osf_toml);
    let home = common::isolated_home("review-run-secret-extra-field");
    let sarif_out = repo.dir.join("out.sarif");
    let output = fakes.run(
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
    let fakes = Fakes::new("", &[("codex", Fake::Answers(&fixture("valid.json")))]);
    let repo = review_repo("if-enabled-on-interim", &fakes.osf_toml);
    let home = common::isolated_home("review-run-if-enabled-on");
    let output = fakes.run(
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
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("base-from-env", &fakes.osf_toml);
    let home = common::isolated_home("review-run-base-from-env");
    let output = fakes.run_with_env(
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
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&answer_path)),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("secret-path", &fakes.osf_toml);
    let home = common::isolated_home("review-run-secret-path");
    let sarif_out = repo.dir.join("out.sarif");
    let output = fakes.run(
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

/// What a recording fake left under `dir`: its prompt, arguments or environment.
#[cfg(unix)]
fn recorded(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).expect("the fake harness recorded this file")
}

/// A trusted config root holding the lens overrides and `osf.toml` with `osf_toml`.
#[cfg(unix)]
fn trusted_root(name: &str, osf_toml: &str) -> TempDir {
    let root = TempDir::new(name);
    write_lens_overrides_to(&root);
    std::fs::write(root.join("osf.toml"), osf_toml).expect("trusted osf.toml writes");
    root
}

/// A prompt file the trusted config names is the one a reviewer gets, with
/// its placeholders filled in and the answer format after it.
#[test]
#[cfg(unix)]
fn a_prompt_file_named_by_the_trusted_config_is_used() {
    let rec = TempDir::new("review-run-prompt-trusted-rec");
    let fakes = Fakes::new(
        "",
        &[("codex", Fake::Records(&fixture("valid.json"), &rec))],
    );
    let config = trusted_root(
        "review-run-prompt-trusted-base",
        &format!("[review]\nprompt_file = \"mine.md\"\n\n{}", fakes.osf_toml),
    );
    std::fs::write(
        config.join("mine.md"),
        "TRUSTED-FRAME for {lens_name}\n{metadata}\n",
    )
    .expect("prompt file writes");
    let repo = review_repo("prompt-trusted", &fakes.osf_toml);
    let home = common::isolated_home("review-run-prompt-trusted");
    let output = fakes.run(
        &repo.dir,
        &home,
        &[
            "review",
            "run",
            "--base",
            "origin/main",
            "--config-root",
            &config.to_string_lossy(),
        ],
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let prompt = recorded(&rec, "prompt");
    assert!(prompt.contains("TRUSTED-FRAME for correctness"), "{prompt}");
    assert!(prompt.contains("review-answer schema"), "{prompt}");
    assert!(!prompt.contains("read-only checkout"), "{prompt}");
}

/// A pull request that names its own prompt file in its own `osf.toml`
/// cannot rewrite its reviewer's instructions: only the trusted config counts.
#[test]
#[cfg(unix)]
fn a_prompt_file_named_by_the_pull_requests_own_config_is_ignored() {
    let rec = TempDir::new("review-run-prompt-untrusted-rec");
    let fakes = Fakes::new(
        "",
        &[("codex", Fake::Records(&fixture("valid.json"), &rec))],
    );
    let config = trusted_root("review-run-prompt-untrusted-base", &fakes.osf_toml);
    let repo = review_repo_with(
        "prompt-untrusted",
        &format!("[review]\nprompt_file = \"evil.md\"\n\n{}", fakes.osf_toml),
        &[("evil.md", "UNTRUSTED-FRAME {metadata}\n")],
    );
    let home = common::isolated_home("review-run-prompt-untrusted");
    let output = fakes.run(
        &repo.dir,
        &home,
        &[
            "review",
            "run",
            "--base",
            "origin/main",
            "--config-root",
            &config.to_string_lossy(),
        ],
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let prompt = recorded(&rec, "prompt");
    assert!(!prompt.contains("UNTRUSTED-FRAME"), "{prompt}");
    assert!(prompt.contains("read-only checkout"), "{prompt}");
}

/// The reviewer's prompt holds the metadata and no diff: the pull request,
/// both commits, the changed files with their line counts, and the work
/// item, with a secret in text osf inserts redacted.
#[test]
#[cfg(unix)]
fn the_prompt_carries_metadata_and_no_diff() {
    let rec = TempDir::new("review-run-metadata-rec");
    let fakes = Fakes::new(
        "",
        &[("codex", Fake::Records(&fixture("valid.json"), &rec))],
    );
    let repo = review_repo("metadata", &fakes.osf_toml);
    let inputs = TempDir::new("review-run-metadata-inputs");
    let secret = common::fake_forge_token("ghp_");
    let pr = inputs.join("pr.json");
    std::fs::write(
        &pr,
        serde_json::json!({
            "number": 178,
            "title": "Add the review check",
            "body": format!("Reviewers, read this. {secret}"),
        })
        .to_string(),
    )
    .expect("pull request file writes");
    let work_item = inputs.join("issue.md");
    std::fs::write(&work_item, "# A work item\n\nWORK-ITEM-TEXT\n").expect("work item writes");
    let home = common::isolated_home("review-run-metadata");
    let output = fakes.run(
        &repo.dir,
        &home,
        &[
            "review",
            "run",
            "--base",
            "origin/main",
            "--pull-request",
            &pr.to_string_lossy(),
            "--work-item",
            &work_item.to_string_lossy(),
        ],
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let prompt = recorded(&rec, "prompt");
    let base = repo.git(&["rev-parse", "origin/main"]);
    let head = repo.git(&["rev-parse", "HEAD"]);
    assert!(prompt.contains("number: 178"), "{prompt}");
    assert!(prompt.contains("title: Add the review check"), "{prompt}");
    assert!(
        prompt.contains(&format!("base: {}", base.trim())),
        "{prompt}"
    );
    assert!(
        prompt.contains(&format!("head: {}", head.trim())),
        "{prompt}"
    );
    assert!(prompt.contains("README.md (+1 -0)"), "{prompt}");
    assert!(prompt.contains("WORK-ITEM-TEXT"), "{prompt}");
    assert!(
        prompt.contains("(none touched or linked by this change)"),
        "{prompt}"
    );
    assert!(
        !prompt.contains("plus a change to review"),
        "no diff text: {prompt}"
    );
    assert!(!prompt.contains(&secret), "{prompt}");
    assert!(prompt.contains("redacted by scan-secret"), "{prompt}");
}

/// Each agent that documents a read-only mode starts with exactly those
/// flags or that setting.
#[test]
#[cfg(unix)]
fn each_agent_starts_with_its_read_only_settings() {
    let cases: [(&str, &str, &str, &str); 3] = [
        ("codex", "valid.json", "--sandbox\nread-only\n", ""),
        (
            "claude",
            "claude-envelope.json",
            "--restricted\n--tools\nRead,Grep,Glob\n--add-dir\n/",
            "",
        ),
        (
            "opencode",
            "valid.json",
            "",
            "OPENCODE_PERMISSION={\"edit\":\"deny\"",
        ),
    ];
    for (agent, answer, expected_args, expected_env) in cases {
        let rec = TempDir::new("review-run-read-only-rec");
        let fakes = Fakes::new("", &[(agent, Fake::Records(&fixture(answer), &rec))]);
        let repo = review_repo(
            &format!("read-only-{agent}"),
            &fakes.osf_toml_with_qwen_opencode(),
        );
        let home = common::isolated_home(&format!("review-run-read-only-{agent}"));
        let output = fakes.run(
            &repo.dir,
            &home,
            &["review", "run", "--base", "origin/main"],
        );
        assert!(
            output.status.success(),
            "{agent}: stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let args = recorded(&rec, "args");
        assert!(args.contains(expected_args), "{agent}: {args}");
        let env = recorded(&rec, "env");
        assert!(env.contains(expected_env), "{agent}: {env}");
        // No agent is given a shell: no Bash tool, and no git allowance.
        assert!(
            !args.contains("Bash") && !args.contains("allowedTools"),
            "{agent}: {args}"
        );
        assert!(
            !env.contains("git diff") && !env.contains("git log"),
            "{agent}: {env}"
        );
    }
}

/// The reviewer's folder holds the diff and the commit log, and is named
/// to the agent's own settings: claude's `--add-dir` and opencode's
/// `external_directory` allowance, both the very folder the prompt names.
#[test]
#[cfg(unix)]
fn the_agents_settings_name_the_folder_that_holds_the_diff() {
    for agent in ["claude", "opencode"] {
        let answer = if agent == "claude" {
            "claude-envelope.json"
        } else {
            "valid.json"
        };
        let rec = TempDir::new("review-run-folder-settings-rec");
        let fakes = Fakes::new("", &[(agent, Fake::Records(&fixture(answer), &rec))]);
        let repo = review_repo(
            &format!("folder-settings-{agent}"),
            &fakes.osf_toml_with_qwen_opencode(),
        );
        let home = common::isolated_home(&format!("review-run-folder-settings-{agent}"));
        let output = fakes.run(
            &repo.dir,
            &home,
            &["review", "run", "--base", "origin/main"],
        );
        assert!(
            output.status.success(),
            "{agent}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let file = recorded(&rec, "change.path");
        let dir = Path::new(file.trim())
            .parent()
            .expect("file has a folder")
            .to_path_buf();
        let dir = dir.to_string_lossy().into_owned();
        if agent == "claude" {
            let args = recorded(&rec, "args");
            assert!(args.contains(&format!("--add-dir\n{dir}\n")), "{args}");
        } else {
            let env = recorded(&rec, "env");
            assert!(env.contains("\"bash\":\"deny\""), "{env}");
            assert!(
                env.contains(&format!(
                    "\"external_directory\":{{\"{dir}/**\":\"allow\"}}"
                )),
                "{env}"
            );
        }
    }
}

/// Before the reviewer starts, osf writes the commit log and the full diff of
/// the range into a read-only folder and names the file in the prompt. The
/// folder is gone after the run.
#[test]
#[cfg(unix)]
fn the_reviewer_reads_the_diff_and_log_from_a_read_only_file() {
    let rec = TempDir::new("review-run-change-file-rec");
    let fakes = Fakes::new(
        "",
        &[("codex", Fake::Records(&fixture("valid.json"), &rec))],
    );
    let repo = review_repo("change-file", &fakes.osf_toml);
    let home = common::isolated_home("review-run-change-file");
    let output = fakes.run(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let prompt = recorded(&rec, "prompt");
    let path = recorded(&rec, "change.path");
    assert!(
        prompt.contains(path.trim()),
        "the prompt names the file: {prompt}"
    );
    let change = recorded(&rec, "change");
    assert!(
        change.contains("+plus a change to review"),
        "the diff: {change}"
    );
    assert!(
        change.contains("a small change to review"),
        "the log: {change}"
    );
    let modes = recorded(&rec, "change.modes");
    assert_eq!(modes.trim(), "555\n444", "folder and file are read-only");
    assert!(
        !Path::new(path.trim()).exists(),
        "the folder is removed after the run"
    );
}

/// An agent with no documented read-only mode is could-not-run with that
/// reason, and its harness never starts.
#[test]
#[cfg(unix)]
fn an_agent_with_no_read_only_mode_is_could_not_run_and_never_starts() {
    let rec = TempDir::new("review-run-no-read-only-rec");
    let fakes = Fakes::new("", &[("dsh", Fake::Records(&fixture("valid.json"), &rec))]);
    let repo = review_repo("no-read-only", &fakes.osf_toml);
    let home = common::isolated_home("review-run-no-read-only");
    let output = fakes.run(
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
    let journal = journal_lines_of(&home, "dsh");
    assert!(journal.contains("no read-only mode"), "{journal}");
    assert!(
        journal.contains("\"result\":\"could-not-run\""),
        "{journal}"
    );
    assert!(
        !rec.join("started").exists(),
        "the harness must never start"
    );
}

/// The text a run prints, which names each lens's verdict and the whole one.
fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// One reviewer job per reviewer, then one reduce, reaches the same
/// decision, printed the same way, as the single run does.
#[test]
fn one_run_per_reviewer_then_reduce_reaches_the_single_runs_decision() {
    for (label, codex_answer, expected) in [
        ("pass", fixture("valid.json"), 0),
        ("blocker", fixture("blocker.json"), 1),
    ] {
        let fakes = Fakes::new(
            "",
            &[
                ("codex", Fake::Answers(&codex_answer)),
                ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
            ],
        );
        let repo = review_repo(&format!("split-{label}"), &fakes.osf_toml);
        let single_home = common::isolated_home(&format!("review-run-single-{label}"));
        let single = fakes.run(
            &repo.dir,
            &single_home,
            &["review", "run", "--base", "origin/main"],
        );
        assert_eq!(single.status.code(), Some(expected), "{label}");

        let saved = TempDir::new(&format!("review-run-split-{label}"));
        let mut files = Vec::new();
        for name in ["codex", "claude"] {
            let file = saved
                .join(format!("{name}.json"))
                .to_string_lossy()
                .into_owned();
            let home = common::isolated_home(&format!("review-run-split-{label}-{name}"));
            let job = fakes.run(
                &repo.dir,
                &home,
                &[
                    "review",
                    "run",
                    "--reviewer",
                    name,
                    "--out",
                    &file,
                    "--base",
                    "origin/main",
                ],
            );
            assert!(
                job.status.success(),
                "{label} {name}: {}",
                String::from_utf8_lossy(&job.stderr)
            );
            files.push(file);
        }
        let reduce_home = common::isolated_home(&format!("review-run-reduce-{label}"));
        let mut args = vec!["review", "reduce", "--base", "origin/main"];
        args.extend(files.iter().map(String::as_str));
        let reduced = fakes.run(&repo.dir, &reduce_home, &args);
        assert_eq!(reduced.status.code(), Some(expected), "{label}");
        assert_eq!(stdout_of(&single), stdout_of(&reduced), "{label}");
        let types = journal_event_types(&reduce_home);
        assert_eq!(
            types.iter().filter(|t| *t == "review-answer").count(),
            4,
            "{label}: {types:?}"
        );
    }
}

/// A reviewer whose job left no file counts as could-not-run, never as a pass.
#[test]
fn a_reviewer_with_no_saved_run_is_could_not_run_at_reduce() {
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("reduce-missing", &fakes.osf_toml);
    let saved = TempDir::new("review-run-reduce-missing");
    let file = saved.join("codex.json").to_string_lossy().into_owned();
    let home = common::isolated_home("review-run-reduce-missing-job");
    let job = fakes.run(
        &repo.dir,
        &home,
        &[
            "review",
            "run",
            "--reviewer",
            "codex",
            "--out",
            &file,
            "--base",
            "origin/main",
        ],
    );
    assert!(job.status.success());
    let reduce_home = common::isolated_home("review-run-reduce-missing");
    let reduced = fakes.run(
        &repo.dir,
        &reduce_home,
        &["review", "reduce", "--base", "origin/main", &file],
    );
    assert_eq!(reduced.status.code(), Some(2));
    assert!(
        stdout_of(&reduced).contains("only one family answered"),
        "{}",
        stdout_of(&reduced)
    );
    let journal = journal_lines_of(&reduce_home, "claude");
    assert!(journal.contains("left no answer"), "{journal}");
}

/// A reviewer outside the roster is an error, unless `--if-enabled` asks
/// for a quiet skip, which still leaves an empty file for the reduce step.
#[test]
fn a_reviewer_outside_the_roster_is_an_error_or_a_quiet_skip() {
    let fakes = Fakes::new("", &[("codex", Fake::Answers(&fixture("valid.json")))]);
    let repo = review_repo("reviewer-outside-roster", &fakes.osf_toml);
    let saved = TempDir::new("review-run-outside-roster");
    let file = saved.join("claude.json").to_string_lossy().into_owned();
    let home = common::isolated_home("review-run-outside-roster");
    let base_args = [
        "review",
        "run",
        "--reviewer",
        "claude",
        "--out",
        &file,
        "--base",
        "origin/main",
    ];
    let refused = fakes.run(&repo.dir, &home, &base_args);
    assert_eq!(refused.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("not in the roster"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    let mut quiet = base_args.to_vec();
    quiet.push("--if-enabled");
    let skipped = fakes.run(&repo.dir, &home, &quiet);
    assert!(skipped.status.success());
    assert!(
        stdout_of(&skipped).contains("slot off"),
        "{}",
        stdout_of(&skipped)
    );
    let text = std::fs::read_to_string(&file).expect("an empty reviewer file is written");
    assert!(text.contains("\"lenses\": []"), "{text}");
}
