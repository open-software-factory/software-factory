//! Writing lint: deterministic checks on prose written for people.
//!
//! Text is split into paragraphs and sentences. Fenced code, tables, HTML
//! comments, inline code and a `---`-delimited front-matter block are never
//! checked. Every rule has an id, a level and a one-line message that names
//! the offending text.

pub mod agnix;
mod meta;
mod names;
mod rules;
pub mod skill;

pub use meta::RuleMeta;
pub use osf_lint_core::{
    check_expectation, parse_expectation, Context, Finding, KnownNames, Level, Mismatch,
    Remediation,
};

/// A rule's doc text and metadata, whether it is a writing rule or a skill rule.
#[must_use]
pub fn rule_meta(id: &str) -> Option<&'static RuleMeta> {
    meta::rule_meta(id).or_else(|| skill::rule_meta(id))
}

use crate::config::WritingConfig;
use std::path::Path;

/// # Errors
/// Returns an error if `path` is given and cannot be read.
pub fn load_known_names(extra: &[String], path: Option<&Path>) -> Result<KnownNames, String> {
    let built_in: Vec<&str> = names::BUILT_IN
        .iter()
        .copied()
        .chain(extra.iter().map(String::as_str))
        .collect();
    osf_lint_core::load_known_names(&built_in, path)
}

/// Whether `name` sits under a `tests/fixtures` directory. Hard-coded, not
/// a configured setting: an `osf-expect` marker only takes effect here, so
/// a repository cannot use it to launder a real finding in an ordinary
/// file. The tool does not honour the marker anywhere else.
#[must_use]
pub fn is_fixture_path(name: &str) -> bool {
    let normalised = name.replace('\\', "/");
    normalised
        .split('/')
        .collect::<Vec<_>>()
        .windows(2)
        .any(|pair| pair == ["tests", "fixtures"])
}

/// Whether `id` names a scan rule: a leaked name, a session link, a local
/// path. Hard-coded, not a configured setting: no `osf-expect` marker may
/// declare one of these expected, so nothing inside the repository can
/// silence a scan finding by naming it in a fixture.
#[must_use]
pub fn is_scan_rule(id: &str) -> bool {
    id.starts_with("scan-")
}

/// Lints a text written for the given [`Context`]. With `fast_only`, only
/// the deterministic fast tier runs; the stop hook uses this, since it
/// must stay fast on every turn end. The command line runs every tier.
///
/// With `no_suppress`, every `osf-disable`-family marker is ignored, so
/// every finding it would have silenced is reported. Continuous integration
/// runs with this set.
#[must_use]
pub fn lint_writing(
    text: &str,
    known: &KnownNames,
    cfg: &WritingConfig,
    context: Context,
    fast_only: bool,
    no_suppress: bool,
) -> Vec<Finding> {
    let doc = osf_lint_core::segment::parse(text);
    let mut findings = Vec::new();
    // heading-in-short-text guards a short reply to a person: a chat
    // transcript or a commit message. A document and a skill file are
    // structured text that should have headings, so the rule is off there.
    if matches!(context, Context::Transcript | Context::Commit) {
        rules::headings_in_short_text(&doc, cfg, &mut findings);
    }
    rules::per_sentence(&doc, cfg, fast_only, &mut findings);
    rules::undefined_names(&doc, known, cfg, &mut findings);
    apply_context(&mut findings, context);
    let mut findings = if no_suppress {
        findings
    } else {
        osf_lint_core::apply_suppressions(text, findings, &rules::rule_ids())
    };
    osf_lint_core::sort_findings(&mut findings);
    add_explain_pointers(&mut findings);
    findings
}

/// Sets each finding's level and remediation from its rule's class and
/// group, resolved against `context`. A finding with no metadata (a
/// suppression-engine diagnostic, for instance) is left as its own level.
fn apply_context(findings: &mut [Finding], context: Context) {
    for f in findings.iter_mut() {
        if let Some(meta) = meta::rule_meta(f.rule) {
            let (level, remediation) = meta.resolve(context);
            f.level = level;
            f.remediation = remediation;
        }
    }
}

/// Points every finding at `osf explain <rule-id>`, so the reader can see
/// the full doc text: what the rule does, why it is bad, and its class.
fn add_explain_pointers(findings: &mut [Finding]) {
    for f in findings.iter_mut() {
        if meta::rule_meta(f.rule).is_some() {
            f.message = format!("{} (see `osf explain {}`)", f.message, f.rule);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint(text: &str) -> Vec<Finding> {
        lint_writing(
            text,
            &load_known_names(&[], None).expect("built-in names load"),
            &WritingConfig::default(),
            Context::Transcript,
            false,
            false,
        )
    }

    #[test]
    fn a_document_may_have_headings() {
        let known = load_known_names(&[], None).expect("built-in names load");
        assert!(lint_writing(
            "## Result\n\nIt passed.\n",
            &known,
            &WritingConfig::default(),
            Context::Document,
            false,
            false
        )
        .is_empty());
    }

    fn rules_of(text: &str) -> Vec<&'static str> {
        lint(text).into_iter().map(|f| f.rule).collect()
    }

    fn errors_of(text: &str) -> Vec<&'static str> {
        lint(text)
            .into_iter()
            .filter(|f| f.level == Level::Error)
            .map(|f| f.rule)
            .collect()
    }

    #[test]
    fn clean_text_has_no_findings() {
        let t = "The build failed on the test step. The fix is in [open-software-factory/software-factory#12 (the build fix)](https://example.com/12).\n";
        assert!(lint(t).is_empty(), "{:?}", lint(t));
    }

    #[test]
    fn bare_issue_reference() {
        assert_eq!(rules_of("Fixed in #125 today."), vec!["bare-reference"]);
        assert!(rules_of("Use `#12` in code.").is_empty());
    }

    #[test]
    fn reference_needs_a_label_and_a_link() {
        assert_eq!(
            rules_of("Fixed in open-software-factory/software-factory#125 today."),
            vec!["reference-without-label", "reference-without-link"]
        );
        assert_eq!(
            rules_of("Fixed in repo#125 (the canvas fixes) today."),
            vec!["reference-without-link"]
        );
        assert!(
            rules_of("Fixed in [repo#125 (the canvas fixes)](https://example.com) today.")
                .is_empty()
        );
    }

    /// A reference matched twice in one sentence, once inside a real
    /// Markdown link and once bare, must flag only the bare occurrence.
    #[test]
    fn a_second_bare_occurrence_of_a_linked_reference_is_still_flagged() {
        let t = "See [open-software-factory/software-factory#125 (the topic)](https://example.com) or open-software-factory/software-factory#125 (the topic) again.";
        let findings = lint(t);
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.rule == "reference-without-link")
                .count(),
            1,
            "{findings:?}"
        );
        assert!(
            findings.iter().all(|f| f.rule != "reference-without-label"),
            "{findings:?}"
        );
    }

    #[test]
    fn chat_local_phrases() {
        assert_eq!(rules_of("Do Phase 2 next."), vec!["chat-local-reference"]);
        assert_eq!(
            rules_of("As discussed, ship it."),
            vec!["chat-local-reference"]
        );
        assert!(rules_of("Round 2 found nothing.").is_empty());
    }

    #[test]
    fn undefined_name_needs_a_description() {
        assert_eq!(rules_of("Use Vale for this."), vec!["undefined-name"]);
        assert!(rules_of("Use Vale, a prose checker, for this.").is_empty());
        assert!(rules_of("Use Vale for this. Vale is a prose checker.").is_empty());
        assert!(rules_of("Use GitHub for this.").is_empty());
        assert!(rules_of("Run `Vale` for this.").is_empty());
    }

    #[test]
    fn multi_word_name_is_one_name() {
        let f = lint("Open Sublime Merge now.");
        assert_eq!(f.len(), 1);
        assert_eq!(f.first().map(|x| x.excerpt.as_str()), Some("Sublime Merge"));
    }

    #[test]
    fn long_sentence() {
        let t = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty one two three four five six.";
        assert_eq!(rules_of(t), vec!["long-sentence"]);
    }

    /// Change 1: a limit set in a config file must change what a rule
    /// reports, not just what `osf config show` prints.
    #[test]
    fn a_configured_sentence_limit_changes_what_long_sentence_reports() {
        let t = "one two three four five six seven eight nine ten.";
        let known = load_known_names(&[], None).expect("built-in names load");
        let default = lint_writing(
            t,
            &known,
            &WritingConfig::default(),
            Context::Transcript,
            false,
            false,
        );
        assert!(default.is_empty(), "{default:?}");
        let tight = WritingConfig {
            max_sentence_words: 5,
            ..WritingConfig::default()
        };
        let found = lint_writing(t, &known, &tight, Context::Transcript, false, false);
        assert_eq!(
            found.into_iter().map(|f| f.rule).collect::<Vec<_>>(),
            vec!["long-sentence"]
        );
    }

    #[test]
    fn dash_arrow_semicolon() {
        assert_eq!(rules_of("A thing — another thing."), vec!["em-dash"]);
        assert_eq!(
            rules_of("Input -> output."),
            vec!["arrow", "undefined-name-at-start"]
        );
        assert_eq!(rules_of("It ran; it passed."), vec!["semicolon"]);
    }

    #[test]
    fn filler_phrases() {
        assert_eq!(rules_of("Let me know if that helps."), vec!["filler"]);
        assert_eq!(rules_of("We should leverage the cache."), vec!["filler"]);
    }

    /// An empty `filler` list must turn the rule off, not make it match a
    /// zero-width span at nearly every word boundary.
    #[test]
    fn an_empty_filler_list_never_matches() {
        let known = load_known_names(&[], None).expect("built-in names load");
        let cfg = WritingConfig {
            filler: Vec::new(),
            ..WritingConfig::default()
        };
        let findings = lint_writing(
            "This is a perfectly clean sentence with no issues at all.",
            &known,
            &cfg,
            Context::Transcript,
            false,
            false,
        );
        assert!(findings.iter().all(|f| f.rule != "filler"), "{findings:?}");
    }

    /// The same guard for `chat_local_phrases` and `chat_local_labels`
    /// together: emptying both must turn `chat-local-reference` off for
    /// the word-list half of the rule, not flood every word boundary.
    #[test]
    fn empty_chat_local_lists_never_match_on_the_word_list_half() {
        let known = load_known_names(&[], None).expect("built-in names load");
        let cfg = WritingConfig {
            chat_local_phrases: Vec::new(),
            chat_local_labels: Vec::new(),
            ..WritingConfig::default()
        };
        let findings = lint_writing(
            "This is a perfectly clean sentence with no issues at all.",
            &known,
            &cfg,
            Context::Transcript,
            false,
            false,
        );
        assert!(
            findings.iter().all(|f| f.rule != "chat-local-reference"),
            "{findings:?}"
        );
    }

    #[test]
    fn numbers_in_prose() {
        assert_eq!(
            rules_of("It ran 12 axes over 3 rounds in 41 minutes."),
            vec!["numbers-in-prose"]
        );
    }

    /// Change 4: only a whole sentence in bold fires, not a long bold span
    /// inside an otherwise plain sentence.
    #[test]
    fn bold_sentence_only_fires_on_a_whole_bolded_sentence() {
        assert_eq!(
            rules_of("**Run the full suite before every release, without exception.**"),
            vec!["bold-sentence"]
        );
        assert!(rules_of("**Run the tests.**").is_empty());
        assert!(rules_of(
            "Run the tests before every release, but only **the smoke suite** needs a rerun."
        )
        .is_empty());
    }

    #[test]
    fn headings_in_short_text() {
        assert_eq!(
            rules_of("## Result\n\nIt passed.\n"),
            vec!["heading-in-short-text"]
        );
    }

    #[test]
    fn code_and_comments_are_skipped() {
        let t = "```\nUse Vale -> now; Phase 2 #1\n```\n<!-- Phase 2 -->\n";
        assert!(lint(t).is_empty(), "{:?}", lint(t));
    }

    #[test]
    fn a_processor_architecture_a_developer_already_knows_needs_no_description() {
        // Real case: a pull request table of processor architectures flagged
        // Intel, Arm and Apple Silicon as undefined names.
        assert!(rules_of("The build runs on Intel and Arm.").is_empty());
        assert!(rules_of("Apple Silicon runs the same binary.").is_empty());
    }

    #[test]
    fn front_matter_is_never_read_as_prose() {
        // A skill file's front matter, the shape that first surfaced this bug.
        let t = "---\nTitle: Demo\nPlatform: Windows\n---\n\nIt ran on Windows.\n";
        assert!(lint(t).is_empty(), "{:?}", lint(t));
    }

    #[test]
    fn a_heading_word_that_is_ordinary_elsewhere_is_not_a_name() {
        // Real example from the false-positive analysis, docs/architecture/open-questions.md:1.
        let filler = "It ran. ".repeat(300);
        let t = format!(
            "# Open Questions\n\n{filler}This document lists open questions about the design.\n"
        );
        assert_eq!(errors_of(&t), Vec::<&str>::new(), "{:?}", lint(&t));
        assert!(
            !rules_of(&t).contains(&"undefined-name-at-start"),
            "{:?}",
            lint(&t)
        );
    }

    #[test]
    fn a_genuine_title_in_a_heading_is_still_reported() {
        // Real example from the false-positive analysis, docs/research/ux/ux-references.md:147.
        let filler = "It ran. ".repeat(300);
        let t = format!("{filler}\n\n### RimWorld\n");
        assert_eq!(errors_of(&t), vec!["undefined-name"], "{:?}", lint(&t));
    }

    #[test]
    fn a_multi_word_name_in_a_list_item_is_not_torn_apart() {
        // Real example from the false-positive analysis,
        // docs/research/build-runners-and-compute.md:11: "Alibaba Cloud" was
        // fragmented into a lone, meaningless "Alibaba" because "cloud" is an
        // ordinary word used lowercase on the same line.
        let t = "- cloud providers such as Alibaba Cloud;\n";
        assert_eq!(errors_of(t), vec!["undefined-name"], "{:?}", lint(t));
        let found = lint(t);
        let excerpt = found.first().map(|f| f.excerpt.as_str());
        assert_eq!(excerpt, Some("Alibaba Cloud"), "{found:?}");
    }

    #[test]
    fn a_multi_word_name_ending_in_an_acronym_plural_is_not_torn_apart() {
        // Real example from the false-positive analysis,
        // docs/research/ahp-acp-architecture-direction.md:296: "JetBrains
        // IDEs" was fragmented into a lone "JetBrains" because "IDEs" alone
        // is an acronym's plural.
        let t = "- JetBrains IDEs include a built-in client.\n";
        let found = lint(t);
        let excerpt = found
            .iter()
            .find(|f| f.level == Level::Error)
            .map(|f| f.excerpt.as_str());
        assert_eq!(excerpt, Some("JetBrains IDEs"), "{found:?}");
    }

    #[test]
    fn a_hyphen_compound_headed_by_an_ordinary_word_is_not_a_name() {
        // Real example from the false-positive analysis,
        // docs/architecture/decisions/0001-go-for-the-factory-engine.md:13.
        let t = "The service links to Go-specific tooling. It also runs a go binary directly.\n";
        assert!(rules_of(t).is_empty(), "{:?}", rules_of(t));
    }

    #[test]
    fn a_real_name_compound_is_still_reported() {
        // Real example from the false-positive analysis, docs/product/ux/open-questions.md:28.
        let t = "The team is choosing between React/TypeScript for the client.\n";
        assert_eq!(errors_of(t), vec!["undefined-name"]);
    }

    #[test]
    fn a_list_item_word_that_is_ordinary_elsewhere_is_not_a_name() {
        // Real example from the false-positive analysis, docs/architecture/open-questions.md:17.
        let t = "- What is the durable unit: WorkItem, Run, Execution, Task, Step, Attempt, Session?\n\nA run of the pipeline records each attempt and session in a task queue.\n";
        let found = lint(t);
        let names: Vec<&str> = found
            .iter()
            .filter(|f| f.rule == "undefined-name")
            .map(|f| f.excerpt.as_str())
            .collect();
        assert_eq!(names, vec!["WorkItem", "Execution"], "{found:?}");
    }

    #[test]
    fn an_acronym_plural_is_not_a_name() {
        // Real example from the false-positive analysis,
        // docs/architecture/decisions/0004-protocol-independent-core-with-ahp-acp-edges.md:16.
        let t = "The team should treat factory-owned UIs as AHP clients.";
        assert!(rules_of(t).is_empty(), "{:?}", rules_of(t));
    }

    #[test]
    fn table_cells_check_names_but_not_length() {
        let long = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty one two three four five six";
        let t = format!("| Tool | Note |\n|---|---|\n| prose | Use Vale for {long} |\n");
        assert_eq!(errors_of(&t), vec!["undefined-name"]);
    }

    #[test]
    fn table_only_name_is_reported_the_factory_engine_example() {
        // Real example from the false-positive analysis, docs/architecture/decisions/README.md:9.
        let t = "| A | B |\n|---|---|\n| x | The Factory Engine starts in Go. |\n";
        assert_eq!(rules_of(t), vec!["undefined-name-at-start"]);
    }

    #[test]
    fn a_name_also_in_prose_is_not_reported_from_the_table() {
        let t = "| A | B |\n|---|---|\n| x | The Factory Engine starts in Go. |\n\nThe Factory Engine has no upstream dependency.\n";
        let f = lint(t);
        let hits: Vec<_> = f
            .iter()
            .filter(|x| x.excerpt == "The Factory Engine")
            .collect();
        assert_eq!(hits.len(), 1, "{f:?}");
        let hit = hits.first().expect("one hit checked above");
        assert_eq!(hit.line, 5, "reported from prose, not the table");
    }

    #[test]
    fn a_name_that_starts_a_sentence_is_a_warning() {
        // "Vale runs fast." and "Build runs fast." parse the same, so this only warns.
        assert_eq!(rules_of("Vale runs fast."), vec!["undefined-name-at-start"]);
        assert!(rules_of("Build runs fast.").is_empty());
        assert!(rules_of("Fixing runs fast.").is_empty());
        assert!(errors_of("🤖 Generated with care.").is_empty());
    }

    #[test]
    fn a_word_spelled_lowercase_elsewhere_is_not_a_name() {
        // Real example from the false-positive analysis, AGENTS.md:9.
        let t = "Deterministic checks are authoritative gates. LLM judgments may augment them but should not replace deterministic verification when deterministic tooling exists.";
        assert!(rules_of(t).is_empty(), "{:?}", rules_of(t));
    }

    #[test]
    fn a_name_never_spelled_lowercase_still_warns_at_start() {
        assert_eq!(
            rules_of("Vale runs fast. Vale never appears lowercase."),
            vec!["undefined-name-at-start"]
        );
    }

    #[test]
    fn heading_words_are_not_name_candidates() {
        let long = format!("## Result\n\n{}\n", "It ran. ".repeat(300));
        assert!(rules_of(&long).is_empty(), "{:?}", rules_of(&long));
    }

    #[test]
    fn line_numbers_point_at_the_sentence() {
        let f: Vec<Finding> = lint("Fine.\n\nAlso fine.\nUse #9 here.\n")
            .into_iter()
            .filter(|f| f.level == Level::Error)
            .collect();
        assert_eq!(f.len(), 1);
        assert_eq!(f.first().map(|x| x.line), Some(4));
    }

    #[test]
    fn a_suppressed_line_is_kept_but_marked() {
        let f = lint("Fixed in #125 today. <!-- osf-disable-line bare-reference -- tracked -->\n");
        // The marker's own `-->` and ` -- ` must not lint as an arrow or an em dash.
        assert_eq!(f.len(), 1, "{f:?}");
        let bare = f
            .iter()
            .find(|x| x.rule == "bare-reference")
            .expect("finding kept");
        assert!(bare.suppressed.is_some());
    }

    #[test]
    fn no_suppress_ignores_every_marker() {
        let known = load_known_names(&[], None).expect("built-in names load");
        let text = "Fixed in #125 today. <!-- osf-disable-line bare-reference -- tracked -->\n";
        let f = lint_writing(
            text,
            &known,
            &WritingConfig::default(),
            Context::Transcript,
            false,
            true,
        );
        let bare = f
            .iter()
            .find(|x| x.rule == "bare-reference")
            .expect("finding kept");
        assert!(bare.suppressed.is_none());
    }

    fn find_in(context: Context, text: &str, rule: &str) -> Finding {
        let known = load_known_names(&[], None).expect("built-in names load");
        lint_writing(
            text,
            &known,
            &WritingConfig::default(),
            context,
            false,
            false,
        )
        .into_iter()
        .find(|f| f.rule == rule)
        .unwrap_or_else(|| panic!("{rule} did not fire on {text:?} in {context:?}"))
    }

    /// Change 3: a comprehension rule (bare-reference) resolves to the
    /// matrix's level and remediation in every context.
    #[test]
    fn a_comprehension_rule_resolves_per_context() {
        let text = "Fixed in #125 today.";
        let cases = [
            (Context::Transcript, Level::Error, Remediation::Clarify),
            (Context::Commit, Level::Error, Remediation::Rewrite),
            (Context::Document, Level::Error, Remediation::Rewrite),
            (Context::Skill, Level::Error, Remediation::Rewrite),
        ];
        for (context, level, remediation) in cases {
            let f = find_in(context, text, "bare-reference");
            assert_eq!(f.level, level, "{context:?}");
            assert_eq!(f.remediation, remediation, "{context:?}");
        }
    }

    /// Change 3: a style rule (em-dash) resolves to the matrix's level and
    /// remediation in every context, only blocking outside a transcript.
    #[test]
    fn a_style_rule_resolves_per_context() {
        let text = "A thing — another thing.";
        let cases = [
            (Context::Transcript, Level::Warning, Remediation::Advise),
            (Context::Commit, Level::Error, Remediation::Rewrite),
            (Context::Document, Level::Warning, Remediation::Rewrite),
            (Context::Skill, Level::Error, Remediation::Rewrite),
        ];
        for (context, level, remediation) in cases {
            let f = find_in(context, text, "em-dash");
            assert_eq!(f.level, level, "{context:?}");
            assert_eq!(f.remediation, remediation, "{context:?}");
        }
    }

    /// Change 3: heading-in-short-text is off in a document.
    #[test]
    fn heading_in_short_text_is_off_in_a_document() {
        let known = load_known_names(&[], None).expect("built-in names load");
        let f = lint_writing(
            "## Result\n\nIt passed.\n",
            &known,
            &WritingConfig::default(),
            Context::Document,
            false,
            false,
        );
        assert!(f.iter().all(|x| x.rule != "heading-in-short-text"), "{f:?}");
    }

    /// A skill file is structured text, expected to have headings, same as
    /// a document; the rule must not fire there either.
    #[test]
    fn heading_in_short_text_is_off_in_a_skill_file() {
        let known = load_known_names(&[], None).expect("built-in names load");
        let f = lint_writing(
            "## Result\n\nIt passed.\n",
            &known,
            &WritingConfig::default(),
            Context::Skill,
            false,
            false,
        );
        assert!(f.iter().all(|x| x.rule != "heading-in-short-text"), "{f:?}");
    }

    /// Change 3: reference-without-link is pinned to warning even where the
    /// matrix would otherwise make a style rule an error.
    #[test]
    fn reference_without_link_is_always_a_warning() {
        let f = find_in(
            Context::Commit,
            "Fixed in repo#125 (the canvas fixes) today.",
            "reference-without-link",
        );
        assert_eq!(f.level, Level::Warning);
    }

    /// Change 3: every finding points at `osf explain <rule-id>`.
    #[test]
    fn every_finding_points_at_explain() {
        let f = find_in(
            Context::Transcript,
            "Fixed in #125 today.",
            "bare-reference",
        );
        assert!(
            f.message.contains("osf explain bare-reference"),
            "{}",
            f.message
        );
    }

    #[test]
    fn a_tests_fixtures_path_is_recognised_either_separator() {
        assert!(is_fixture_path(
            "crates/osf/tests/fixtures/skills/bad/SKILL.md"
        ));
        assert!(is_fixture_path(
            r"crates\osf\tests\fixtures\skills\bad\SKILL.md"
        ));
    }

    #[test]
    fn a_path_that_only_mentions_fixtures_or_tests_is_not_a_fixture_path() {
        assert!(!is_fixture_path("docs/real.md"));
        assert!(!is_fixture_path("greatest-fixtures-ever/tests/file.md"));
        assert!(!is_fixture_path("tests/README.md"));
    }

    #[test]
    fn a_scan_prefixed_id_is_a_scan_rule() {
        assert!(is_scan_rule("scan-denied-name"));
        assert!(is_scan_rule("scan-session-link"));
    }

    #[test]
    fn an_ordinary_id_is_not_a_scan_rule() {
        assert!(!is_scan_rule("arrow"));
        assert!(!is_scan_rule("bare-reference"));
    }
}
