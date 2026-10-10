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

/// [`review_repo_with`], with each of `links` committed in the base commit as a
/// symbolic link named by the first item and pointing at the second.
#[cfg(unix)]
fn review_repo_linked(
    name: &str,
    osf_toml: &str,
    extra: &[(&str, &str)],
    links: &[(&str, &Path)],
) -> TempRepo {
    let repo = TempRepo::new(name);
    write_lens_overrides(&repo);
    for (path, content) in extra {
        repo.write(path, content);
    }
    for (link, target) in links {
        std::os::unix::fs::symlink(target, repo.dir.join(link)).expect("symlink creates");
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
    /// Prints the answer file's text, with `@@KEY@@`, `@@KEY_B64@@` and
    /// `@@KEY_HEX@@` replaced by the named environment variable's value in
    /// plain, base64 and hex form, as a reviewer that echoes its own key would.
    Echoes(&'a str, &'a str),
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
             OSF_FAKE_CWD_CAPTURE='{d}/cwd' exec '{harness}' \"$@\"",
            d = dir.display()
        ),
        Fake::Echoes(answer, var) => {
            format!("OSF_FAKE_ANSWER='{answer}' OSF_FAKE_ECHO_ENV='{var}' exec '{harness}'")
        }
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
        Fake::Answers(answer) | Fake::Records(answer, _) | Fake::Echoes(answer, _) => {
            run(answer, "")
        }
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
    /// in `env` when it names one. A saved run and the reduce step need a
    /// binding, so those commands get the flags for [`binding_flags`] added.
    fn run_with_env(
        &self,
        dir: &Path,
        home: &Path,
        env: &[(&str, &str)],
        args: &[&str],
    ) -> std::process::Output {
        let saves = args.contains(&"--reviewer");
        let reduces = args.windows(2).any(|pair| pair == ["review", "reduce"]);
        if !(saves || reduces) {
            return self.run_unbound_with_env(dir, home, env, args);
        }
        let flags = binding_flags(dir, SAVED_RUN_ID);
        let mut all: Vec<&str> = args.to_vec();
        all.extend(flags.iter().map(String::as_str));
        self.run_unbound_with_env(dir, home, env, &all)
    }

    /// [`Fakes::run_with_env`] with exactly the arguments given.
    fn run_unbound_with_env(
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

/// The CI run id the tests bind saved runs to.
const SAVED_RUN_ID: &str = "1001";

/// The repository the tests bind saved runs to.
const SAVED_REPOSITORY: &str = "owner/repo";

/// The pull request number the tests bind saved runs to.
const SAVED_PULL_REQUEST: u64 = 7;

/// The commit `rev` names in the repository at `dir`.
fn commit_of(dir: &Path, rev: &str) -> String {
    let mut command = std::process::Command::new("git");
    command.current_dir(dir).args(["rev-parse", rev]);
    osf::scrub_git_env(&mut command);
    let out = command.output().expect("git runs");
    assert!(out.status.success(), "git rev-parse {rev} failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The flags that bind a saved run, and the reduce step, to one pull
/// request and CI run `run_id`, at the repository's current head.
fn binding_flags(dir: &Path, run_id: &str) -> Vec<String> {
    [
        "--repository",
        SAVED_REPOSITORY,
        "--pull-request-number",
        &SAVED_PULL_REQUEST.to_string(),
        "--head",
        &commit_of(dir, "HEAD"),
        "--ci-run-id",
        run_id,
    ]
    .iter()
    .map(ToString::to_string)
    .collect()
}

/// The binding of a run saved by [`SAVED_RUN_ID`]'s jobs in the repository at `dir`, as JSON.
#[cfg(unix)]
fn binding_value(dir: &Path) -> serde_json::Value {
    serde_json::json!({
        "repository": SAVED_REPOSITORY,
        "pull_request": SAVED_PULL_REQUEST,
        "base": commit_of(dir, "origin/main"),
        "head": commit_of(dir, "HEAD"),
        "run_id": SAVED_RUN_ID,
    })
}

/// Writes `content` to `name` under `dir`, and returns its absolute path as a string.
fn write_answer_file(dir: &TempDir, name: &str, content: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, content).expect("answer fixture writes");
    path.to_string_lossy().into_owned()
}

/// Every `event_type` value found in a journal file's own lines.
fn journal_event_types(home: &Path) -> Vec<String> {
    let runs_dir = home.join(".osf/state/runs");
    let mut types = Vec::new();
    let Ok(entries) = std::fs::read_dir(&runs_dir) else {
        return types;
    };
    for entry in entries {
        let entry = entry.expect("dir entry reads");
        let text = std::fs::read_to_string(entry.path()).expect("journal reads");
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

/// The payload of the single `review-decision` event in the journal
/// under `home`.
fn journal_review_decision(home: &Path) -> serde_json::Value {
    let runs_dir = home.join(".osf/state/runs");
    let entries = std::fs::read_dir(&runs_dir).expect("journal dir reads");
    let mut found = None;
    for entry in entries {
        let entry = entry.expect("dir entry reads");
        let text = std::fs::read_to_string(entry.path()).expect("journal reads");
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

/// The whole content of every journal file under `home`, concatenated.
fn journal_text(home: &Path) -> String {
    let runs_dir = home.join(".osf/state/runs");
    let mut text = String::new();
    let Ok(entries) = std::fs::read_dir(&runs_dir) else {
        return text;
    };
    for entry in entries {
        let entry = entry.expect("dir entry reads");
        text.push_str(&std::fs::read_to_string(entry.path()).expect("journal reads"));
    }
    text
}

/// Asserts `secret` is nowhere in `output`'s standard output or standard
/// error, nor in any journal file under `home`, nor (when given) in
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

/// The payload of the first `review-answer` event in the journal
/// under `home`.
fn journal_first_review_answer(home: &Path) -> serde_json::Value {
    let runs_dir = home.join(".osf/state/runs");
    let entries = std::fs::read_dir(&runs_dir).expect("journal dir reads");
    for entry in entries {
        let entry = entry.expect("dir entry reads");
        let text = std::fs::read_to_string(entry.path()).expect("journal reads");
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
/// family that has not been configured.
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

/// Two families are configured but one times out on every round: only one
/// family answered, so under the interim policy the working family's
/// critical round counts and decides the lens.
#[test]
fn a_family_that_times_out_leaves_the_working_family_its_critical_round() {
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
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("interim policy"), "{stdout}");
    let journal = journal_text(&home);
    let answered = journal
        .lines()
        .filter(|l| l.contains("\"reviewer\":\"claude\"") && l.contains("\"result\":\"answered\""))
        .count();
    assert_eq!(answered, 3, "two rounds plus the critical round: {journal}");
}

/// Both families answer: each saved run holds a critical round marked as
/// such, and the reduce step ignores every critical round, so the lens is
/// decided by the two families' own two rounds and the interim policy is
/// not named.
#[test]
fn a_critical_round_is_saved_by_every_reviewer_and_ignored_when_both_families_answer() {
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("critical-ignored", &fakes.osf_toml);
    let saved = TempDir::new("review-run-critical-ignored");
    let mut files = Vec::new();
    for name in ["codex", "claude"] {
        let file = saved
            .join(format!("{name}.json"))
            .to_string_lossy()
            .into_owned();
        let home = common::isolated_home(&format!("review-run-critical-ignored-{name}"));
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
            "{name}: {}",
            String::from_utf8_lossy(&job.stderr)
        );
        let run: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).expect("saved run reads"))
                .expect("saved run is JSON");
        let flags: Vec<bool> = run
            .pointer("/lenses/0/attempts")
            .and_then(serde_json::Value::as_array)
            .expect("attempts")
            .iter()
            .map(|a| {
                a.get("critical")
                    .and_then(serde_json::Value::as_bool)
                    .expect("critical flag")
            })
            .collect();
        assert_eq!(flags, vec![false, false, true], "{name}: {run}");
        files.push(file);
    }
    let reduce_home = common::isolated_home("review-run-critical-ignored-reduce");
    let mut args = vec!["review", "reduce", "--base", "origin/main"];
    args.extend(files.iter().map(String::as_str));
    let reduced = fakes.run(&repo.dir, &reduce_home, &args);
    assert!(reduced.status.success(), "{}", stdout_of(&reduced));
    assert!(
        !stdout_of(&reduced).contains("interim policy"),
        "{}",
        stdout_of(&reduced)
    );
    let journal = journal_text(&reduce_home);
    assert!(
        !journal.contains("\"round\":3"),
        "no critical round is journaled: {journal}"
    );
    let answers = journal
        .lines()
        .filter(|l| l.contains("\"result\":\"answered\""))
        .count();
    assert_eq!(answers, 4, "two rounds for each family: {journal}");
}

/// Saves one reviewer's run for `repo` to `<dir>/<name>.json` through the fake agent.
fn save_run(fakes: &Fakes, repo: &TempRepo, dir: &TempDir, name: &str) -> String {
    let file = dir
        .join(format!("{name}.json"))
        .to_string_lossy()
        .into_owned();
    let home = common::isolated_home(&format!("review-run-save-{name}"));
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
        "{name}: {}",
        String::from_utf8_lossy(&job.stderr)
    );
    file
}

/// Edits the first lens's attempts in the saved run at `file` with `edit`.
fn edit_attempts(file: &str, edit: impl FnOnce(&mut Vec<serde_json::Value>)) {
    let mut run: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(file).expect("saved run reads"))
            .expect("saved run is JSON");
    let attempts = run
        .pointer_mut("/lenses/0/attempts")
        .and_then(serde_json::Value::as_array_mut)
        .expect("attempts");
    edit(attempts);
    std::fs::write(file, run.to_string()).expect("saved run writes");
}

/// Marks the attempt at `index` as one that never answered.
fn fail_attempt(attempts: &mut [serde_json::Value], index: usize) {
    let attempt = attempts.get_mut(index).expect("attempt exists");
    attempt["result"] = serde_json::json!("could-not-run");
    attempt["reason"] = serde_json::json!("timed out");
    attempt["answer"] = serde_json::Value::Null;
}

/// Runs `review reduce` over `files` for `repo`.
fn reduce_saved_files(
    fakes: &Fakes,
    repo: &TempRepo,
    label: &str,
    files: &[&str],
) -> std::process::Output {
    let home = common::isolated_home(&format!("review-run-reduce-{label}"));
    let mut args = vec!["review", "reduce", "--base", "origin/main"];
    args.extend(files.iter().copied());
    fakes.run(&repo.dir, &home, &args)
}

/// Two families that each saved a single answered round do not reach the
/// quorum: a family needs two answered rounds, so the lens is could-not-run.
#[test]
fn a_family_with_one_answered_round_does_not_count_at_reduce() {
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("one-round-each", &fakes.osf_toml);
    let saved = TempDir::new("review-run-one-round-each");
    let codex = save_run(&fakes, &repo, &saved, "codex");
    let claude = save_run(&fakes, &repo, &saved, "claude");
    let control = reduce_saved_files(&fakes, &repo, "one-round-control", &[&codex, &claude]);
    assert!(control.status.success(), "{}", stdout_of(&control));
    for file in [&codex, &claude] {
        edit_attempts(file, |attempts| attempts.truncate(1));
    }
    let reduced = reduce_saved_files(&fakes, &repo, "one-round-each", &[&codex, &claude]);
    assert_eq!(reduced.status.code(), Some(2), "{}", stdout_of(&reduced));
    assert!(
        stdout_of(&reduced).contains("no family answered 2 rounds"),
        "{}",
        stdout_of(&reduced)
    );
}

/// A second round that failed leaves its family out of the quorum. The other
/// family then stands alone, so it needs its critical round, and it has one.
#[test]
fn a_family_whose_second_round_failed_is_left_out_and_the_other_needs_its_critical_round() {
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo("second-round-failed", &fakes.osf_toml);
    let saved = TempDir::new("review-run-second-round-failed");
    let codex = save_run(&fakes, &repo, &saved, "codex");
    let claude = save_run(&fakes, &repo, &saved, "claude");
    edit_attempts(&codex, |attempts| fail_attempt(attempts, 1));
    let alone = reduce_saved_files(&fakes, &repo, "second-round-failed", &[&codex, &claude]);
    assert!(alone.status.success(), "{}", stdout_of(&alone));
    assert!(
        stdout_of(&alone).contains("interim policy"),
        "{}",
        stdout_of(&alone)
    );
    edit_attempts(&claude, |attempts| fail_attempt(attempts, 2));
    let no_critical = reduce_saved_files(&fakes, &repo, "critical-failed", &[&codex, &claude]);
    assert_eq!(
        no_critical.status.code(),
        Some(2),
        "{}",
        stdout_of(&no_critical)
    );
    assert!(
        stdout_of(&no_critical).contains("no critical answer"),
        "{}",
        stdout_of(&no_critical)
    );
}

/// A lone family whose saved run stops after its independent rounds has no
/// critical answer, so it cannot pass on its own.
#[test]
fn a_lone_family_with_no_critical_attempt_is_could_not_run_at_reduce() {
    let fakes = Fakes::new("", &[("codex", Fake::Answers(&fixture("valid.json")))]);
    let repo = review_repo("lone-no-critical", &fakes.osf_toml);
    let saved = TempDir::new("review-run-lone-no-critical");
    let codex = save_run(&fakes, &repo, &saved, "codex");
    let control = reduce_saved_files(&fakes, &repo, "lone-control", &[&codex]);
    assert!(control.status.success(), "{}", stdout_of(&control));
    edit_attempts(&codex, |attempts| attempts.truncate(2));
    let reduced = reduce_saved_files(&fakes, &repo, "lone-no-critical", &[&codex]);
    assert_eq!(reduced.status.code(), Some(2), "{}", stdout_of(&reduced));
    assert!(
        stdout_of(&reduced).contains("no critical answer"),
        "{}",
        stdout_of(&reduced)
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
    assert!(decision.get("score").is_none(), "{decision}");
    assert!(decision.get("threshold").is_none(), "{decision}");
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
/// there. The repository under review cannot change them. A pull request that
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
    let codex_args = if osf::agents::in_factory_container() {
        "--dangerously-bypass-approvals-and-sandbox\n"
    } else {
        "--sandbox\nread-only\n"
    };
    let cases: [(&str, &str, &str, &str); 3] = [
        ("codex", "valid.json", codex_args, ""),
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

/// A reviewer whose job left no file counts as could-not-run for itself. The
/// other family then answered alone, so the interim policy lets its critical
/// round decide the lens.
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
    assert!(reduced.status.success(), "{}", stdout_of(&reduced));
    assert!(
        stdout_of(&reduced).contains("interim policy"),
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

/// A reviewer starts in a clean copy of the change: a temporary folder with
/// the change's files, no coding agent's settings and no symbolic link. The
/// folder is gone after the run.
#[test]
#[cfg(unix)]
fn a_reviewer_starts_in_a_clean_copy_of_the_change_with_no_agent_settings() {
    let rec = TempDir::new("review-run-clean-copy-rec");
    let outside = TempDir::new("review-run-clean-copy-outside");
    std::fs::write(outside.join("secret.txt"), "outside").expect("outside file writes");
    let fakes = Fakes::new(
        "",
        &[("codex", Fake::Records(&fixture("valid.json"), &rec))],
    );
    let repo = review_repo_linked(
        "clean-copy",
        &fakes.osf_toml,
        &[
            (".opencode/plugin/extra.js", "x"),
            ("opencode.json", "{}"),
            ("opencode.jsonc", "{}"),
            (".omp/agent/hooks/a/index.js", "x"),
            (".codex/hooks.json", "{}"),
            (".claude/settings.json", "{}"),
            (".mcp.json", "{}"),
            (".cursor/rules/r.mdc", "x"),
            (".dsh/p.yml", "x"),
            ("AGENTS.md", "x"),
            ("CLAUDE.md", "x"),
            ("docs/AGENTS.md", "x"),
            ("docs/guide.md", "kept"),
        ],
        &[
            ("link-to-file", &outside.join("secret.txt")),
            ("link-to-dir", &outside),
        ],
    );
    let home = common::isolated_home("review-run-clean-copy");
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
    let seen = recorded(&rec, "cwd");
    let mut lines = seen.lines();
    let dir = lines.next().expect("the working folder");
    let files: Vec<&str> = lines.collect();
    assert_ne!(
        Path::new(dir),
        repo.dir.canonicalize().expect("repo resolves"),
        "the reviewer must not start in the checkout"
    );
    assert!(dir.contains("osf-review-copy-"), "{dir}");
    for kept in [
        "./README.md",
        "./src/lib.rs",
        "./docs/guide.md",
        "./osf.toml",
    ] {
        assert!(files.contains(&kept), "{kept} is missing: {files:?}");
    }
    for left_out in [
        ".opencode",
        "opencode.json",
        "opencode.jsonc",
        ".omp",
        ".codex",
        ".claude",
        ".mcp.json",
        ".cursor",
        ".dsh",
        "AGENTS.md",
        "CLAUDE.md",
        "link-to-file",
        "link-to-dir",
    ] {
        assert!(
            !files.iter().any(|f| f.contains(left_out)),
            "{left_out} reached the copy: {files:?}"
        );
    }
    assert!(
        !Path::new(dir).exists(),
        "the copy is removed after the run"
    );
}

/// With a real opencode, a plugin file under `.opencode/plugin` in the change
/// does not run when osf starts the reviewer, and does run when opencode is
/// started in the checkout itself, which shows the test can tell. It is built
/// only with `--features real-agents`, where the opencode command is installed.
#[test]
#[cfg(all(unix, feature = "real-agents"))]
fn a_plugin_in_the_change_does_not_run_in_a_real_opencode_reviewer() {
    let marker_dir = TempDir::new("review-run-real-opencode-marker");
    let ran = marker_dir.join("plugin-ran");
    let plugin = format!(
        "import fs from \"node:fs\";\nfs.writeFileSync({:?}, \"ran\");\nexport const Extra = async () => ({{}});\n",
        ran.to_string_lossy()
    );
    let osf_toml = "[agents]\nreviewers = [\"opencode\"]\n\n[agents.models]\nopencode = \"openrouter/qwen/qwen3-coder-next\"\n\n[review]\ntimeout_seconds = 120\n";
    let repo = review_repo_with(
        "real-opencode",
        osf_toml,
        &[(".opencode/plugin/extra.js", &plugin)],
    );
    let key = common::fake_provider_key("sk-");

    let control_home = common::isolated_home("review-run-real-opencode-control");
    let control = std::process::Command::new("opencode")
        .args([
            "run",
            "--format",
            "json",
            "-m",
            "openrouter/qwen/qwen3-coder-next",
            "hello",
        ])
        .current_dir(&repo.dir)
        .env("HOME", &*control_home)
        .env("OPENROUTER_API_KEY", &key)
        .output()
        .expect("opencode starts");
    assert!(
        ran.exists(),
        "opencode in the checkout must load the plugin, or this test shows nothing: {}",
        String::from_utf8_lossy(&control.stdout)
    );
    std::fs::remove_file(&ran).expect("marker removes");

    let home = common::isolated_home("review-run-real-opencode");
    let output = common::run_osf_with_env(
        &repo.dir,
        &home,
        &[("OPENROUTER_API_KEY", &key)],
        &["review", "run", "--base", "origin/main"],
    );
    let journal = journal_lines_of(&home, "opencode");
    assert!(
        journal.contains("exited with code"),
        "the reviewer must have started: {journal}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !ran.exists(),
        "the plugin in the change ran in the reviewer"
    );
}

/// The base64 text of `text`, from the system tool.
#[cfg(unix)]
fn base64_of(text: &str) -> String {
    use std::io::Write as _;
    let mut child = std::process::Command::new("base64")
        .args(["-w", "0"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("base64 runs");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(text.as_bytes())
        .expect("writes");
    let out = child.wait_with_output().expect("base64 finishes");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The lower-case hex text of `text`.
#[cfg(unix)]
fn hex_of(text: &str) -> String {
    use std::fmt::Write as _;
    text.bytes().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// An answer file whose findings carry the placeholders [`Fake::Echoes`] fills in.
#[cfg(unix)]
fn echoing_answer(dir: &TempDir) -> String {
    let payload = serde_json::json!({
        "lens": "correctness",
        "scores": {"c1": 0.9, "c2": 0.9},
        "findings": [
            {
                "path": "src/lib.rs", "line": 2, "quote": "fn broken() {}",
                "severity": "minor", "action": "justify",
                "body": "plain @@KEY@@ base64 @@KEY_B64@@ hex @@KEY_HEX@@"
            },
            {
                "path": "notes/@@KEY@@.md", "line": 1, "quote": "see @@KEY_HEX@@ here",
                "severity": "minor", "action": "justify", "body": "again @@KEY_B64@@"
            }
        ]
    })
    .to_string();
    write_answer_file(dir, "echo.json", &payload)
}

/// A reviewer's own key, in plain, base64 and hex form, is removed from every
/// field of the answer before it is saved.
#[test]
#[cfg(unix)]
fn a_reviewers_own_key_is_removed_from_its_saved_answer() {
    let key = "reviewer-key-4f9c1a7be2d84a05";
    let payloads = TempDir::new("review-run-own-key-payload");
    let answer = echoing_answer(&payloads);
    let fakes = Fakes::new("", &[("codex", Fake::Echoes(&answer, "CODEX_API_KEY"))]);
    let repo = review_repo("own-key", &fakes.osf_toml);
    let saved = TempDir::new("review-run-own-key-saved");
    let file = saved.join("codex.json").to_string_lossy().into_owned();
    let home = common::isolated_home("review-run-own-key");
    let output = fakes.run_with_env(
        &repo.dir,
        &home,
        &[("CODEX_API_KEY", key)],
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
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = std::fs::read_to_string(&file).expect("the saved run reads");
    for form in [key.to_string(), base64_of(key), hex_of(key)] {
        assert!(!text.contains(&form), "the saved run holds {form}: {text}");
    }
    assert!(text.contains("a secret this job holds"), "{text}");
}

/// The same reviewer through the single run: the key reaches no journal line,
/// printed line or SARIF file either.
#[test]
#[cfg(unix)]
fn a_reviewers_own_key_reaches_no_journal_line_or_output() {
    let key = "reviewer-key-91d0b3c75e6f4a28";
    let payloads = TempDir::new("review-run-own-key-single-payload");
    let answer = echoing_answer(&payloads);
    let fakes = Fakes::new("", &[("codex", Fake::Echoes(&answer, "CODEX_API_KEY"))]);
    let repo = review_repo("own-key-single", &fakes.osf_toml);
    let home = common::isolated_home("review-run-own-key-single");
    let sarif_out = repo.dir.join("out.sarif");
    let output = fakes.run_with_env(
        &repo.dir,
        &home,
        &[("CODEX_API_KEY", key)],
        &[
            "review",
            "run",
            "--base",
            "origin/main",
            "--sarif-out",
            &sarif_out.to_string_lossy(),
        ],
    );
    for form in [key.to_string(), base64_of(key), hex_of(key)] {
        assert_no_leak(&form, &output, &home, Some(&sarif_out));
    }
}

/// One saved run: `reviewer`'s attempts at the `correctness` lens.
#[cfg(unix)]
fn saved_run_value(reviewer: &str, attempts: &[serde_json::Value]) -> serde_json::Value {
    serde_json::json!({
        "reviewer": reviewer,
        "lenses": [{"lens": "correctness", "context_error": null, "attempts": attempts}],
        "binding": "auto"
    })
}

/// One attempt, as saved: answered when it holds an answer.
#[cfg(unix)]
fn attempt_value(
    round: u64,
    critical: bool,
    answer: Option<&serde_json::Value>,
) -> serde_json::Value {
    serde_json::json!({
        "result": if answer.is_some() { "answered" } else { "could-not-run" },
        "reason": null, "notes": [], "round": round, "critical": critical, "answer": answer
    })
}

/// A well-formed answer for the lens these tests exercise, with `body` on its one finding.
#[cfg(unix)]
fn good_answer_value(body: &str) -> serde_json::Value {
    answer_value(
        "correctness",
        &serde_json::json!({"c1": 0.9, "c2": 0.9}),
        body,
    )
}

/// An answer for `lens` with `scores` and one finding that holds `body`.
#[cfg(unix)]
fn answer_value(lens: &str, scores: &serde_json::Value, body: &str) -> serde_json::Value {
    serde_json::json!({
        "lens": lens,
        "scores": scores,
        "findings": [{
            "path": "src/lib.rs", "line": 2, "quote": "fn broken() {}",
            "severity": "minor", "action": "justify", "body": body
        }]
    })
}

/// Three attempts that all hold `answer`: two independent rounds and the critical one.
#[cfg(unix)]
fn three_answers(answer: &serde_json::Value) -> Vec<serde_json::Value> {
    vec![
        attempt_value(1, false, Some(answer)),
        attempt_value(2, false, Some(answer)),
        attempt_value(3, true, Some(answer)),
    ]
}

/// What `review reduce` made of `files`, each given as the folder it sits in,
/// its name and its content: the output and the home that holds the journal.
#[cfg(unix)]
fn reduce_files(
    label: &str,
    reviewers: &[&str],
    files: &[(&str, &str, serde_json::Value)],
    env: &[(&str, &str)],
    extra_args: &[&str],
) -> (std::process::Output, common::IsolatedHome) {
    let rec_answers: Vec<(&str, Fake)> = reviewers
        .iter()
        .map(|name| (*name, Fake::Answers("/dev/null")))
        .collect();
    let fakes = Fakes::new("", &rec_answers);
    let repo = review_repo(label, &fakes.osf_toml);
    let saved = TempDir::new(&format!("review-run-reduce-{label}"));
    let mut paths = Vec::new();
    for (folder, name, value) in files {
        let dir = saved.join(folder);
        std::fs::create_dir_all(&dir).expect("folder creates");
        let path = dir.join(name);
        let mut value = value.clone();
        if value.get("binding") == Some(&serde_json::json!("auto")) {
            value
                .as_object_mut()
                .expect("a saved run is an object")
                .insert("binding".to_string(), binding_value(&repo.dir));
        }
        std::fs::write(&path, value.to_string()).expect("saved run writes");
        paths.push(path.to_string_lossy().into_owned());
    }
    let home = common::isolated_home(&format!("review-run-reduce-{label}"));
    let mut args = vec!["review", "reduce", "--base", "origin/main"];
    args.extend_from_slice(extra_args);
    args.extend(paths.iter().map(String::as_str));
    let output = fakes.run_with_env(&repo.dir, &home, env, &args);
    (output, home)
}

/// A saved run that is well formed passes, which is what makes the refusals below mean something.
#[test]
#[cfg(unix)]
fn a_well_formed_saved_run_is_accepted_at_reduce() {
    let attempts = three_answers(&good_answer_value("fine"));
    let (output, _home) = reduce_files(
        "accepts",
        &["codex"],
        &[("a", "codex.json", saved_run_value("codex", &attempts))],
        &[],
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stdout_of(&output));
}

/// Each saved answer that fails the reviewer job's own checks is refused at
/// reduce as could-not-run for that reviewer, and counts for nothing.
#[test]
#[cfg(unix)]
fn a_saved_answer_that_fails_the_schema_or_the_lens_is_refused_at_reduce() {
    let wrong_score = answer_value(
        "correctness",
        &serde_json::json!({"c1": 1000, "c2": 0.9}),
        "fine",
    );
    let wrong_lens = answer_value(
        "security",
        &serde_json::json!({"c1": 0.9, "c2": 0.9}),
        "fine",
    );
    let missing = answer_value("correctness", &serde_json::json!({"c1": 0.9}), "fine");
    for (label, answer, expected) in [
        ("score", wrong_score, "does not validate"),
        ("lens", wrong_lens, "another lens"),
        ("criterion", missing, "c2"),
    ] {
        let attempts = three_answers(&answer);
        let (output, home) = reduce_files(
            &format!("refuses-{label}"),
            &["codex"],
            &[("a", "codex.json", saved_run_value("codex", &attempts))],
            &[],
            &[],
        );
        assert_eq!(
            output.status.code(),
            Some(2),
            "{label}: {}",
            stdout_of(&output)
        );
        let journal = journal_lines_of(&home, "codex");
        assert!(journal.contains(expected), "{label}: {journal}");
        assert!(
            !journal.contains("\"result\":\"answered\""),
            "{label}: {journal}"
        );
    }
}

/// A file is read for the reviewer its name gives. A file named `codex.json`
/// that holds `claude`'s run counts for neither, and quorum is not met by it.
#[test]
#[cfg(unix)]
fn a_saved_run_under_another_reviewers_file_name_is_refused_at_reduce() {
    let attempts = three_answers(&good_answer_value("fine"));
    let (output, home) = reduce_files(
        "wrong-name",
        &["codex", "claude"],
        &[("a", "codex.json", saved_run_value("claude", &attempts))],
        &[],
        &[],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stdout_of(&output));
    let codex = journal_lines_of(&home, "codex");
    assert!(codex.contains("names another reviewer"), "{codex}");
    let claude = journal_lines_of(&home, "claude");
    assert!(claude.contains("left no answer"), "{claude}");
    assert!(
        !journal_text(&home).contains("\"result\":\"answered\""),
        "no answer counts"
    );
}

/// Two saved runs that carry one reviewer's name are both refused.
#[test]
#[cfg(unix)]
fn two_saved_runs_for_one_reviewer_are_both_refused_at_reduce() {
    let attempts = three_answers(&good_answer_value("fine"));
    let (output, home) = reduce_files(
        "duplicate",
        &["codex"],
        &[
            ("first", "codex.json", saved_run_value("codex", &attempts)),
            ("second", "codex.json", saved_run_value("codex", &attempts)),
        ],
        &[],
        &[],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stdout_of(&output));
    let journal = journal_lines_of(&home, "codex");
    assert!(journal.contains("more than one saved run"), "{journal}");
}

/// A file name that is not a plain reviewer name is never used.
#[test]
#[cfg(unix)]
fn a_saved_run_from_a_file_with_an_odd_name_is_never_used() {
    let attempts = three_answers(&good_answer_value("fine"));
    let (output, home) = reduce_files(
        "odd-name",
        &["codex"],
        &[("a", "co dex;1.json", saved_run_value("codex", &attempts))],
        &[],
        &[],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stdout_of(&output));
    let journal = journal_lines_of(&home, "codex");
    assert!(journal.contains("left no answer"), "{journal}");
}

/// The round and critical flags come from the order of the attempts, whatever the file says.
#[test]
#[cfg(unix)]
fn the_rounds_are_worked_out_from_the_order_of_the_attempts_at_reduce() {
    let answer = good_answer_value("fine");
    let attempts = vec![
        attempt_value(7, true, Some(&answer)),
        attempt_value(7, true, Some(&answer)),
        attempt_value(7, false, Some(&answer)),
    ];
    let (output, home) = reduce_files(
        "rounds",
        &["codex"],
        &[("a", "codex.json", saved_run_value("codex", &attempts))],
        &[],
        &[],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stdout_of(&output));
    let journal = journal_lines_of(&home, "codex");
    for round in 1..=3 {
        assert!(journal.contains(&format!("\"round\":{round}")), "{journal}");
    }
    assert!(!journal.contains("\"round\":7"), "{journal}");
}

/// A saved run with more attempts than a run makes is refused.
#[test]
#[cfg(unix)]
fn a_saved_run_with_too_many_attempts_is_refused_at_reduce() {
    let answer = good_answer_value("fine");
    let mut attempts = three_answers(&answer);
    attempts.push(attempt_value(4, false, Some(&answer)));
    let (output, home) = reduce_files(
        "too-many",
        &["codex"],
        &[("a", "codex.json", saved_run_value("codex", &attempts))],
        &[],
        &[],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stdout_of(&output));
    let journal = journal_lines_of(&home, "codex");
    assert!(
        journal.contains("more attempts than a run makes"),
        "{journal}"
    );
}

/// The code host's token is removed from a saved finding at reduce, even when
/// the saved text is no pattern a scan would catch.
#[test]
#[cfg(unix)]
fn the_code_hosts_token_is_removed_from_saved_findings_at_reduce() {
    let token = "host-token-5b1e7c93a02d4f68";
    let body = format!(
        "plain {token} base64 {} hex {}",
        base64_of(token),
        hex_of(token)
    );
    let attempts = three_answers(&good_answer_value(&body));
    let sarif_dir = TempDir::new("review-run-reduce-token-sarif");
    let sarif = sarif_dir.join("out.sarif");
    let (output, home) = reduce_files(
        "token",
        &["codex"],
        &[("a", "codex.json", saved_run_value("codex", &attempts))],
        &[("GH_TOKEN", token)],
        &["--sarif-out", &sarif.to_string_lossy()],
    );
    for form in [token.to_string(), base64_of(token), hex_of(token)] {
        assert_no_leak(&form, &output, &home, Some(&sarif));
    }
    let text = std::fs::read_to_string(&sarif).expect("sarif reads");
    assert!(text.contains("a secret this job holds"), "{text}");
}

/// Two saved runs for `repo`, under the default binding, and the control
/// reduce that shows they pass before a test changes anything.
fn saved_pair(label: &str) -> (Fakes, TempRepo, TempDir, String, String) {
    let fakes = Fakes::new(
        "",
        &[
            ("codex", Fake::Answers(&fixture("valid.json"))),
            ("claude", Fake::Answers(&fixture("claude-envelope.json"))),
        ],
    );
    let repo = review_repo(label, &fakes.osf_toml);
    let saved = TempDir::new(&format!("review-run-{label}"));
    let codex = save_run(&fakes, &repo, &saved, "codex");
    let claude = save_run(&fakes, &repo, &saved, "claude");
    let control = reduce_saved_files(
        &fakes,
        &repo,
        &format!("{label}-control"),
        &[&codex, &claude],
    );
    assert!(control.status.success(), "{}", stdout_of(&control));
    (fakes, repo, saved, codex, claude)
}

/// `review reduce` over `files` with `flags` as the binding, exactly as given.
fn reduce_with_flags(
    fakes: &Fakes,
    repo: &TempRepo,
    label: &str,
    flags: &[&str],
    files: &[&str],
) -> std::process::Output {
    let home = common::isolated_home(&format!("review-run-flags-{label}"));
    let mut args = vec!["review", "reduce", "--base", "origin/main"];
    args.extend(flags.iter().copied());
    args.extend(files.iter().copied());
    // A CI runner sets these two, and the flags are what this helper tests.
    let env = [("GITHUB_REPOSITORY", ""), ("GITHUB_RUN_ID", "")];
    fakes.run_unbound_with_env(&repo.dir, &home, &env, &args)
}

/// A saved run is bound to its pull request, its commits and its CI run:
/// `review reduce` refuses the run when any of them differs.
#[test]
fn a_saved_run_for_another_pull_request_or_ci_run_is_refused_at_reduce() {
    let (fakes, repo, _saved, codex, claude) = saved_pair("binding-mismatch");
    let head = commit_of(&repo.dir, "HEAD");
    let cases: [(&str, [&str; 8]); 3] = [
        (
            "another CI run",
            [
                "--repository",
                SAVED_REPOSITORY,
                "--pull-request-number",
                "7",
                "--head",
                &head,
                "--ci-run-id",
                "2002",
            ],
        ),
        (
            "another pull request",
            [
                "--repository",
                SAVED_REPOSITORY,
                "--pull-request-number",
                "8",
                "--head",
                &head,
                "--ci-run-id",
                SAVED_RUN_ID,
            ],
        ),
        (
            "another repository",
            [
                "--repository",
                "owner/other",
                "--pull-request-number",
                "7",
                "--head",
                &head,
                "--ci-run-id",
                SAVED_RUN_ID,
            ],
        ),
    ];
    for (label, flags) in cases {
        let out = reduce_with_flags(&fakes, &repo, label, &flags, &[&codex, &claude]);
        assert_eq!(out.status.code(), Some(2), "{label}: {}", stdout_of(&out));
        assert!(
            stdout_of(&out).contains("could-not-run"),
            "{label}: {}",
            stdout_of(&out)
        );
    }
}

/// A run saved at an older head is refused once the pull request has a new head.
#[test]
fn a_saved_run_from_an_earlier_head_is_refused_at_reduce() {
    let (fakes, repo, _saved, codex, claude) = saved_pair("binding-old-head");
    repo.write("README.md", "base\nplus a change to review\nand one more\n");
    repo.commit("a later change");
    let out = reduce_saved_files(&fakes, &repo, "binding-old-head-after", &[&codex, &claude]);
    assert_eq!(out.status.code(), Some(2), "{}", stdout_of(&out));
    assert!(
        stdout_of(&out).contains("could-not-run"),
        "{}",
        stdout_of(&out)
    );
}

/// A run with no binding at all, such as one saved before the binding existed, is refused.
#[test]
fn a_saved_run_with_no_binding_is_refused_at_reduce() {
    let (fakes, repo, _saved, codex, claude) = saved_pair("binding-absent");
    for file in [&codex, &claude] {
        let mut run: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(file).expect("reads")).expect("json");
        run.as_object_mut().expect("object").remove("binding");
        std::fs::write(file, run.to_string()).expect("writes");
    }
    let out = reduce_saved_files(&fakes, &repo, "binding-absent-after", &[&codex, &claude]);
    assert_eq!(out.status.code(), Some(2), "{}", stdout_of(&out));
}

/// The reduce step and a saved run each need the binding flags, and a head
/// that is not the checked-out commit is an error.
#[test]
fn the_binding_flags_are_required_and_the_head_must_be_the_checkout() {
    let (fakes, repo, _saved, codex, claude) = saved_pair("binding-flags");
    let missing = reduce_with_flags(&fakes, &repo, "missing", &[], &[&codex, &claude]);
    assert_eq!(missing.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&missing.stderr).into_owned();
    assert!(stderr.contains("a repository is required"), "{stderr}");
    let wrong_head = "f".repeat(40);
    let out = reduce_with_flags(
        &fakes,
        &repo,
        "wrong-head",
        &[
            "--repository",
            SAVED_REPOSITORY,
            "--pull-request-number",
            "7",
            "--head",
            &wrong_head,
            "--ci-run-id",
            SAVED_RUN_ID,
        ],
        &[&codex, &claude],
    );
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(stderr.contains("is not the commit checked out"), "{stderr}");
}

/// A saved file for a reviewer outside the roster, and a lens entry the change
/// did not select, are named in the run notes instead of dropped without a word.
#[test]
fn entries_the_reduce_step_drops_are_named_in_the_run_notes() {
    let (fakes, repo, _saved, codex, claude) = saved_pair("binding-extras");
    let saved = TempDir::new("review-run-binding-extras");
    let extra = saved.join("mallory.json").to_string_lossy().into_owned();
    let mut run: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&codex).expect("reads")).expect("json");
    run.as_object_mut()
        .expect("object")
        .insert("reviewer".to_string(), serde_json::json!("mallory"));
    std::fs::write(&extra, run.to_string()).expect("writes");
    let mut with_lens: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&claude).expect("reads")).expect("json");
    let lenses = with_lens
        .get_mut("lenses")
        .and_then(serde_json::Value::as_array_mut)
        .expect("lenses");
    let mut other = lenses.first().cloned().expect("a lens entry");
    other
        .as_object_mut()
        .expect("object")
        .insert("lens".to_string(), serde_json::json!("security"));
    lenses.push(other);
    std::fs::write(&claude, with_lens.to_string()).expect("writes");
    let out = reduce_saved_files(
        &fakes,
        &repo,
        "binding-extras-run",
        &[&codex, &claude, &extra],
    );
    assert!(out.status.success(), "{}", stdout_of(&out));
    let stdout = stdout_of(&out);
    assert!(
        stdout.contains(
            "note: ignored the saved run for \"mallory\": that reviewer is not in the roster"
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains("note: ignored the lens \"security\" in the saved run for \"claude\""),
        "{stdout}"
    );
}

/// A threshold that is NaN stops the review before any reviewer starts, so a
/// bad setting can never let a review pass.
#[test]
fn a_threshold_that_is_not_a_number_stops_the_review_before_any_reviewer_starts() {
    let fakes = Fakes::new(
        "[review]\nthreshold = nan\n\n",
        &[("codex", Fake::Answers(&fixture("valid.json")))],
    );
    let repo = review_repo("nan-threshold", &fakes.osf_toml);
    let home = common::isolated_home("review-run-nan-threshold");
    let output = fakes.run(
        &repo.dir,
        &home,
        &["review", "run", "--base", "origin/main"],
    );
    assert_eq!(output.status.code(), Some(2), "{}", stdout_of(&output));
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        stderr.contains("threshold must be a number from 0 to 1"),
        "{stderr}"
    );
}
