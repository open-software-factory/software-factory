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
    let dir = std::env::temp_dir().join(format!("osf-wt-no-repo-{}", std::process::id())); // osf: temp-dir allowed, a throwaway directory outside any repository
    std::fs::create_dir_all(&dir).expect("make a directory");
    let output = osf(&["work-tracker", "ready"])
        .current_dir(&dir)
        .output()
        .expect("runs osf");
    std::fs::remove_dir_all(&dir).ok();
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
fn an_empty_recap_is_a_usage_error() {
    let path = std::env::temp_dir().join(format!("osf-wt-recap-{}", std::process::id())); // osf: temp-dir allowed, a throwaway recap the command must refuse
    std::fs::write(&path, "   \n").expect("write recap");
    let output = osf(&[
        "work-tracker",
        "report",
        "github:open-software-factory/software-factory#1",
        "--state",
        "in-review",
        "--recap",
        path.to_str().expect("path is text"),
    ])
    .output()
    .expect("runs osf");
    std::fs::remove_file(&path).ok();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("recap is empty"), "{stderr}");
}

#[test]
fn the_help_lists_both_commands() {
    let output = osf(&["work-tracker", "--help"]).output().expect("runs osf");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ready"), "{stdout}");
    assert!(stdout.contains("report"), "{stdout}");
}
