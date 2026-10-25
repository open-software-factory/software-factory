//! The work-tracker command's contract without a real project: a missing
//! table, a usage error, and the two subcommands.

use std::process::Command;

fn osf(args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_osf"));
    cmd.args(args);
    cmd
}

#[test]
fn a_missing_work_tracker_table_is_named() {
    let output = osf(&["work-tracker", "ready"]).output().expect("runs osf");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[work_tracker]"), "{stderr}");
}

#[test]
fn reading_two_items_at_once_is_a_usage_error() {
    let output = osf(&["work-tracker", "ready", "--limit", "2"])
        .output()
        .expect("runs osf");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("one item at a time"), "{stderr}");
}

#[test]
fn a_blocked_report_needs_a_cause() {
    let output = osf(&[
        "work-tracker",
        "report",
        "github:open-software-factory/software-factory#1",
        "--state",
        "blocked",
        "--recap",
        "recap.md",
    ])
    .output()
    .expect("runs osf");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--cause"), "{stderr}");
}

#[test]
fn an_unknown_state_is_a_usage_error() {
    let output = osf(&[
        "work-tracker",
        "report",
        "github:open-software-factory/software-factory#1",
        "--state",
        "done",
        "--recap",
        "recap.md",
    ])
    .output()
    .expect("runs osf");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown state done"), "{stderr}");
}

#[test]
fn the_help_lists_both_commands() {
    let output = osf(&["work-tracker", "--help"]).output().expect("runs osf");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ready"), "{stdout}");
    assert!(stdout.contains("report"), "{stdout}");
}
