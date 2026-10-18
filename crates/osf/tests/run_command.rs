//! The run start and complete commands, through the built osf binary.

mod common;

use common::{isolated_home, run_osf, TempRepo};

#[test]
fn start_and_complete_write_one_complete_journal() {
    let repo = TempRepo::new("run-command");
    let home = isolated_home("run-command");
    let start = run_osf(
        &repo.dir,
        &home,
        &[
            "run",
            "start",
            "--run",
            "cli-run",
            "--work-item",
            "github:open-software-factory/software-factory#1",
            "--format",
            "json",
        ],
    );
    assert!(
        start.status.success(),
        "start failed: {}",
        String::from_utf8_lossy(&start.stderr)
    );
    let complete = run_osf(
        &repo.dir,
        &home,
        &[
            "run",
            "complete",
            "--run",
            "cli-run",
            "--outcome",
            "completed",
            "--summary",
            "done",
            "--format",
            "json",
        ],
    );
    assert!(
        complete.status.success(),
        "complete failed: {}",
        String::from_utf8_lossy(&complete.stderr)
    );
    let state = home.join(".osf").join("state");
    let run = osf::journal::read_run(&state, "cli-run").expect("the journal reads");
    assert_eq!(run.events.len(), 2);
    assert!(run.complete);
}
