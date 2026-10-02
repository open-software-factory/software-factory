//! CLI-level case for `osf pr status render --base`: the test summary is
//! computed against `HEAD` and threaded into the block, the same way `osf
//! pr status refresh` does, without going through a pull request at all.

mod common;

use common::{isolated_home, run_osf, TempRepo};
use std::fs;

fn write_inputs(repo: &TempRepo) -> (std::path::PathBuf, std::path::PathBuf) {
    let tier_json = repo.dir.join("tier.json");
    fs::write(
        &tier_json,
        r#"{"tier":"normal","reasons":["small change"]}"#,
    )
    .expect("tier json writes");
    let review_json = repo.dir.join("review.json");
    fs::write(
        &review_json,
        r#"{"reviewDecision":"APPROVED","reviews":[],"comments":[]}"#,
    )
    .expect("review json writes");
    (tier_json, review_json)
}

#[test]
fn render_with_base_carries_the_test_summary_in_the_block() {
    let repo = TempRepo::new("pr-status-render-base");
    repo.write("crates/osf/src/thing.rs", "\n");
    let base = repo.commit("base");
    repo.write("crates/osf/src/thing.rs", "#[test]\nfn a_new_test() {}\n");
    repo.commit("head");

    let home = isolated_home("pr-status-render-base");
    let (tier_json, review_json) = write_inputs(&repo);

    let output = run_osf(
        &repo.dir,
        &home,
        &[
            "pr",
            "status",
            "render",
            "--tier-json",
            tier_json.to_str().expect("utf8 path"),
            "--gates",
            "build: passed",
            "--review-json",
            review_json.to_str().expect("utf8 path"),
            "--base",
            &base,
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("| Tests | 1 added, 0 changed, 0 removed |"),
        "{stdout}"
    );
}

#[test]
fn render_with_no_base_carries_no_test_summary() {
    let repo = TempRepo::new("pr-status-render-no-base");
    repo.write("a.md", "Clean.\n");
    repo.commit("only commit");
    let home = isolated_home("pr-status-render-no-base");
    let (tier_json, review_json) = write_inputs(&repo);

    let output = run_osf(
        &repo.dir,
        &home,
        &[
            "pr",
            "status",
            "render",
            "--tier-json",
            tier_json.to_str().expect("utf8 path"),
            "--gates",
            "build: passed",
            "--review-json",
            review_json.to_str().expect("utf8 path"),
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("| Tests | ⏸ not run |"), "{stdout}");
}

#[test]
fn render_writes_the_current_head_into_the_marker_and_heading() {
    let repo = TempRepo::new("pr-status-render-head");
    repo.write("a.md", "Clean.\n");
    let head = repo.commit("only commit");
    let home = isolated_home("pr-status-render-head");
    let (tier_json, review_json) = write_inputs(&repo);

    let output = run_osf(
        &repo.dir,
        &home,
        &[
            "pr",
            "status",
            "render",
            "--tier-json",
            tier_json.to_str().expect("utf8 path"),
            "--gates",
            "build: passed",
            "--review-json",
            review_json.to_str().expect("utf8 path"),
        ],
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&format!("<!-- osf:status:start head={head} -->")),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("### Status at {}", &head[..7])),
        "{stdout}"
    );
}
