//! Regressions for the audit's precise detector fixes.

mod common;

use common::{public_config, TempRepo};
use osf::config::WritingConfig;
use osf::lints::{load_known_names, Context};

fn writing_rules(text: &str, context: Context) -> Vec<&'static str> {
    osf::lints::writing::lint_writing(
        text,
        &load_known_names(&[], None).expect("names load"),
        &WritingConfig::default(),
        context,
        false,
        true,
    )
    .into_iter()
    .map(|f| f.rule)
    .collect()
}

#[test]
fn trailer_field_case_is_insensitive_for_every_supported_agent() {
    let repo = TempRepo::new("audit-trailer-case");
    let rules = osf::scan::Rules::build(&repo.dir, &public_config()).expect("rules build");
    for agent in osf::agents::AGENTS {
        for field in [
            "Co-Authored-By",
            "co-authored-by",
            "CO-AUTHORED-BY",
            "cO-aUtHoReD-bY",
        ] {
            for newline in ["\n", "\r\n"] {
                let text = format!(
                    "Add file{newline}{newline}{field}: {} <agent@example.com>{newline}",
                    agent.name
                );
                let findings = rules.scan_text(&text, Context::Commit);
                assert_eq!(
                    findings
                        .iter()
                        .filter(|f| f.rule == "scan-coauthor-trailer")
                        .count(),
                    1,
                    "missed {field} for {}: {findings:?}",
                    agent.name
                );
            }
        }
    }
    let findings = rules.scan_text(
        "co-authored-by-not: Example <agent@example.com>",
        Context::Commit,
    );
    assert!(!findings.iter().any(|f| f.rule == "scan-coauthor-trailer"));
    let findings = rules.scan_text(
        "co-authored-by-not: Example <agent@example.com>\nCo-Authored-By: Claude <noreply@anthropic.com>\n",
        Context::Commit,
    );
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.rule == "scan-coauthor-trailer")
            .count(),
        1,
        "only the real trailer counts: {findings:?}"
    );
}

#[test]
fn only_generated_commit_merge_subject_is_exempt_from_bare_reference() {
    let subject = "Merge pull request #215 from owner/docs/reference-fixes";
    assert!(!writing_rules(subject, Context::Commit).contains(&"bare-reference"));
    assert!(writing_rules(subject, Context::Document).contains(&"bare-reference"));
    assert!(writing_rules(
        "Merge pull request #215 from owner/docs/reference-fixes\n\nFixed in #125 today.",
        Context::Commit
    )
    .contains(&"bare-reference"));
    for body in ["Fixed in #125 today.", "Fixed in #215 today."] {
        let text = format!("{subject}\n{body}");
        assert!(
            writing_rules(&text, Context::Commit).contains(&"bare-reference"),
            "body reference was hidden after a generated subject: {text}"
        );
    }
    for text in [
        "Fixed in #125 today.",
        "Merge pull request #215 from branch",
        "Merge pull request #215 from owner/branch and #125",
        "\nMerge pull request #215 from owner/branch",
    ] {
        assert!(
            writing_rules(text, Context::Commit).contains(&"bare-reference"),
            "ordinary reference exempted: {text}"
        );
    }
    let identical = format!("{subject}\nFixed in #215 today.");
    let finding = osf::lints::writing::lint_writing(
        &identical,
        &load_known_names(&[], None).expect("names load"),
        &WritingConfig::default(),
        Context::Commit,
        false,
        true,
    )
    .into_iter()
    .find(|f| f.rule == "bare-reference")
    .expect("body occurrence remains");
    assert_eq!(finding.line, 2, "body line location changed: {finding:?}");
}

#[test]
fn labelled_references_need_links_except_in_commit_messages() {
    let text = "Fixed in repo#125 (the loading fix) today.";
    assert!(!writing_rules(text, Context::Commit).contains(&"reference-without-link"));
    for context in [Context::Document, Context::Skill, Context::Transcript] {
        assert!(
            writing_rules(text, context).contains(&"reference-without-link"),
            "link check lost in {context:?}"
        );
    }
    assert!(writing_rules("Fixed in repo#125 today.", Context::Commit)
        .contains(&"reference-without-label"));
}

#[test]
fn weasel_claim_is_exempt_only_with_a_safe_labelled_link_after_it_in_its_sentence() {
    let linked =
        "Studies show that small changes help [the review data](https://example.org/review).";
    let before = writing_rules(linked, Context::Document);
    assert!(!before.contains(&"weasel-attribution"), "{before:?}");
    // The same sentence must still be checked by this rule, so the empty result is real.
    assert!(
        writing_rules("Studies show that small changes help.", Context::Document)
            .contains(&"weasel-attribution")
    );

    let mixed = "Experts agree [the data](https://example.org/a) and studies show this works.";
    let count = writing_rules(mixed, Context::Document)
        .iter()
        .filter(|id| **id == "weasel-attribution")
        .count();
    assert_eq!(
        count, 1,
        "only the phrase after the link is unsourced: {mixed}"
    );

    for text in [
        "[The review data](https://example.org/review) says studies show that small changes help.",
        "[Review data](https://example.org/review) and experts agree small changes help.",
        "Studies show that small changes help, according to the review data.",
        "Studies show that small changes help. [Review data](https://example.org/review).",
        "Studies show that small changes help https://example.org/review.",
        "Studies show that small changes help <https://example.org/review>.",
        "Studies show that small changes help [](https://example.org/review).",
        "Studies show that small changes help [review data](javascript:alert(1)).",
    ] {
        assert!(
            writing_rules(text, Context::Document).contains(&"weasel-attribution"),
            "unsupported source syntax excused claim: {text}"
        );
    }
    let meta = osf::lints::rule_meta("weasel-attribution").expect("rule is documented");
    assert!(
        meta.doc.matches("after the phrase").count() >= 2,
        "the rule text must say the link comes after the phrase: {}",
        meta.doc
    );
}

#[test]
fn em_dash_rule_leaves_en_dash_ranges_and_double_hyphens_alone() {
    for (text, count) in [
        ("Check pages 4–8.", 0),
        ("Check the result — then stop.", 1),
        ("The code uses -- as a separator.", 0),
    ] {
        let findings = writing_rules(text, Context::Document);
        assert_eq!(
            findings.iter().filter(|id| **id == "em-dash").count(),
            count,
            "{text}: {findings:?}"
        );
    }
}

#[test]
fn direct_install_tokens_distinguish_exact_versions_from_files_and_floating_specs() {
    let repo = TempRepo::new("audit-install-tokens");
    repo.write(
        "skills/demo/SKILL.md",
        "---\nname: demo\ndescription: Use when checking a folder.\n---\n\n1. Run the check.\n",
    );
    let cases = [
        ("pip install -r requirements.txt", 0),
        ("pip3 install --requirement=requirements.txt", 0),
        ("pip install -rrequirements.txt", 0),
        ("pip install example-tool", 1),
        ("pip3 install example-tool==1.2.3", 0),
        ("pip install example-tool>=1.2.3", 1),
        ("pip install -r requirements.txt example-tool", 1),
        ("npm install @scope/example-tool", 1),
        ("npm install -g @scope/example-tool@1.2.3", 0),
        ("npm install example-tool@=1.2.3", 0),
        ("npm install @scope/example-tool@=1.2.3", 0),
        ("pnpm add example-tool@=1.2.3", 0),
        ("pnpm add @scope/example-tool@=1.2.3", 0),
        ("npm install owner/repo", 0),
        ("npm install tool-1.2.3.tgz", 0),
        ("pnpm add owner/repo", 0),
        ("pnpm add tool-1.2.3.tgz", 0),
        ("pip install SomePackage-1.0.tar", 0),
        ("pip install SomePackage-1.0.gz", 0),
        ("pip install ./tool-1.2.3.whl", 0),
        ("pip install tool-1.2.3.tar.gz", 0),
        ("npm install example-tool@latest", 1),
        ("npm install example-tool@^1.2.3", 1),
        ("npm install --global example-tool@1.2.3", 0),
        ("npm install", 0),
        ("pnpm add -g example-tool", 1),
        ("pnpm add example-tool@1.2.3", 0),
        ("pnpm add example-tool@latest", 1),
        ("cargo install example-tool --locked", 1),
        ("cargo install example-tool --version 1.2.3 --locked", 0),
        ("cargo install example-tool --version=1.2.3", 0),
        ("cargo install example-tool --version '^1.2.3'", 1),
        ("docker run example/image:latest", 1),
        ("podman run example/image:latest", 1),
        ("echo 'example/image:latest'", 0),
        ("# pip install example-tool", 0),
        ("Write-Output 'pip install example-tool'", 0),
        ("echo npm install example-tool", 0),
    ];
    let known = load_known_names(&[], None).expect("names load");
    for (command, count) in cases {
        repo.write("skills/demo/scripts/install.txt", &format!("{command}\n"));
        let findings = osf::lints::skill::lint_skill(
            &repo.dir.join("skills/demo"),
            &osf::config::SkillConfig::default(),
            &known,
            &WritingConfig::default(),
        )
        .expect("skill reads");
        let found: Vec<_> = findings
            .iter()
            .filter(|f| f.finding.rule == "skill-script-unpinned")
            .map(|f| &f.finding)
            .collect();
        assert_eq!(found.len(), count, "{command}: {found:?}");
    }
}

#[test]
fn skill_pin_coverage_explicitly_excludes_interpreted_and_indirect_commands() {
    let repo = TempRepo::new("audit-pin-coverage");
    let home = common::isolated_home("audit-pin-coverage");
    repo.write(
        "skills/demo/SKILL.md",
        "---\nname: demo\ndescription: Use when checking a folder.\n---\n\n1. Run the check.\n",
    );
    repo.write(
        "skills/demo/scripts/install.txt",
        "npm install example-tool@$VERSION\n",
    );
    let out = common::run_osf(
        &repo.dir,
        &home,
        &["lint", "skill", "--format", "json", "skills/demo"],
    );
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(
        text.contains("script-pin coverage:"),
        "coverage missing: {text}"
    );
    assert!(
        text.contains("variables") && text.contains("requirements-file contents"),
        "omissions missing: {text}"
    );
}
