//! Integration tests for `osf scan`: a throwaway git repository stands in
//! for a real one, so the git-tracked-files path and the commit-range path
//! are both exercised against real git plumbing, not a mock.

mod common;

use common::{isolated_home, run_osf, TempRepo};
use osf::config::ScanConfig;
use osf::scan::{scan_commits, scan_paths, Rules};

fn rules() -> Rules {
    Rules::build(ScanConfig::default()).expect("empty config builds")
}

#[test]
fn scanning_with_no_paths_covers_every_tracked_file() {
    let repo = TempRepo::new("tracked-files");
    repo.write(
        "notes.md",
        "See https://claude.ai/code/session_abc123 here.\n",
    );
    repo.write("clean.md", "Nothing to see here.\n");
    repo.commit("add fixtures");

    let found = scan_paths(&repo.dir, &[], &rules()).expect("scan runs");
    let with_findings: Vec<&(String, Vec<osf_lint_core::Finding>)> =
        found.iter().filter(|(_, f)| !f.is_empty()).collect();
    assert_eq!(with_findings.len(), 1, "{found:?}");
    let (name, findings) = with_findings.first().expect("one flagged file");
    assert_eq!(name.as_str(), "notes.md");
    assert_eq!(findings.len(), 1);
}

#[test]
fn a_binary_tracked_file_is_skipped_not_scanned() {
    let repo = TempRepo::new("binary-file");
    std::fs::write(repo.dir.join("blob.bin"), [0u8, 1, 2, 3, b'C', b'o']).expect("binary writes");
    repo.commit("add a binary file");

    let found = scan_paths(&repo.dir, &[], &rules()).expect("scan runs");
    assert!(
        found.iter().all(|(_, f)| f.is_empty()),
        "a binary file must never be scanned as text: {found:?}"
    );
}

#[test]
fn an_explicit_path_is_scanned_even_when_not_tracked() {
    let repo = TempRepo::new("explicit-path");
    repo.write(
        "untracked.md",
        "Fixed near D:\\Users\\pat\\work\\notes.md today.\n",
    );

    let target = repo.dir.join("untracked.md");
    let found = scan_paths(&repo.dir, std::slice::from_ref(&target), &rules()).expect("scan runs");
    assert_eq!(found.len(), 1);
    let (_, findings) = found.first().expect("one file scanned");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings.first().map(|f| f.rule), Some("scan-local-path"));
}

#[test]
fn a_commit_range_scan_reports_the_hash_and_the_line() {
    let repo = TempRepo::new("commit-range");
    repo.write("a.txt", "one\n");
    let base = repo.commit("base commit");
    repo.write("b.txt", "two\n");
    let bad = repo.commit("Fix the bug.\n\nCo-Authored-By: Someone <someone@example.com>\n");

    let range = format!("{base}..{bad}");
    let found = scan_commits(&repo.dir, &range, &rules()).expect("commit scan runs");
    let flagged: Vec<&(String, Vec<osf_lint_core::Finding>)> =
        found.iter().filter(|(_, f)| !f.is_empty()).collect();
    assert_eq!(flagged.len(), 1, "{found:?}");
    let (hash, findings) = flagged.first().expect("one flagged commit");
    assert_eq!(hash.as_str(), bad);
    let finding = findings.first().expect("one finding");
    assert_eq!(finding.rule, "scan-coauthor-trailer");
    assert_eq!(finding.line, 3);
}

/// Exit code 0: git runs fine, and nothing it finds is a problem.
#[test]
fn a_clean_scan_exits_zero() {
    let repo = TempRepo::new("exit-clean");
    repo.write("clean.md", "Nothing to see here.\n");
    repo.commit("add a clean file");
    let home = isolated_home("exit-clean");

    let output = run_osf(&repo.dir, &home, &["scan", "--format", "json"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

/// Exit code 1: git runs fine, and it finds something that must never
/// reach a public repository.
#[test]
fn a_scan_that_finds_something_exits_one() {
    let repo = TempRepo::new("exit-dirty");
    repo.write(
        "notes.md",
        "Co-Authored-By: Someone <someone@example.com>\n",
    );
    repo.commit("add a flagged file");
    let home = isolated_home("exit-dirty");

    let output = run_osf(&repo.dir, &home, &["scan", "--format", "json"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
}

/// Exit code 2: the tool itself could not run, distinct from running and
/// finding nothing. A named path that does not exist is never "clean".
#[test]
fn scanning_a_path_that_does_not_exist_exits_two() {
    let repo = TempRepo::new("exit-missing-path");
    repo.write("a.txt", "one\n");
    repo.commit("add a file");
    let home = isolated_home("exit-missing-path");

    let output = run_osf(
        &repo.dir,
        &home,
        &["scan", "--format", "json", "does-not-exist.md"],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}

/// Exit code 2 again, this time because `--commits` names a range git
/// cannot resolve: still "could not run", not "found nothing".
#[test]
fn scanning_an_unresolvable_commit_range_exits_two() {
    let repo = TempRepo::new("exit-bad-range");
    repo.write("a.txt", "one\n");
    repo.commit("add a file");
    let home = isolated_home("exit-bad-range");

    let output = run_osf(
        &repo.dir,
        &home,
        &["scan", "--commits", "not-a-real-ref..HEAD"],
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}
