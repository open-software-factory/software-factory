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

// C4 negative test: a lockfile-only change never earns "concurrency",
// even though a real Cargo.lock entry can name a crate containing one of
// the concurrency words.
#[test]
fn a_lockfile_change_earns_no_concurrency_signal() {
    let repo = base_repo("lockfile-no-concurrency");
    repo.write(
        "Cargo.lock",
        "[[package]]\nname = \"tokio\"\nversion = \"1.0\"\n",
    );
    repo.commit("add a lockfile");
    repo.track_origin_main();
    repo.write(
        "Cargo.lock",
        "[[package]]\nname = \"tokio\"\nversion = \"1.0\"\n\n[[package]]\nname = \"async-trait\"\nversion = \"0.1\"\n",
    );
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert!(!report.signals().contains(&"concurrency".to_string()));
}

#[test]
fn a_mutex_in_a_source_file_earns_the_concurrency_signal() {
    let repo = base_repo("mutex-earns-concurrency");
    repo.write("src/worker.c", "int worker(void) { return 0; }\n");
    repo.commit("base worker");
    repo.track_origin_main();
    repo.write(
        "src/worker.c",
        "int worker(void) { Mutex guard = mutex_new(); return 0; }\n",
    );
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert!(report.signals().contains(&"concurrency".to_string()));
}

// C4 negative test: two README.md files, same base name, whose added
// content does not match, never earn "repeated-logic".
#[test]
fn two_readme_files_with_different_content_earn_no_repeated_logic_signal() {
    let repo = base_repo("two-readmes-no-repeat");
    repo.write("docs/a/README.md", "orig a\n");
    repo.write("docs/b/README.md", "orig b\n");
    repo.commit("add two readmes");
    repo.track_origin_main();
    repo.write(
        "docs/a/README.md",
        "orig a\nunique alpha one\nunique alpha two\nunique alpha three\nunique alpha four\nunique alpha five\n",
    );
    repo.write(
        "docs/b/README.md",
        "orig b\nunique beta one\nunique beta two\nunique beta three\nunique beta four\nunique beta five\n",
    );
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert!(!report.signals().contains(&"repeated-logic".to_string()));
}

#[test]
fn two_files_sharing_an_added_block_earn_the_repeated_logic_signal() {
    let repo = base_repo("shared-block-earns-repeated-logic");
    repo.write("src/a.c", "int a(void) { return 0; }\n");
    repo.write("src/b.c", "int b(void) { return 0; }\n");
    repo.commit("base a and b");
    repo.track_origin_main();
    let block =
        "step_one();\nstep_two();\nstep_three();\nstep_four();\nstep_five();\nstep_six();\n";
    repo.write("src/a.c", &format!("int a(void) {{ return 0; }}\n{block}"));
    repo.write("src/b.c", &format!("int b(void) {{ return 0; }}\n{block}"));
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert!(report.signals().contains(&"repeated-logic".to_string()));
}

// C4 negative test: a `pub(crate)` item is not a public-surface change.
#[test]
fn a_pub_crate_item_is_not_a_public_surface() {
    let repo = base_repo("pub-crate-not-public-surface");
    repo.write("src/lib.rs", "fn existing() {}\n");
    repo.commit("base lib.rs");
    repo.track_origin_main();
    repo.write(
        "src/lib.rs",
        "fn existing() {}\npub(crate) fn helper() {}\n",
    );
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert!(!report.signals().contains(&"public surface".to_string()));
}

#[test]
fn a_new_pub_fn_earns_the_public_surface_signal() {
    let repo = base_repo("new-pub-fn-earns-public-surface");
    repo.write("src/lib.rs", "fn existing() {}\n");
    repo.commit("base lib.rs");
    repo.track_origin_main();
    repo.write("src/lib.rs", "fn existing() {}\npub fn new_helper() {}\n");
    let report = assess(&repo.dir, "origin/main").expect("assess runs");
    assert!(report.signals().contains(&"public surface".to_string()));
}
