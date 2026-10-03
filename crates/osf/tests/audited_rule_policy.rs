//! Consumer-level policy tests. Raw detector fixtures remain independent
//! of enforcement so a disabled check can still be evaluated and improved.

mod common;

use common::{isolated_home, run_osf, TempRepo};
use osf::config::WritingConfig;
use osf::lints::{load_known_names, Context};
use std::path::Path;

fn output_text(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn ordinary_writing_does_not_enforce_audited_detectors_but_raw_fixtures_do() {
    let repo = TempRepo::new("audit-writing-policy");
    let home = isolated_home("audit-writing-policy");
    repo.write("osf.toml", "[writing]\nmust_explain_names = [\"Fable\"]\n");
    let mut cases = vec![
        ("undefined-name", "Use Fable for this.\n".to_string()),
        (
            "undefined-name-at-start",
            "Deploy with FableDb today.\n".to_string(),
        ),
        ("arrow", "Input -> output.\n".to_string()),
        ("filler", "Let me know if that helps.\n".to_string()),
        (
            "numbers-in-prose",
            "It ran 12 axes over 3 rounds in 41 minutes.\n".to_string(),
        ),
        (
            "bold-sentence",
            "**Run the full suite before every release, without exception.**\n".to_string(),
        ),
    ];
    for rule in [
        "contrast-tail",
        "aphorism",
        "rhetorical-setup",
        "universal-pronoun",
        "colon-reveal",
        "short-kicker",
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/fixtures/writing/{rule}-positive.md"));
        cases.push((rule, std::fs::read_to_string(path).expect("fixture reads")));
    }
    let known = load_known_names(&[], None).expect("names load");
    let cfg = WritingConfig {
        must_explain_names: vec!["Fable".to_string()],
        ..WritingConfig::default()
    };
    for (rule, text) in cases {
        let raw =
            osf::lints::writing::lint_writing(&text, &known, &cfg, Context::Document, false, true);
        assert!(
            raw.iter().any(|f| f.rule == rule),
            "raw detector lost: {rule}: {raw:?}"
        );
        repo.write("ordinary.md", &text);
        let out = run_osf(
            &repo.dir,
            &home,
            &["lint", "writing", "--format", "json", "ordinary.md"],
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let findings: Vec<serde_json::Value> = stdout
            .lines()
            .map(|line| serde_json::from_str(line).expect("finding JSON"))
            .collect();
        assert!(
            findings.iter().all(|f| f["rule"] != rule),
            "disabled {rule} enforced: {stdout}"
        );
    }
    repo.write("ordinary.md", "Fixed in #125 today.\n");
    let out = run_osf(&repo.dir, &home, &["lint", "writing", "ordinary.md"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "retained rule must enforce: {}",
        output_text(&out)
    );
    assert!(output_text(&out).contains("bare-reference"));
}

#[test]
fn skill_enforcement_uses_both_audited_families_without_hiding_syntax_errors() {
    let repo = TempRepo::new("audit-skill-policy");
    let home = isolated_home("audit-skill-policy");
    for fixture in ["no-trigger", "first-person", "manual-shaped", "no-done"] {
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("tests/fixtures/skills/{fixture}/SKILL.md")),
        )
        .expect("fixture reads");
        repo.write("skills/demo/SKILL.md", &text);
        let out = run_osf(
            &repo.dir,
            &home,
            &["lint", "skill", "--format", "human", "skills/demo"],
        );
        let printed = output_text(&out);
        for rule in [
            "skill-description-no-trigger",
            "skill-first-person",
            "skill-first-section-is-overview",
            "skill-descriptive-over-imperative",
            "skill-no-done-condition",
        ] {
            assert!(
                !String::from_utf8_lossy(&out.stdout).contains(&format!("[{rule}]")),
                "disabled {rule} enforced: {printed}"
            );
        }
    }
    repo.write("skills/demo/SKILL.md", "---\nname: demo\ndescription: Use when checking a folder.\n---\n\n## Steps\n\n1. Count 12 axes over 3 rounds in 41 minutes.\n");
    let out = run_osf(
        &repo.dir,
        &home,
        &["lint", "skill", "--format", "human", "skills/demo"],
    );
    assert!(
        out.status.success(),
        "writing policy missing from skill: {}",
        output_text(&out)
    );
    repo.write("skills/demo/SKILL.md", "---\nname: demo\nname: demo\ndescription: Use when checking a folder.\n---\n\n1. Run the check.\n");
    let out = run_osf(
        &repo.dir,
        &home,
        &["lint", "skill", "--format", "human", "skills/demo"],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "syntax check lost: {}",
        output_text(&out)
    );
    assert!(output_text(&out).contains("agnix:AS-016"));
}

#[test]
fn commit_gate_does_not_enforce_audited_quantity_rule() {
    let repo = TempRepo::new("audit-commit-policy");
    let home = isolated_home("audit-commit-policy");
    repo.write("MSG", "Count 12 axes over 3 rounds in 41 minutes.\n");
    let out = run_osf(
        &repo.dir,
        &home,
        &[
            "verify",
            "--stage",
            "pre-commit",
            "--message-file",
            "MSG",
            "--gate",
            "--format",
            "human",
        ],
    );
    assert!(
        out.status.success(),
        "disabled rule blocked commit: {}",
        output_text(&out)
    );
    assert!(
        output_text(&out).contains("0 warning(s)"),
        "disabled warning leaked: {}",
        output_text(&out)
    );
}

#[test]
fn ci_gate_cannot_reenable_disabled_rules_or_disable_retained_rules() {
    let repo = TempRepo::new("audit-ci-policy");
    let home = isolated_home("audit-ci-policy");
    repo.write("base.txt", "base\n");
    let base = repo.commit("Add base file");
    repo.write("guide.md", "Count 12 axes over 3 rounds in 41 minutes.\n");
    repo.write(
        "skills/demo/SKILL.md",
        "---\nname: demo\ndescription: Checks a folder for problems.\n---\n\n1. Run the check.\n",
    );
    repo.write("poison.toml", "[writing.levels]\nnumbers-in-prose = \"error\"\nbare-reference = \"off\"\n[skill.levels]\nskill-description-no-trigger = \"error\"\n");
    repo.commit("Add checked prose");
    let args = [
        "--config",
        "poison.toml",
        "verify",
        "--stage",
        "ci",
        "--base",
        &base,
        "--gate",
        "--format",
        "human",
    ];
    let out = run_osf(&repo.dir, &home, &args);
    assert!(
        out.status.success(),
        "gate reenabled a disabled rule: {}",
        output_text(&out)
    );
    assert!(
        output_text(&out).contains("total: 0 error(s), 0 warning(s)"),
        "disabled finding leaked: {}",
        output_text(&out)
    );
    repo.write("guide.md", "Fixed in #125 today.\n");
    repo.commit("Add ordinary reference");
    let out = run_osf(&repo.dir, &home, &args);
    assert_eq!(
        out.status.code(),
        Some(1),
        "gate accepted poisoned retained rule: {}",
        output_text(&out)
    );
    assert!(output_text(&out).contains("bare-reference"));
}

#[test]
fn gate_fixtures_still_evaluate_disabled_raw_detectors() {
    let repo = TempRepo::new("audit-fixture-policy");
    let home = isolated_home("audit-fixture-policy");
    repo.write("base.txt", "base\n");
    let base = repo.commit("Add base file");
    repo.write(
        "tests/fixtures/quantity.md",
        "Count 12 axes over 3 rounds in 41 minutes.\n\n<!-- osf-expect numbers-in-prose -->\n",
    );
    repo.commit("Add detector fixture");
    let args = ["verify", "--stage", "ci", "--base", &base, "--gate"];
    let out = run_osf(&repo.dir, &home, &args);
    assert!(
        out.status.success(),
        "raw fixture not evaluated: {}",
        output_text(&out)
    );
    repo.write(
        "tests/fixtures/quantity.md",
        "Count the axes.\n\n<!-- osf-expect numbers-in-prose -->\n",
    );
    repo.commit("Break detector fixture");
    let out = run_osf(&repo.dir, &home, &args);
    assert_eq!(
        out.status.code(),
        Some(1),
        "fixture mismatch hidden: {}",
        output_text(&out)
    );
    assert!(output_text(&out).contains("expectation-missing"));
}

#[test]
fn explain_preserves_disabled_rule_purpose_and_reports_enforcement_status() {
    let repo = TempRepo::new("audit-explain-policy");
    let home = isolated_home("audit-explain-policy");
    for rule in ["numbers-in-prose", "skill-description-no-trigger"] {
        let out = run_osf(&repo.dir, &home, &["explain", rule]);
        assert!(out.status.success(), "disabled rule definition lost");
        let text = output_text(&out);
        assert!(
            text.contains("disabled by default"),
            "disabled status missing: {text}"
        );
        assert!(text.contains("### Why it is bad"), "purpose lost: {text}");
    }
}

#[test]
fn clean_enforcement_reports_what_was_disabled() {
    let repo = TempRepo::new("audit-coverage-policy");
    let home = isolated_home("audit-coverage-policy");
    repo.write("ordinary.md", "The check passed.\n");
    for args in [
        vec!["lint", "writing", "--format", "human", "ordinary.md"],
        vec!["lint", "writing", "--format", "json", "ordinary.md"],
    ] {
        let out = run_osf(&repo.dir, &home, &args);
        let text = output_text(&out);
        assert!(out.status.success(), "{text}");
        assert!(
            text.contains("disabled from enforcement:"),
            "silent coverage omission: {text}"
        );
        assert!(
            text.contains("numbers-in-prose"),
            "disabled coverage missing: {text}"
        );
    }
}

#[test]
fn required_reference_description_is_not_a_parenthetical_violation() {
    let known = load_known_names(&[], None).expect("names load");
    let text = "See [owner/repo#125 (fix the slow loading path)](https://example.com/125).";
    let raw = osf::lints::writing::lint_writing(
        text,
        &known,
        &WritingConfig::default(),
        Context::Document,
        false,
        true,
    );
    assert!(
        !raw.iter().any(|f| f.rule == "parenthetical"),
        "contradictory label finding: {raw:?}"
    );
    assert!(osf::lints::rule_meta("parenthetical").is_none());
}

#[test]
fn manual_shape_retains_individual_detectors_without_the_redundant_vote() {
    let known = load_known_names(&[], None).expect("names load");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/skills/manual-shaped");
    let findings = osf::lints::skill::lint_skill(
        &dir,
        &osf::config::SkillConfig::default(),
        &known,
        &WritingConfig::default(),
    )
    .expect("skill reads");
    assert!(
        !findings
            .iter()
            .any(|f| f.finding.rule == "skill-reads-as-manual"),
        "redundant composite still reported"
    );
    for id in [
        "skill-first-section-is-overview",
        "skill-descriptive-over-imperative",
    ] {
        assert!(
            findings.iter().any(|f| f.finding.rule == id),
            "disabled detector lost: {id}"
        );
    }
    assert!(osf::lints::rule_meta("skill-reads-as-manual").is_none());
}
