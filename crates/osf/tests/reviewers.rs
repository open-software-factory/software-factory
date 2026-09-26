//! `reviewers::run_one`, driven through a fake harness script so no test
//! ever calls a real model. `OSF_FAKE_ANSWER` and friends are process-global
//! state, so every test that sets one goes through `serial`, the same
//! pattern `tests/moon_adapter.rs` uses for `OSF_MOON`.

mod common;

use common::TempDir;
use osf::lenses::{Criterion, Lens, Runs, SeverityGuide, Trigger};
use osf::reviewers::{roster, run_one, Outcome, Reviewer, SchemaArg};
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

/// The fake harness, invoked the way a real coding-agent CLI would be: one
/// program, no shell. `fake-harness.sh` needs its executable bit, which git
/// preserves once set; `fake-harness.ps1` runs through `powershell -File`.
fn fake_harness_command() -> Vec<String> {
    if cfg!(windows) {
        vec![
            "powershell".to_string(),
            "-NoProfile".to_string(),
            "-ExecutionPolicy".to_string(),
            "Bypass".to_string(),
            "-File".to_string(),
            fixture("fake-harness.ps1"),
        ]
    } else {
        vec![fixture("fake-harness.sh")]
    }
}

fn fake_reviewer() -> Reviewer {
    Reviewer {
        name: "fake".to_string(),
        harness: "fake".to_string(),
        family: "fake-family".to_string(),
        command: fake_harness_command(),
        schema_flag: None,
        schema_as: SchemaArg::default(),
        enabled: true,
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
    serial(&[("OSF_FAKE_ANSWER", &answer_path)], || {
        let workdir = TempDir::new("osf-reviewers-valid");
        let outcome = run_one(
            &fake_reviewer(),
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
    });
}

#[test]
fn an_invalid_answer_is_retried_once_then_reported_invalid() {
    let workdir = TempDir::new("osf-reviewers-invalid");
    let log_path = workdir.join("harness.log");
    let log_str = log_path.to_string_lossy().into_owned();
    let answer_path = fixture("prose-only.txt");
    serial(
        &[
            ("OSF_FAKE_ANSWER", &answer_path),
            ("OSF_FAKE_HARNESS_LOG", &log_str),
        ],
        || {
            let outcome = run_one(
                &fake_reviewer(),
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
        },
    );
    let log = std::fs::read_to_string(&log_path).expect("the fake harness's log writes");
    assert_eq!(
        log.lines().count(),
        2,
        "expected the fake harness to run twice (the first try and one retry): {log}"
    );
}

#[test]
fn a_harness_that_cannot_start_is_could_not_run() {
    let mut reviewer = fake_reviewer();
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
    serial(
        &[
            ("OSF_FAKE_ANSWER", &answer_path),
            ("OSF_FAKE_SLEEP_SECS", "6"),
            ("OSF_FAKE_MARKER", &marker_str),
        ],
        || {
            let outcome = run_one(
                &fake_reviewer(),
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
        },
    );

    // The sleep would finish around the 6s mark from spawn; wait well past
    // that before checking the marker never showed up.
    std::thread::sleep(Duration::from_secs(6));
    assert!(
        !marker.exists(),
        "the sleeping harness kept running after its timeout and wrote its marker file"
    );
}

#[test]
fn the_shipped_roster_has_no_gemini_and_every_entry_disabled() {
    let r = roster(&std::env::temp_dir()).expect("roster"); // osf: temp-dir allowed, no osf.toml is read from it here
    assert!(r.iter().all(|x| !x.harness.contains("gemini")));
    assert!(r.iter().all(|x| !x.enabled));
}
