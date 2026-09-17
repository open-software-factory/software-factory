//! Integration tests for `osf risk`: one throwaway git repository per case,
//! one change made against it, and the tier and axes that change earns.
//! Each case here answers to a case documented in
//! `crates/osf/src/risk.rs`'s own module doc comment.

mod common;

use common::{isolated_home, TempRepo};
use osf::risk::{assess, Axis, Tier};
use std::path::Path;
use std::process::Command;

/// A repository with one commit, and `origin/main` already pointing at it,
/// the way a fresh clone of a real remote would look before any change.
fn base_repo(name: &str) -> TempRepo {
    let repo = TempRepo::new(name);
    repo.write("README.md", "base\n");
    repo.write("src/app.c", "int main(){}\n");
    repo.commit("base");
    repo.track_origin_main();
    repo
}

fn axis_names(report: &osf::risk::Report) -> Vec<&'static str> {
    report.axes_add.iter().map(|a| Axis::as_str(*a)).collect()
}

#[test]
fn docs_only() {
    let repo = base_repo("docs-only");
    repo.write("README.md", "base\nmore\n");
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert_eq!(report.tier, Tier::Low);
    assert!(axis_names(&report).is_empty(), "{report:?}");
}

#[test]
fn a_large_design_doc_is_still_docs_only_and_adds_the_documents_axis() {
    let repo = base_repo("large-design-doc");
    repo.write_lines("docs/design/thing.md", 200);
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert_eq!(report.tier, Tier::Low);
    assert_eq!(axis_names(&report), vec!["documents-design-adrs"]);
}

#[test]
fn tests_only() {
    let repo = base_repo("tests-only");
    repo.write_lines("tests/app_test.c", 300);
    repo.write_lines("src/app.spec.js", 300);
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert_eq!(report.tier, Tier::Low);
    assert!(axis_names(&report).is_empty(), "{report:?}");
}

#[test]
fn two_files_sixty_lines() {
    let repo = base_repo("two-files-sixty-lines");
    repo.write_lines("src/a.c", 30);
    repo.write_lines("src/b.c", 30);
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert_eq!(report.tier, Tier::Low);
    assert!(axis_names(&report).is_empty(), "{report:?}");
}

#[test]
fn three_files_one_hundred_twenty_lines() {
    let repo = base_repo("three-files-120-lines");
    repo.write_lines("src/a.c", 50);
    repo.write_lines("src/b.c", 50);
    repo.write_lines("src/c.c", 20);
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert_eq!(report.tier, Tier::Normal);
    assert!(axis_names(&report).is_empty(), "{report:?}");
}

#[test]
fn agent_files_count_as_code_not_docs() {
    let repo = base_repo("agent-files-are-code");
    repo.write_lines(".agents/rules/x.md", 40);
    repo.write_lines(".agents/skills/y/SKILL.md", 50);
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert_eq!(report.tier, Tier::Normal);
    assert!(axis_names(&report).is_empty(), "{report:?}");
}

#[test]
fn an_auth_path_is_high() {
    let repo = base_repo("auth-path");
    repo.write_lines("src/auth/login.c", 10);
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert_eq!(report.tier, Tier::High);
}

#[test]
fn a_migration_is_high() {
    let repo = base_repo("migration");
    repo.write_lines("db/migrations/001.sql", 10);
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert_eq!(report.tier, Tier::High);
}

#[test]
fn a_workflow_file_is_high() {
    let repo = base_repo("workflow-file");
    repo.write_lines(".github/workflows/ci.yml", 10);
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert_eq!(report.tier, Tier::High);
}

#[test]
fn over_six_hundred_lines_is_high() {
    let repo = base_repo("over-600-lines");
    repo.write_lines("src/big.c", 700);
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert_eq!(report.tier, Tier::High);
}

#[test]
fn over_twelve_files_is_high() {
    let repo = base_repo("over-12-files");
    for i in 1..=13 {
        repo.write_lines(&format!("src/f{i}.c"), 2);
    }
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert_eq!(report.tier, Tier::High);
}

#[test]
fn a_screen_adds_the_ui_axis() {
    let repo = base_repo("screen-adds-ui-axis");
    repo.write_lines("lib/screens/home.dart", 40);
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert_eq!(report.tier, Tier::Low);
    assert_eq!(axis_names(&report), vec!["ui-proof-accessibility"]);
}

#[test]
fn a_manifest_adds_the_dependency_axis() {
    let repo = base_repo("manifest-adds-dependency-axis");
    repo.write_lines("pubspec.yaml", 10);
    repo.write_lines("src/x.c", 100);
    repo.write_lines("src/y.c", 100);
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert_eq!(report.tier, Tier::Normal);
    assert_eq!(axis_names(&report), vec!["dependency-licence-supply-chain"]);
}

#[test]
fn a_committed_change_counts() {
    let repo = base_repo("committed-change-counts");
    repo.write("src/app.c", "int main(){}\nx\n");
    repo.git(&["commit", "-q", "-am", "tweak"]);
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert_eq!(report.tier, Tier::Low);
}

#[test]
fn a_repo_added_high_path_is_high() {
    let repo = TempRepo::new("repo-added-high-path");
    repo.write("README.md", "base\n");
    repo.write("src/app.c", "int main(){}\n");
    repo.write(".agents/risk-paths.txt", "^src/money/\n");
    repo.commit("base with a risk-paths file");
    repo.track_origin_main();

    repo.write_lines("src/money/calc.c", 10);
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert_eq!(report.tier, Tier::High);
}

#[test]
fn a_comments_only_risk_paths_file_adds_nothing() {
    let repo = TempRepo::new("comments-only-risk-paths");
    repo.write("README.md", "base\n");
    repo.write("src/app.c", "int main(){}\n");
    repo.write(".agents/risk-paths.txt", "# a comment\n\n");
    repo.commit("base with a comments-only risk-paths file");
    repo.track_origin_main();

    repo.write_lines("src/a.c", 10);
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert_eq!(report.tier, Tier::Low);
}

/// Runs the compiled binary's `risk` subcommand and returns its JSON report.
fn run_risk_json(dir: &Path, home: &Path, extra_env: &[(&str, &str)]) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_osf"));
    cmd.current_dir(dir)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env_remove("OSF_CONFIG")
        .args(["risk", "--format", "json"]);
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
    let output = cmd.output().expect("osf risk runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn the_same_change_gives_the_same_report_across_runs_and_locales() {
    let repo = base_repo("deterministic");
    repo.write_lines("src/a.c", 50);
    repo.write_lines("src/b.c", 50);
    repo.write_lines("src/c.c", 20);

    let first = assess(&repo.dir, "origin/main").expect("assess runs");
    let second = assess(&repo.dir, "origin/main").expect("assess runs again");
    assert_eq!(first.to_json(), second.to_json());

    let home = isolated_home("risk-determinism");
    let default_locale = run_risk_json(&repo.dir, &home, &[]);
    let c_locale = run_risk_json(&repo.dir, &home, &[("LC_ALL", "C")]);
    assert_eq!(default_locale, c_locale);
}
