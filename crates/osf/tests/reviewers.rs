//! `reviewers::run_one`, driven through a fake harness script so no test
//! ever calls a real model. `OSF_FAKE_ANSWER` and friends are process-global
//! state, so every test that sets one goes through `serial`, the same
//! pattern `tests/moon_adapter.rs` uses for `OSF_MOON`.

mod common;

use common::TempDir;
use osf::lenses::{Criterion, Lens, Runs, SeverityGuide, Trigger};
use osf::reviewers::{roster, run_one, Outcome, ReadOnly, Reviewer, SchemaArg, Switches};
use std::sync::Mutex;
use std::time::Duration;

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Runs `f` while holding the process-wide environment lock, setting `vars`
/// first and clearing them again afterwards. Every test here goes through
/// this, because the fake harness is driven entirely by `OSF_FAKE_*`
/// environment variables the spawned process inherits.
fn serial<T>(vars: &[(&str, &str)], f: impl FnOnce() -> T) -> T {
    let guard = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for (k, v) in vars {
        // SAFETY: serialised by ENV_LOCK; no other thread touches the environment here.
        unsafe { std::env::set_var(k, v) };
    }
    let result = f();
    for (k, _) in vars {
        // SAFETY: serialised by ENV_LOCK; no other thread touches the environment here.
        unsafe { std::env::remove_var(k) };
    }
    drop(guard);
    result
}

fn fixture(name: &str) -> String {
    format!(
        "{}/tests/fixtures/review/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// `s`, single-quoted for a POSIX shell: any single quote inside is closed,
/// escaped, and reopened.
#[cfg(unix)]
fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// The fake harness, invoked the way a real coding-agent CLI would be, with
/// `vars` set for the harness's own run only, never on this test process:
/// `run_one` now starts a reviewer with an allow-listed environment, so a
/// control variable set on the test process (the old way every test here
/// worked) would never reach the child. `fake-harness.sh` needs its
/// executable bit, which git preserves once set.
#[cfg(unix)]
fn fake_harness_command(vars: &[(&str, &str)]) -> Vec<String> {
    use std::fmt::Write as _;
    let mut assignments = String::new();
    for (k, v) in vars {
        let _ = write!(assignments, "{k}={} ", shell_single_quote(v));
    }
    vec![
        "sh".to_string(),
        "-c".to_string(),
        format!(
            "{assignments}exec {}",
            shell_single_quote(&fixture("fake-harness.sh"))
        ),
    ]
}

#[cfg(windows)]
fn fake_harness_command(vars: &[(&str, &str)]) -> Vec<String> {
    use std::fmt::Write as _;
    let mut prefix = String::new();
    for (k, v) in vars {
        let _ = write!(prefix, "$env:{k}=\"{}\"; ", v.replace('"', "`\""));
    }
    vec![
        "powershell".to_string(),
        "-NoProfile".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-Command".to_string(),
        format!("{prefix}& '{}'", fixture("fake-harness.ps1")),
    ]
}

/// A reviewer backed by the fake harness, started with `vars` set for its
/// own run (see [`fake_harness_command`]) and no declared credential: tests
/// that need one set `credential_env` on the result themselves.
fn fake_reviewer(vars: &[(&str, &str)]) -> Reviewer {
    Reviewer {
        name: "fake".to_string(),
        family: "fake-family".to_string(),
        family_error: None,
        command: fake_harness_command(vars),
        read_only: Some(ReadOnly {
            args: &[],
            env: &[],
        }),
        clean_copy: Switches {
            args: &[],
            env: &[],
        },
        schema_flag: None,
        schema_as: SchemaArg::default(),
        answer_pointer: String::new(),
        model: None,
        model_flag: None,
        credential_env: Vec::new(),
        login_paths: Vec::new(),
        sandbox_check: Vec::new(),
        review_dir: None,
    }
}

fn test_lens() -> Lens {
    Lens {
        name: "correctness".to_string(),
        summary: "does the change do what it claims to do".to_string(),
        criteria: vec![
            Criterion {
                id: "c1".to_string(),
                question: "does the change do what it claims".to_string(),
            },
            Criterion {
                id: "c2".to_string(),
                question: "does it handle its error paths".to_string(),
            },
        ],
        severity_guide: SeverityGuide {
            blocker: "the change loses data or breaks the build".to_string(),
            major: "a wrong behaviour a user can hit".to_string(),
            minor: "a small correctness nit".to_string(),
        },
        weight: 1.0,
        runs: Runs::Always,
        trigger: Trigger::default(),
        context: Vec::new(),
    }
}

#[test]
fn a_valid_answer_from_the_fake_harness_is_answered() {
    let answer_path = fixture("valid.json");
    let workdir = TempDir::new("osf-reviewers-valid");
    let outcome = run_one(
        &fake_reviewer(&[("OSF_FAKE_ANSWER", &answer_path)]),
        "review this change",
        &test_lens(),
        &workdir,
        Duration::from_secs(10),
    );
    match outcome {
        Outcome::Answered(answer) => assert_eq!(answer.lens, "correctness"),
        Outcome::Invalid(e) => panic!("expected Answered, got Invalid({e})"),
        Outcome::CouldNotRun(e) => panic!("expected Answered, got CouldNotRun({e})"),
    }
}

/// A reviewer's own harness never sees the tokens `osf` itself uses to post
/// a review, nor any other reviewer's own provider credential: `run_one`
/// starts it with an allow-listed environment holding only the variables
/// every child needs to run and this reviewer's own declared
/// `credential_env`, whatever else is set on `osf`'s own process.
#[test]
fn a_reviewer_child_sees_only_its_own_declared_credential() {
    let workdir = TempDir::new("osf-reviewers-no-token-leak");
    let capture_path = workdir.join("env-capture.txt");
    let capture_str = capture_path.to_string_lossy().into_owned();
    let answer_path = fixture("valid.json");
    let mut reviewer = fake_reviewer(&[
        ("OSF_FAKE_ANSWER", &answer_path),
        ("OSF_FAKE_ENV_CAPTURE", &capture_str),
    ]);
    reviewer.credential_env = vec!["ANTHROPIC_API_KEY".to_string()];
    serial(
        &[
            ("GH_TOKEN", "verifier-token-must-never-reach-a-reviewer"),
            ("GITHUB_TOKEN", "default-token-must-never-reach-a-reviewer"),
            (
                "GH_ENTERPRISE_TOKEN",
                "enterprise-token-must-never-reach-a-reviewer",
            ),
            ("ANTHROPIC_API_KEY", "this-reviewers-own-credential"),
            ("OPENAI_API_KEY", "a-different-providers-credential"),
            ("DEEPSEEK_API_KEY", "a-different-providers-credential"),
            (
                "CLAUDE_CODE_OAUTH_TOKEN",
                "a-different-reviewers-own-subscription-token",
            ),
            ("OPENROUTER_API_KEY", "a-different-providers-credential"),
        ],
        || {
            let outcome = run_one(
                &reviewer,
                "review this change",
                &test_lens(),
                &workdir,
                Duration::from_secs(10),
            );
            match outcome {
                Outcome::Answered(_) => {}
                Outcome::Invalid(e) => panic!("expected Answered, got Invalid({e})"),
                Outcome::CouldNotRun(e) => panic!("expected Answered, got CouldNotRun({e})"),
            }
        },
    );
    let captured =
        std::fs::read_to_string(&capture_path).expect("the fake harness's env capture writes");
    assert_eq!(
        captured,
        "GH_TOKEN=\n\
         GITHUB_TOKEN=\n\
         GH_ENTERPRISE_TOKEN=\n\
         OPENAI_API_KEY=\n\
         ANTHROPIC_API_KEY=this-reviewers-own-credential\n\
         DEEPSEEK_API_KEY=\n\
         CLAUDE_CODE_OAUTH_TOKEN=\n\
         OPENROUTER_API_KEY=\n",
        "{captured}"
    );
}

#[test]
fn an_invalid_answer_is_retried_once_then_reported_invalid() {
    let workdir = TempDir::new("osf-reviewers-invalid");
    let log_path = workdir.join("harness.log");
    let log_str = log_path.to_string_lossy().into_owned();
    let answer_path = fixture("prose-only.txt");
    let outcome = run_one(
        &fake_reviewer(&[
            ("OSF_FAKE_ANSWER", &answer_path),
            ("OSF_FAKE_HARNESS_LOG", &log_str),
        ]),
        "review this change",
        &test_lens(),
        &workdir,
        Duration::from_secs(10),
    );
    match outcome {
        Outcome::Invalid(_) => {}
        Outcome::Answered(_) => panic!("expected Invalid, got Answered"),
        Outcome::CouldNotRun(e) => panic!("expected Invalid, got CouldNotRun({e})"),
    }
    let log = std::fs::read_to_string(&log_path).expect("the fake harness's log writes");
    assert_eq!(
        log.lines().count(),
        2,
        "expected the fake harness to run twice (the first try and one retry): {log}"
    );
}

#[test]
fn a_harness_that_cannot_start_is_could_not_run() {
    let mut reviewer = fake_reviewer(&[]);
    reviewer.command = vec!["/nonexistent/osf-fake-harness-that-does-not-exist".to_string()];
    let workdir = TempDir::new("osf-reviewers-missing");
    let outcome = run_one(
        &reviewer,
        "review this change",
        &test_lens(),
        &workdir,
        Duration::from_secs(5),
    );
    assert!(matches!(outcome, Outcome::CouldNotRun(_)));
}

/// A real reviewer tool reads its own API key from a variable such as
/// `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, or `DEEPSEEK_API_KEY`, and exits
/// non-zero when that variable is missing, rather than hanging on an
/// interactive login prompt. `OSF_FAKE_REQUIRE_ENV` names a variable the
/// fake harness checks the same way, standing in for that missing-key
/// case: `run_one` must report it as could-not-run for this reviewer, and
/// never as answered or invalid, so a run over several reviewers moves on
/// to the next one instead of stalling.
#[test]
fn a_reviewer_whose_required_key_is_missing_is_could_not_run() {
    let answer_path = fixture("valid.json");
    let workdir = TempDir::new("osf-reviewers-missing-key");
    let outcome = run_one(
        &fake_reviewer(&[
            ("OSF_FAKE_ANSWER", &answer_path),
            ("OSF_FAKE_REQUIRE_ENV", "OSF_FAKE_REVIEWER_API_KEY"),
        ]),
        "review this change",
        &test_lens(),
        &workdir,
        Duration::from_secs(10),
    );
    match outcome {
        Outcome::CouldNotRun(_) => {}
        Outcome::Answered(_) => panic!("expected CouldNotRun, got Answered"),
        Outcome::Invalid(e) => panic!("expected CouldNotRun, got Invalid({e})"),
    }
}

/// A reviewer whose harness hangs past its timeout is killed through the
/// shared tree-kill helper `moon.rs` also uses, and counts as could-not-run
/// rather than hanging the review forever. The fake harness sleeps well
/// past the timeout, then writes a marker file; if the marker still
/// appears, the harness (or a child it spawned) kept running after
/// `run_one` returned.
#[test]
fn a_reviewer_that_hangs_past_its_timeout_is_killed_and_reported_could_not_run() {
    let workdir = TempDir::new("osf-reviewers-timeout");
    let marker = workdir.join("marker.txt");
    let marker_str = marker.to_string_lossy().into_owned();
    let answer_path = fixture("valid.json");
    let outcome = run_one(
        &fake_reviewer(&[
            ("OSF_FAKE_ANSWER", &answer_path),
            ("OSF_FAKE_SLEEP_SECS", "6"),
            ("OSF_FAKE_MARKER", &marker_str),
        ]),
        "review this change",
        &test_lens(),
        &workdir,
        Duration::from_secs(2),
    );
    match outcome {
        Outcome::CouldNotRun(reason) => {
            assert!(reason.contains("timed out"), "{reason}");
        }
        Outcome::Answered(_) => panic!("expected CouldNotRun, got Answered"),
        Outcome::Invalid(e) => panic!("expected CouldNotRun, got Invalid({e})"),
    }

    // The sleep would finish around the 6s mark from spawn; wait well past
    // that before checking the marker never showed up.
    std::thread::sleep(Duration::from_secs(6));
    assert!(
        !marker.exists(),
        "the sleeping harness kept running after its timeout and wrote its marker file"
    );
}

/// A 1 MB prompt is well over an OS pipe's own buffer (64 KiB on Linux), so
/// a harness that never reads its stdin would block a synchronous write
/// forever. `run_one` must still be governed by `timeout` alone: the write
/// runs on its own thread, and the harness is killed on schedule.
#[test]
fn a_large_prompt_to_a_harness_that_ignores_stdin_is_still_killed_at_its_timeout() {
    let workdir = TempDir::new("osf-reviewers-large-stdin");
    let answer_path = fixture("valid.json");
    let big_prompt = "x".repeat(1024 * 1024);
    let started = std::time::Instant::now();
    let outcome = run_one(
        &fake_reviewer(&[
            ("OSF_FAKE_ANSWER", &answer_path),
            ("OSF_FAKE_SLEEP_SECS", "6"),
        ]),
        &big_prompt,
        &test_lens(),
        &workdir,
        Duration::from_secs(2),
    );
    match outcome {
        Outcome::CouldNotRun(reason) => {
            assert!(reason.contains("timed out"), "{reason}");
        }
        Outcome::Answered(_) => panic!("expected CouldNotRun, got Answered"),
        Outcome::Invalid(e) => panic!("expected CouldNotRun, got Invalid({e})"),
    }
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "run_one took {:?}, the 2s timeout should have governed it, not the 1 MB stdin write",
        started.elapsed()
    );
}

/// codex's real shape with `--output-schema` and no `--json`: the final,
/// schema-matching message goes straight to stdout, no envelope. This is
/// `fake_reviewer`'s default (`answer_pointer` empty), the same shape
/// `a_valid_answer_from_the_fake_harness_is_answered` already covers; named
/// separately so it reads as codex's own case.
#[test]
fn a_codex_style_plain_answer_needs_no_envelope_pointer() {
    let answer_path = fixture("valid.json");
    let mut reviewer = fake_reviewer(&[("OSF_FAKE_ANSWER", &answer_path)]);
    reviewer.answer_pointer = String::new();
    let workdir = TempDir::new("osf-reviewers-codex-shape");
    let outcome = run_one(
        &reviewer,
        "review this change",
        &test_lens(),
        &workdir,
        Duration::from_secs(10),
    );
    match outcome {
        Outcome::Answered(answer) => assert_eq!(answer.lens, "correctness"),
        Outcome::Invalid(e) => panic!("expected Answered, got Invalid({e})"),
        Outcome::CouldNotRun(e) => panic!("expected Answered, got CouldNotRun({e})"),
    }
}

/// A 1 MB prompt is well over an OS pipe's own buffer, so the write blocks
/// until something drains it or closes the pipe. This harness never reads
/// its own stdin and (with no sleep configured) runs to completion at
/// once, closing its end of the pipe while the write is still blocked: the
/// write then fails with a broken-pipe error every time. The pipe's own
/// buffering causes that failure, and scheduling plays no part. That failure
/// alone used to be treated as could-not-run even though the harness
/// answered correctly; under real load, the same failure could also happen
/// with a small prompt, whenever the harness happened to exit before the
/// write started.
#[test]
fn a_harness_that_never_reads_a_large_prompt_and_exits_at_once_is_still_answered() {
    let answer_path = fixture("valid.json");
    let big_prompt = "x".repeat(1024 * 1024);
    let workdir = TempDir::new("osf-reviewers-broken-pipe-answered");
    let outcome = run_one(
        &fake_reviewer(&[("OSF_FAKE_ANSWER", &answer_path)]),
        &big_prompt,
        &test_lens(),
        &workdir,
        Duration::from_secs(10),
    );
    match outcome {
        Outcome::Answered(answer) => assert_eq!(answer.lens, "correctness"),
        Outcome::Invalid(e) => panic!("expected Answered, got Invalid({e})"),
        Outcome::CouldNotRun(e) => panic!("expected Answered, got CouldNotRun({e})"),
    }
}

/// Claude Code's real shape with `--output-format json` and `--json-schema`:
/// the schema-matching answer is nested under the envelope's own
/// `structured_output` field, alongside session metadata and cost.
#[test]
fn a_claude_code_style_envelope_extracts_structured_output() {
    let answer_path = fixture("claude-envelope.json");
    let mut reviewer = fake_reviewer(&[("OSF_FAKE_ANSWER", &answer_path)]);
    reviewer.answer_pointer = "/structured_output".to_string();
    let workdir = TempDir::new("osf-reviewers-claude-envelope");
    let outcome = run_one(
        &reviewer,
        "review this change",
        &test_lens(),
        &workdir,
        Duration::from_secs(10),
    );
    match outcome {
        Outcome::Answered(answer) => assert_eq!(answer.lens, "correctness"),
        Outcome::Invalid(e) => panic!("expected Answered, got Invalid({e})"),
        Outcome::CouldNotRun(e) => panic!("expected Answered, got CouldNotRun({e})"),
    }
}

/// This repository's own `osf.toml` picks its reviewers from the agent
/// list, in this order, and pins the models it names.
#[test]
fn this_repositorys_osf_toml_selects_its_reviewers_from_the_agent_list_in_order() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let loaded = roster(&repo_root).expect("this repository's osf.toml roster loads");
    let names: Vec<&str> = loaded.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["codex", "claude", "opencode"], "{loaded:?}");
    let model = |name: &str| {
        loaded
            .iter()
            .find(|r| r.name == name)
            .and_then(|r| r.model.as_deref())
    };
    assert_eq!(model("claude"), Some("claude-sonnet-5"));
    assert_eq!(model("opencode"), Some("openrouter/qwen/qwen3-coder-next"));
    assert_eq!(model("codex"), None);
}

/// A Claude-built range's family (`anthropic`, from its `Code-Generator:`
/// trailer) leaves this repository's own `claude` reviewer out, and no
/// reviewer of another family.
#[test]
fn a_claude_built_range_excludes_this_repositorys_claude_reviewer_and_no_other() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let loaded = roster(&repo_root).expect("this repository's osf.toml roster loads");
    let review_config =
        osf::config::review_config(&repo_root).expect("this repository's review config loads");
    let family = osf::builder::family_of("Claude Sonnet 5", &review_config.builder_family_aliases);
    let builders = std::collections::BTreeSet::from([family]);
    let excluded: Vec<&str> = loaded
        .iter()
        .filter(|r| r.is_excluded_by(&builders))
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(excluded, vec!["claude"], "{loaded:?}");
}

/// A fake sandbox check that records each run in `marker`, then succeeds when
/// `works`, or prints a reason to standard error and fails.
#[cfg(unix)]
fn fake_sandbox_check(works: bool, marker: &std::path::Path) -> Vec<String> {
    let marker = shell_single_quote(&marker.to_string_lossy());
    let ending = if works {
        "exit 0"
    } else {
        "echo 'bwrap: No permissions to create new namespace' 1>&2; exit 1"
    };
    vec![
        "sh".to_string(),
        "-c".to_string(),
        format!("echo ran >> {marker}; {ending}"),
    ]
}

#[cfg(windows)]
fn fake_sandbox_check(works: bool, marker: &std::path::Path) -> Vec<String> {
    let ending = if works {
        "exit 0"
    } else {
        "[Console]::Error.WriteLine('bwrap: No permissions to create new namespace'); exit 1"
    };
    vec![
        "powershell".to_string(),
        "-NoProfile".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-Command".to_string(),
        format!(
            "Add-Content -Path '{}' -Value ran; {ending}",
            marker.display()
        ),
    ]
}

/// A reviewer whose sandbox starts is checked once, then runs and answers.
#[test]
fn a_sandbox_that_starts_lets_the_reviewer_answer() {
    let workdir = TempDir::new("osf-reviewers-sandbox-works");
    let marker = workdir.join("sandbox-check.log");
    let log = workdir.join("harness.log");
    let log_str = log.to_string_lossy().into_owned();
    let answer_path = fixture("valid.json");
    let mut reviewer = fake_reviewer(&[
        ("OSF_FAKE_ANSWER", &answer_path),
        ("OSF_FAKE_HARNESS_LOG", &log_str),
    ]);
    reviewer.sandbox_check = fake_sandbox_check(true, &marker);
    let outcome = run_one(
        &reviewer,
        "review this change",
        &test_lens(),
        &workdir,
        Duration::from_secs(10),
    );
    match outcome {
        Outcome::Answered(_) => {}
        Outcome::Invalid(e) => panic!("expected Answered, got Invalid({e})"),
        Outcome::CouldNotRun(e) => panic!("expected Answered, got CouldNotRun({e})"),
    }
    assert!(marker.exists(), "the sandbox check ran before the reviewer");
    assert!(log.exists(), "the reviewer ran after the check passed");
}

/// A reviewer whose sandbox cannot start never starts its agent: it is
/// could-not-run, with the sandbox's own reason.
#[test]
fn a_sandbox_that_cannot_start_stops_the_reviewer_before_its_agent_runs() {
    let workdir = TempDir::new("osf-reviewers-sandbox-fails");
    let marker = workdir.join("sandbox-check.log");
    let log = workdir.join("harness.log");
    let log_str = log.to_string_lossy().into_owned();
    let answer_path = fixture("valid.json");
    let mut reviewer = fake_reviewer(&[
        ("OSF_FAKE_ANSWER", &answer_path),
        ("OSF_FAKE_HARNESS_LOG", &log_str),
    ]);
    reviewer.sandbox_check = fake_sandbox_check(false, &marker);
    let outcome = run_one(
        &reviewer,
        "review this change",
        &test_lens(),
        &workdir,
        Duration::from_secs(10),
    );
    match outcome {
        Outcome::CouldNotRun(reason) => {
            assert!(reason.contains("read-only sandbox"), "{reason}");
            assert!(
                reason.contains("No permissions to create new namespace"),
                "{reason}"
            );
        }
        Outcome::Answered(_) => panic!("expected CouldNotRun, got Answered"),
        Outcome::Invalid(e) => panic!("expected CouldNotRun, got Invalid({e})"),
    }
    assert!(marker.exists(), "the sandbox check ran");
    assert!(!log.exists(), "the agent never started");
}

/// A sandbox check that never finishes is stopped, and the reviewer is could-not-run.
#[cfg(unix)]
#[test]
fn a_sandbox_check_that_never_finishes_is_could_not_run() {
    let workdir = TempDir::new("osf-reviewers-sandbox-hangs");
    let mut reviewer = fake_reviewer(&[]);
    reviewer.sandbox_check = vec!["sh".to_string(), "-c".to_string(), "sleep 30".to_string()];
    let started = std::time::Instant::now();
    let outcome = run_one(
        &reviewer,
        "review this change",
        &test_lens(),
        &workdir,
        Duration::from_secs(1),
    );
    assert!(started.elapsed() < Duration::from_secs(20));
    match outcome {
        Outcome::CouldNotRun(reason) => assert!(reason.contains("timed out"), "{reason}"),
        Outcome::Answered(_) => panic!("expected CouldNotRun, got Answered"),
        Outcome::Invalid(e) => panic!("expected CouldNotRun, got Invalid({e})"),
    }
}

/// The codex reviewer carries the check that the agent list gives it.
#[test]
fn the_codex_reviewer_carries_a_sandbox_check_and_the_others_carry_none() {
    let root = TempDir::new("osf-reviewers-roster-check");
    std::fs::write(
        root.join("osf.toml"),
        "[agents]\nreviewers = [\"codex\", \"claude\"]\n",
    )
    .expect("osf.toml writes");
    let reviewers = roster(&root).expect("roster loads");
    let check = |name: &str| {
        reviewers
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.sandbox_check.clone())
            .expect("reviewer is in the roster")
    };
    assert_eq!(check("codex"), ["codex", "sandbox", "--", "true"]);
    assert!(check("claude").is_empty());
}

/// A reviewer's temporary folder is inside its own home, so that an agent that
/// refuses to set up its sandbox helper when its home sits under the
/// temporary folder it sees still starts.
#[cfg(unix)]
#[test]
fn a_reviewers_temporary_folder_is_inside_its_own_home() {
    let workdir = TempDir::new("osf-reviewers-own-temp");
    let capture = workdir.join("home-and-temp.txt");
    let capture_str = shell_single_quote(&capture.to_string_lossy());
    let answer = shell_single_quote(&fixture("valid.json"));
    let mut reviewer = fake_reviewer(&[]);
    reviewer.command = vec![
        "sh".to_string(),
        "-c".to_string(),
        format!("printf '%s\\n%s\\n' \"$HOME\" \"$TMPDIR\" > {capture_str}; cat {answer}"),
    ];
    let outcome = run_one(
        &reviewer,
        "review this change",
        &test_lens(),
        &workdir,
        Duration::from_secs(10),
    );
    assert!(matches!(outcome, Outcome::Answered(_)), "{outcome:?}");
    let text = std::fs::read_to_string(&capture).expect("capture reads");
    let mut lines = text.lines();
    let home = lines.next().expect("a home line");
    let temp = lines.next().expect("a temp line");
    assert_eq!(temp, format!("{home}/tmp"), "{text}");
    let parent_temp = std::env::temp_dir(); // osf: temp-dir allowed, only compared with the child's own folder
    assert_ne!(
        std::path::Path::new(temp),
        parent_temp,
        "the child does not share its parent's temporary folder"
    );
}
