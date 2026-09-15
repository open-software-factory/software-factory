//! Integration tests for the skill lint, run against fixtures under
//! `tests/fixtures/skills`, plus a CLI-level check that `osf lint skill`
//! prints the note about the checks it defers to `agnix`.

use osf::config::SkillConfig;
use osf::lint::skill::{lint_skill, SkillFinding};
use osf::lint::Level;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/skills")
        .join(name)
}

fn lint(dir: &Path) -> Vec<SkillFinding> {
    lint_skill(dir, &SkillConfig::default()).expect("SKILL.md reads")
}

fn rule_ids(findings: &[SkillFinding]) -> Vec<&'static str> {
    let mut rules: Vec<&'static str> = findings.iter().map(|f| f.finding.rule).collect();
    rules.sort_unstable();
    rules
}

/// Every kept-rule finding resolves to an error in the skill context: the
/// context is held to the strictest standard, for both rule groups.
fn assert_all_errors(findings: &[SkillFinding]) {
    for f in findings {
        assert_eq!(f.finding.level, Level::Error, "{}", f.finding.rule);
    }
}

#[test]
fn good_skill_has_no_findings() {
    let findings = lint(&fixture("good-skill"));
    let rules = rule_ids(&findings);
    assert!(findings.is_empty(), "expected no findings, got: {rules:?}");
}

#[test]
fn a_folded_description_is_read_whole() {
    let findings = lint(&fixture("folded-description"));
    let rules = rule_ids(&findings);
    assert!(findings.is_empty(), "expected no findings, got: {rules:?}");
}

#[test]
fn bad_frontmatter_flags_only_the_duplicate_key() {
    let findings = lint(&fixture("bad-frontmatter"));
    assert_eq!(rule_ids(&findings), vec!["skill-frontmatter-duplicate"]);
    assert_all_errors(&findings);
}

#[test]
fn inert_injection_flags_the_escaped_syntax() {
    let findings = lint(&fixture("inert-injection"));
    assert_eq!(rule_ids(&findings), vec!["skill-context-injection"]);
    assert_all_errors(&findings);
}

#[test]
fn manual_shaped_fixture_reads_as_a_manual() {
    let findings = lint(&fixture("manual-shaped"));
    assert_eq!(
        rule_ids(&findings),
        vec![
            "skill-descriptive-over-imperative",
            "skill-first-section-is-overview",
            "skill-reads-as-manual",
        ]
    );
    assert_all_errors(&findings);
}

#[test]
fn a_description_with_no_trigger_phrase_is_flagged() {
    let findings = lint(&fixture("no-trigger"));
    assert_eq!(rule_ids(&findings), vec!["skill-description-no-trigger"]);
}

#[test]
fn a_first_person_description_is_flagged() {
    let findings = lint(&fixture("first-person"));
    assert_eq!(rule_ids(&findings), vec!["skill-first-person"]);
}

#[test]
fn steps_with_no_stopping_point_are_flagged() {
    let findings = lint(&fixture("no-done"));
    assert_eq!(rule_ids(&findings), vec!["skill-no-done-condition"]);
}

/// The bug: `skill-script-unpinned` used to skip a package name starting
/// with `@`, so a scoped package such as `@scope/tool` was never checked.
#[test]
fn an_unpinned_scoped_package_is_caught_alongside_every_other_unpinned_install() {
    let findings = lint(&fixture("unpinned-script"));
    let script: Vec<&SkillFinding> = findings
        .iter()
        .filter(|f| f.file == "scripts/install.sh")
        .collect();
    assert_eq!(script.len(), 4, "{:?}", rule_ids(&findings));
    assert!(script
        .iter()
        .all(|f| f.finding.rule == "skill-script-unpinned"));
    let flagged: Vec<&str> = script.iter().map(|f| f.finding.excerpt.as_str()).collect();
    assert!(
        flagged.iter().any(|e| e.contains("@scope/tool")),
        "{flagged:?}"
    );
    assert!(
        !flagged.iter().any(|e| e.contains("@scope/pinned")),
        "a pinned scoped package must not be flagged: {flagged:?}"
    );
    assert!(flagged.iter().any(|e| e.contains("plain-tool")));
    assert!(flagged.iter().any(|e| e.contains("pip install helper")));
    assert!(flagged.iter().any(|e| e.contains(":latest")));
}

#[test]
fn a_first_section_at_the_budget_has_no_finding() {
    let findings = lint(&fixture("at-budget"));
    let rules = rule_ids(&findings);
    assert!(findings.is_empty(), "expected no findings, got: {rules:?}");
}

/// Change: a config value lowering the paragraph budget makes the same
/// first section fire that passed under the compiled default.
#[test]
fn a_tighter_configured_paragraph_budget_flags_the_same_section() {
    let cfg = SkillConfig {
        overview_max_paragraphs: 1,
        ..SkillConfig::default()
    };
    let findings = lint_skill(&fixture("at-budget"), &cfg).expect("SKILL.md reads");
    assert_eq!(rule_ids(&findings), vec!["skill-first-section-is-overview"]);
}

/// The body is generated here instead of committing a long fixture file:
/// two paragraphs, within the paragraph budget, but past the word budget.
#[test]
fn over_the_word_budget_fires_even_within_the_paragraph_budget() {
    let words = "lorem ".repeat(65);
    let content = format!(
        "---\nname: over-words\ndescription: Use this skill when the user wants a word check.\n---\n## Overview\n\n{words}\n\n{words}\n\n## Steps\n\n1. Run the check.\n2. Report the result.\n\nStop when the check has run once.\n"
    );
    let dir = std::env::temp_dir()
        .join("osf-skill-lint-test")
        .join("over-words");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir creates");
    std::fs::write(dir.join("SKILL.md"), content).expect("fixture writes");

    let findings = lint(&dir);
    assert_eq!(rule_ids(&findings), vec!["skill-first-section-is-overview"]);

    std::fs::remove_dir_all(&dir).expect("temp dir cleans up");
}

#[test]
fn a_missing_skill_file_is_an_error() {
    let dir = std::env::temp_dir()
        .join("osf-skill-lint-test")
        .join("no-skill-file");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir creates");

    let result = lint_skill(&dir, &SkillConfig::default());
    assert!(result.is_err());

    std::fs::remove_dir_all(&dir).expect("temp dir cleans up");
}

/// `osf lint skill` must never let a clean result read as a full
/// validation: it names what it does not check and points at `agnix`.
#[test]
fn the_cli_prints_the_unchecked_rules_note() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_osf"))
        .args(["lint", "skill", "--format", "human"])
        .arg(fixture("good-skill"))
        .output()
        .expect("osf runs");
    let stdout = String::from_utf8(output.stdout).expect("stdout is utf-8");
    assert!(stdout.contains("agnix"), "{stdout}");
    assert!(stdout.contains("name format"), "{stdout}");
}
