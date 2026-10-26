//! The run start and complete commands, through the built osf binary.

mod common;

use common::{isolated_home, run_osf, TempRepo};
use std::process::Output;

const ITEM: &str = "github:open-software-factory/software-factory#1";

fn start(repo: &TempRepo, home: &std::path::Path, run: &str, format: &str) -> Output {
    run_osf(
        &repo.dir,
        home,
        &[
            "run",
            "start",
            "--run",
            run,
            "--work-item",
            ITEM,
            "--format",
            format,
        ],
    )
}

fn complete(repo: &TempRepo, home: &std::path::Path, run: &str, format: &str) -> Output {
    run_osf(
        &repo.dir,
        home,
        &[
            "run",
            "complete",
            "--run",
            run,
            "--outcome",
            "completed",
            "--summary",
            "done",
            "--format",
            format,
        ],
    )
}

#[test]
fn start_and_complete_write_one_complete_journal() {
    let repo = TempRepo::new("run-command");
    let home = isolated_home("run-command");
    let start = start(&repo, &home, "cli-run", "json");
    assert!(
        start.status.success(),
        "start failed: {}",
        String::from_utf8_lossy(&start.stderr)
    );
    let complete = complete(&repo, &home, "cli-run", "json");
    assert!(
        complete.status.success(),
        "complete failed: {}",
        String::from_utf8_lossy(&complete.stderr)
    );
    let state = home.join(".osf").join("state");
    let run = osf::journal::read_run(&state, "cli-run").expect("the journal reads");
    assert_eq!(run.events.len(), 2);
    assert!(run.complete);
    assert_eq!(
        osf::journal::runs_for_work_item(&state, ITEM).expect("the index reads"),
        ["cli-run"]
    );
}

#[test]
fn a_malformed_work_item_is_a_usage_error() {
    let repo = TempRepo::new("run-command-bad-item");
    let home = isolated_home("run-command-bad-item");
    let output = run_osf(
        &repo.dir,
        &home,
        &[
            "run",
            "start",
            "--run",
            "cli-run",
            "--work-item",
            "not-a-work-item",
        ],
    );
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("provider:owner/repo#number"), "{stderr}");
}

#[test]
fn a_duplicate_run_is_refused() {
    let repo = TempRepo::new("run-command-duplicate");
    let home = isolated_home("run-command-duplicate");
    assert!(start(&repo, &home, "cli-run", "json").status.success());
    let output = start(&repo, &home, "cli-run", "json");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("already has events"), "{stderr}");
}

#[test]
fn completing_a_missing_run_fails() {
    let repo = TempRepo::new("run-command-missing");
    let home = isolated_home("run-command-missing");
    let output = complete(&repo, &home, "missing-run", "json");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not exist"), "{stderr}");
}

#[test]
fn completing_twice_is_refused() {
    let repo = TempRepo::new("run-command-twice");
    let home = isolated_home("run-command-twice");
    assert!(start(&repo, &home, "cli-run", "json").status.success());
    assert!(complete(&repo, &home, "cli-run", "json").status.success());
    let output = complete(&repo, &home, "cli-run", "json");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("already complete"), "{stderr}");
}

#[test]
fn the_json_output_names_the_run_and_head_hash() {
    let repo = TempRepo::new("run-command-json");
    let home = isolated_home("run-command-json");
    let start = start(&repo, &home, "cli-run", "json");
    assert!(start.status.success());
    let stdout = String::from_utf8_lossy(&start.stdout);
    assert!(stdout.contains("\"run\":\"cli-run\""), "{stdout}");
    assert!(stdout.contains("\"head_hash\""), "{stdout}");
    let complete = complete(&repo, &home, "cli-run", "json");
    assert!(complete.status.success());
    let stdout = String::from_utf8_lossy(&complete.stdout);
    assert!(stdout.contains("\"run\":\"cli-run\""), "{stdout}");
    assert!(stdout.contains("\"head_hash\""), "{stdout}");
}
