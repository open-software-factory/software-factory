//! Integration tests for the skill lint, run against fixtures under
//! `tests/fixtures/skills`, plus a CLI-level check that `osf lint skill`
//! names the engine that ran the checks this project does not write itself.
//!
//! A test that asserts an `agnix:` rule id is doing two jobs: it checks the
//! finding, and it is the alarm that goes off if an engine upgrade drops
//! the rule. Do not loosen one of those assertions to a "contains" check
//! without replacing the alarm.

mod common;

use common::unique_dir;
use osf::config::{SkillConfig, WritingConfig};
use osf::lints::skill::{lint_skill, SkillFinding};
use osf::lints::{load_known_names, KnownNames, Level};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/skills")
        .join(name)
}

fn known() -> KnownNames {
    load_known_names(&[], None).expect("built-in names load")
}

fn lint(dir: &Path) -> Vec<SkillFinding> {
    lint_skill(
        dir,
        &SkillConfig::default(),
        &known(),
        &WritingConfig::default(),
    )
    .expect("SKILL.md reads")
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

/// No rule here checks for a duplicate frontmatter key; the agnix engine
/// does. This test is what stops that check going missing: if an upgrade
/// drops the rule, this file reports nothing and the test fails, rather
/// than the gate quietly losing a check that goes unnoticed.
#[test]
fn a_duplicate_frontmatter_key_is_caught_by_the_agnix_engine() {
    let findings = lint(&fixture("bad-frontmatter"));
    assert_eq!(rule_ids(&findings), vec!["agnix:AS-016"]);
    assert_all_errors(&findings);
}

#[test]
fn inert_injection_flags_the_escaped_syntax() {
    let findings = lint(&fixture("inert-injection"));
    assert_eq!(rule_ids(&findings), vec!["skill-context-injection"]);
    assert_all_errors(&findings);
}

/// The three skill rules the manual shape itself fires; none of "Checker",
/// "Design", "Tool", or "Layout" carries any of the writing lint's own
/// positive evidence for a name (an internal capital, a digit, a domain
/// suffix, or a repeated multi-word run), so none of them is reported.
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

/// Change 1: a skill file is structured text, expected to have headings,
/// same as a document. `good-skill` carries a `##` heading and must still
/// report nothing at all, including no heading finding.
#[test]
fn a_skill_with_headings_is_not_flagged_for_the_heading() {
    let findings = lint(&fixture("good-skill"));
    let rules = rule_ids(&findings);
    assert!(findings.is_empty(), "expected no findings, got: {rules:?}");
}

/// Change 2: the writing lint runs over the body, and a finding's line
/// must point at the real line in `SKILL.md`, not a line relative to the
/// body, even past a folded description spanning several lines.
#[test]
fn a_long_sentence_in_the_body_is_reported_at_the_real_line() {
    let findings = lint(&fixture("long-sentence-in-body"));
    let long: Vec<&SkillFinding> = findings
        .iter()
        .filter(|f| f.finding.rule == "long-sentence")
        .collect();
    assert_eq!(long.len(), 1, "{:?}", rule_ids(&findings));
    let finding = long.first().expect("exactly one long-sentence finding");
    assert_eq!(finding.finding.line, 12, "{:?}", finding.finding);
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
    let findings = lint_skill(
        &fixture("at-budget"),
        &cfg,
        &known(),
        &WritingConfig::default(),
    )
    .expect("SKILL.md reads");
    assert_eq!(rule_ids(&findings), vec!["skill-first-section-is-overview"]);
}

/// The body is generated here instead of committing a long fixture file:
/// two paragraphs, within the paragraph budget, but past the word budget.
/// Built from short, repeated, real sentences so the writing lint's own
/// long-sentence rule stays out of the way of this test.
#[test]
fn over_the_word_budget_fires_even_within_the_paragraph_budget() {
    let sentence = "This tool checks the folder for problems that could block a release.";
    let paragraph = std::iter::repeat_n(sentence, 7)
        .collect::<Vec<_>>()
        .join(" ");
    let content = format!(
        "---\nname: over-words\ndescription: Use this skill when the user wants a word check.\n---\n## Overview\n\n{paragraph}\n\n{paragraph}\n\n## Steps\n\n1. Run the check.\n2. Report the result.\n\nStop when the check has run once.\n"
    );
    let dir = unique_dir("osf-skill-lint-test").join("over-words");
    std::fs::create_dir_all(&dir).expect("temp dir creates");
    std::fs::write(dir.join("SKILL.md"), content).expect("fixture writes");

    let findings = lint(&dir);
    assert_eq!(rule_ids(&findings), vec!["skill-first-section-is-overview"]);

    std::fs::remove_dir_all(&dir).expect("temp dir cleans up");
}

#[test]
fn a_missing_skill_file_is_an_error() {
    let dir = unique_dir("osf-skill-lint-test").join("no-skill-file");
    std::fs::create_dir_all(&dir).expect("temp dir creates");

    let result = lint_skill(
        &dir,
        &SkillConfig::default(),
        &known(),
        &WritingConfig::default(),
    );
    assert!(result.is_err());

    std::fs::remove_dir_all(&dir).expect("temp dir cleans up");
}

/// The bug: an opened `---` that is never closed used to make the whole
/// file invisible to every rule that reads frontmatter or body, so an
/// obvious first-person description produced no finding at all. The agnix
/// engine reports the missing block, so a malformed file is never read as a
/// clean one. As with the duplicate key, this test is also the guard that
/// an engine upgrade has not dropped the check.
#[test]
fn an_unclosed_frontmatter_is_reported_not_skipped() {
    let findings = lint(&fixture("unclosed-frontmatter"));
    assert_eq!(rule_ids(&findings), vec!["agnix:AS-001"]);
    assert_all_errors(&findings);
}

/// A name that does not match the folder it sits in is a rule this project
/// never wrote, listed in the note as deferred, and now actually enforced.
#[test]
fn a_name_that_does_not_match_the_folder_is_caught() {
    let dir = unique_dir("osf-skill-lint-test").join("name-mismatch");
    std::fs::create_dir_all(&dir).expect("temp dir creates");
    std::fs::write(
        dir.join("SKILL.md"),
        "---\nname: some-other-name\ndescription: Use this skill when the user wants a check.\n---\n\nRun the check.\n",
    )
    .expect("fixture writes");

    let findings = lint(&dir);
    let rules = rule_ids(&findings);
    assert!(
        rules.iter().any(|r| r.starts_with("agnix:")),
        "the agnix engine reported nothing for a name that does not match its folder: {rules:?}"
    );

    std::fs::remove_dir_all(&dir).expect("temp dir cleans up");
}

/// A `scripts` path that is not a readable folder, such as a plain file
/// left where a folder was meant, used to be treated exactly like a skill
/// with no scripts at all: silently nothing to check.
#[test]
fn a_scripts_path_that_is_a_file_is_reported() {
    let dir = unique_dir("osf-skill-lint-test").join("scripts-is-a-file");
    std::fs::create_dir_all(&dir).expect("temp dir creates");
    std::fs::write(
        dir.join("SKILL.md"),
        "---\nname: scripts-is-a-file\ndescription: Use this skill when the user wants a check.\n---\n\nRun the check.\n",
    )
    .expect("fixture writes");
    std::fs::write(dir.join("scripts"), "not a folder").expect("scripts file writes");

    let findings = lint(&dir);
    assert_eq!(rule_ids(&findings), vec!["skill-script-unreadable"]);
    let finding = findings.first().expect("one finding reported");
    assert_eq!(finding.file, "scripts");

    std::fs::remove_dir_all(&dir).expect("temp dir cleans up");
}

/// A script file that is not valid UTF-8 used to be silently skipped, so
/// any unpinned install inside it went unchecked with no finding at all.
#[test]
fn an_unreadable_script_file_is_reported() {
    let dir = unique_dir("osf-skill-lint-test").join("unreadable-script");
    std::fs::create_dir_all(dir.join("scripts")).expect("temp dir creates");
    std::fs::write(
        dir.join("SKILL.md"),
        "---\nname: unreadable-script\ndescription: Use this skill when the user wants a check.\n---\n\nRun the check.\n",
    )
    .expect("fixture writes");
    std::fs::write(dir.join("scripts").join("install.sh"), [0xFF, 0xFE, 0x00])
        .expect("binary script writes");

    let findings = lint(&dir);
    assert_eq!(rule_ids(&findings), vec!["skill-script-unreadable"]);
    let finding = findings.first().expect("one finding reported");
    assert_eq!(finding.file, "scripts/install.sh");

    std::fs::remove_dir_all(&dir).expect("temp dir cleans up");
}

/// A clean result must say which engine checked what, so a reader can tell
/// a clean file from a check that never ran, and knows which tool to ask
/// about a rule.
#[test]
fn the_cli_names_the_engine_that_ran_the_deferred_checks() {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_osf"));
    cmd.args(["lint", "skill", "--format", "human"])
        .arg(fixture("good-skill"));
    // Scrub inherited OSF_* (e.g. this repository's own checkpoint job's OSF_CONFIG).
    for (key, _) in std::env::vars() {
        if key.starts_with("OSF_") {
            cmd.env_remove(key);
        }
    }
    let output = cmd.output().expect("osf runs");
    let stdout = String::from_utf8(output.stdout).expect("stdout is utf-8");
    assert!(stdout.contains("agnix"), "{stdout}");
    assert!(stdout.contains("name format"), "{stdout}");
}

/// Tests for the `osf-expect-skill` fixture contract as applied to a
/// skill folder: an exact match, never an exclude list. A folder is
/// checked under a label that names a `tests/fixtures` path, whether or
/// not its files actually live under one, so the contract can be tested
/// without committing more fixtures.
mod checked {
    use super::{known, SkillConfig, WritingConfig};
    use crate::common::unique_dir;
    use osf::lints::skill::lint_skill_checked;
    use std::path::{Path, PathBuf};

    const LABEL: &str = "crates/osf/tests/fixtures/skills/synthetic";

    fn temp_dir(name: &str) -> PathBuf {
        unique_dir("osf-skill-lint-checked-test").join(name)
    }

    fn write_skill(dir: &Path, skill_md: &str) {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).expect("temp dir creates");
        std::fs::write(dir.join("SKILL.md"), skill_md).expect("SKILL.md writes");
    }

    fn checked(dir: &Path, label: &str) -> Vec<osf::lints::skill::SkillFinding> {
        lint_skill_checked(
            dir,
            label,
            &SkillConfig::default(),
            &known(),
            &WritingConfig::default(),
        )
        .expect("SKILL.md reads")
    }

    /// A `name:` field matching its own directory, a body already proven
    /// clean by other fixtures, and a first-person description: the one
    /// rule under test here is `skill-first-person`.
    fn first_person_skill(name: &str) -> String {
        format!(
            "---\nname: {name}\ndescription: I can check a folder for common problems when the user wants a health check.\n---\n\nRun this skill to check a folder for basic problems before it ships.\n\n1. Read the folder listing.\n2. Report the result.\n3. Write one line per problem found.\n\nStop when every check has run once.\n"
        )
    }

    /// The same body as [`first_person_skill`], with a third-person
    /// description instead: no skill rule fires at all.
    fn third_person_skill(name: &str) -> String {
        format!(
            "---\nname: {name}\ndescription: Use this skill when the user wants a health check.\n---\n\nRun this skill to check a folder for basic problems before it ships.\n\n1. Read the folder listing.\n2. Report the result.\n3. Write one line per problem found.\n\nStop when every check has run once.\n"
        )
    }

    #[test]
    fn a_matching_declaration_produces_no_findings() {
        let dir = temp_dir("matching");
        let text = format!(
            "{}\n<!-- osf-expect-skill\nskill-first-person\n-->\n",
            first_person_skill("matching")
        );
        write_skill(&dir, &text);
        let findings = checked(&dir, LABEL);
        assert!(findings.is_empty(), "{:?}", rule_ids(&findings));
        std::fs::remove_dir_all(&dir).expect("temp dir cleans up");
    }

    /// An exact match is not an exclude list: a real finding the marker
    /// never named must still fail, not pass because a marker exists at all.
    #[test]
    fn an_undeclared_finding_fails() {
        let dir = temp_dir("undeclared");
        let text = format!(
            "{}\n<!-- osf-expect-skill\n-->\n",
            first_person_skill("undeclared")
        );
        write_skill(&dir, &text);
        let findings = checked(&dir, LABEL);
        assert_eq!(findings.len(), 1, "{:?}", rule_ids(&findings));
        let finding = findings.first().expect("one finding reported");
        assert_eq!(finding.finding.rule, "expectation-unexpected");
        assert_eq!(finding.finding.excerpt, "SKILL.md: skill-first-person");
        std::fs::remove_dir_all(&dir).expect("temp dir cleans up");
    }

    /// A fixture that stops producing a rule it declared must fail the
    /// build: the check is proof the rule still fires, not a one-time note.
    #[test]
    fn a_declared_finding_that_stops_appearing_fails() {
        let dir = temp_dir("stopped-firing");
        let text = format!(
            "{}\n<!-- osf-expect-skill\nskill-first-person\n-->\n",
            third_person_skill("stopped-firing")
        );
        write_skill(&dir, &text);
        let findings = checked(&dir, LABEL);
        assert_eq!(findings.len(), 1, "{:?}", rule_ids(&findings));
        let finding = findings.first().expect("one finding reported");
        assert_eq!(finding.finding.rule, "expectation-missing");
        assert_eq!(finding.finding.excerpt, "SKILL.md: skill-first-person");
        std::fs::remove_dir_all(&dir).expect("temp dir cleans up");
    }

    /// Nothing inside a repository, a fixture's own declaration included,
    /// may mark a scan finding expected; the comparison never even runs.
    #[test]
    fn a_scan_rule_still_cannot_be_declared() {
        let dir = temp_dir("scan-rule-declared");
        let text = format!(
            "{}\n<!-- osf-expect-skill\nskill-first-person\nscan-denied-name\n-->\n",
            first_person_skill("scan-rule-declared")
        );
        write_skill(&dir, &text);
        let findings = checked(&dir, LABEL);
        assert_eq!(findings.len(), 1, "{:?}", rule_ids(&findings));
        let finding = findings.first().expect("one finding reported");
        assert_eq!(finding.finding.rule, "expectation-forbidden-scan-rule");
        assert_eq!(finding.finding.excerpt, "scan-denied-name");
        std::fs::remove_dir_all(&dir).expect("temp dir cleans up");
    }

    /// A declaration naming a file other than `SKILL.md` matches a finding
    /// from that file, such as a script's unpinned install.
    #[test]
    fn a_declaration_can_name_a_script_file() {
        let dir = temp_dir("script-declared");
        let text = format!(
            "{}\n<!-- osf-expect-skill\nscripts/install.sh skill-script-unpinned\n-->\n",
            third_person_skill("script-declared")
        );
        write_skill(&dir, &text);
        std::fs::create_dir_all(dir.join("scripts")).expect("scripts dir creates");
        std::fs::write(
            dir.join("scripts").join("install.sh"),
            "#!/usr/bin/env bash\nnpm install -g plain-tool\n",
        )
        .expect("script writes");
        let findings = checked(&dir, LABEL);
        assert!(findings.is_empty(), "{:?}", rule_ids(&findings));
        std::fs::remove_dir_all(&dir).expect("temp dir cleans up");
    }

    /// A marker outside `tests/fixtures` never silences a real finding: the
    /// folder still lints as normal, plus a warning that the marker itself
    /// had no effect.
    #[test]
    fn a_marker_outside_fixtures_is_inert() {
        let dir = temp_dir("outside-fixtures");
        let text = format!(
            "{}\n<!-- osf-expect-skill\n-->\n",
            first_person_skill("outside-fixtures")
        );
        write_skill(&dir, &text);
        let findings = checked(&dir, "crates/osf/src/lint/skill");
        let rules = rule_ids(&findings);
        assert!(rules.contains(&"skill-first-person"), "{rules:?}");
        assert!(rules.contains(&"expectation-outside-fixtures"), "{rules:?}");
        std::fs::remove_dir_all(&dir).expect("temp dir cleans up");
    }

    fn rule_ids(findings: &[osf::lints::skill::SkillFinding]) -> Vec<&'static str> {
        findings.iter().map(|f| f.finding.rule).collect()
    }

    /// Every committed fixture must be declared exactly: a fixture with no
    /// marker never reaches this check, since [`lint_skill_checked`] passes
    /// it straight through, and every fixture under `tests/fixtures/skills`
    /// now carries one.
    #[test]
    fn every_committed_skill_fixture_matches_its_own_declaration() {
        let names = [
            "good-skill",
            "at-budget",
            "folded-description",
            "bad-frontmatter",
            "first-person",
            "inert-injection",
            "long-sentence-in-body",
            "manual-shaped",
            "no-done",
            "no-trigger",
            "unclosed-frontmatter",
            "unpinned-script",
        ];
        for name in names {
            let dir = super::fixture(name);
            let label = format!("crates/osf/tests/fixtures/skills/{name}");
            let findings = checked(&dir, &label);
            assert!(findings.is_empty(), "{name}: {:?}", rule_ids(&findings));
        }
    }
}
