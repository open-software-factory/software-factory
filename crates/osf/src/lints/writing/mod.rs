//! Writing lint: deterministic checks on prose written for people.
//!
//! Text is split into paragraphs and sentences. Fenced code, tables, HTML
//! comments, inline code and a `---`-delimited front-matter block are never
//! checked. Every rule has an id, a level and a one-line message that names
//! the offending text.

pub(super) mod meta;
pub(super) mod names;
pub(super) mod reference;
pub(super) mod rules;

use super::{Context, Finding, KnownNames};
use crate::config::WritingConfig;
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
    rules::unplaceable_reference(&doc, known, cfg, context, &mut findings);
    rules::recap_ending(&doc, cfg, &mut findings);
    apply_context(&mut findings, context);
    let mut findings = if no_suppress {
        findings
    } else {
        osf_lint_core::apply_suppressions(
            text,
            findings,
            &rules::rule_ids(),
            super::RETIRED_RULE_IDS,
        )
    };
    osf_lint_core::sort_findings(&mut findings);
    add_explain_pointers(&mut findings);
    findings
}

/// Sets each finding's level and remediation from its rule's class and
/// group, resolved against `context` and the finding's own evidence. A
/// finding with no metadata (a suppression-engine diagnostic, for
/// instance) is left as its own level.
fn apply_context(findings: &mut [Finding], context: Context) {
    for f in findings.iter_mut() {
        if let Some(meta) = meta::rule_meta(f.rule) {
            let (level, remediation) = meta.resolve(context, f.evidence);
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
    use crate::lints::{
        is_fixture_path, is_scan_rule, load_known_names, Evidence, Level, Remediation,
    };

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
        assert_eq!(
            rules_of("Fixed in #125 today."),
            vec!["unplaceable-reference"]
        );
        assert!(rules_of("Use `#12` in code.").is_empty());
    }

    #[test]
    fn reference_needs_a_bracketed_description_or_a_link() {
        assert_eq!(
            rules_of("Fixed in open-software-factory/software-factory#125 today."),
            vec!["unplaceable-reference"]
        );
        assert!(rules_of("Fixed in repo#125 (the canvas fixes) today.").is_empty());
        assert!(
            rules_of("Fixed in [repo#125 (the canvas fixes)](https://example.com) today.")
                .is_empty()
        );
    }

    /// A repo-qualified number is judged per occurrence, not deduped like a
    /// name: a placed mention earlier in the paragraph does not excuse a
    /// bare one later.
    #[test]
    fn a_second_bare_occurrence_of_a_placed_reference_is_still_flagged() {
        let t = "See [open-software-factory/software-factory#125 (the topic)](https://example.com) or open-software-factory/software-factory#125 again.";
        let findings = lint(t);
        let hits: Vec<&str> = findings
            .iter()
            .filter(|f| f.rule == "unplaceable-reference")
            .map(|f| f.excerpt.as_str())
            .collect();
        assert_eq!(
            hits,
            vec!["open-software-factory/software-factory#125"],
            "{findings:?}"
        );
    }

    #[test]
    fn a_configured_chat_local_phrase_is_flagged() {
        assert_eq!(
            rules_of("As discussed, ship it."),
            vec!["unplaceable-reference"]
        );
    }

    /// `must_explain_names` is the repository's curated tier-1 list: a name
    /// on it is an error unless the text explains it, whether or not the
    /// name itself looks like a name by any other evidence.
    fn lint_with(cfg: &WritingConfig, text: &str) -> Vec<Finding> {
        let known = load_known_names(&[], None).expect("built-in names load");
        lint_writing(text, &known, cfg, Context::Transcript, false, false)
    }

    #[test]
    fn a_curated_must_explain_name_needs_a_description() {
        let cfg = WritingConfig {
            must_explain_names: vec!["Vale".to_string()],
            ..WritingConfig::default()
        };
        assert_eq!(
            lint_with(&cfg, "Use Vale for this.")
                .into_iter()
                .map(|f| f.rule)
                .collect::<Vec<_>>(),
            vec!["unplaceable-reference"]
        );
        assert!(lint_with(&cfg, "Use Vale, a prose checker, for this.").is_empty());
        assert!(lint_with(&cfg, "Use Vale for this. Vale is a prose checker.").is_empty());
        assert!(lint_with(&cfg, "Use GitHub for this.").is_empty());
        assert!(lint_with(&cfg, "Run `Vale` for this.").is_empty());
    }

    /// A plain word with no evidence at all, and not on the must-explain
    /// list, is never reported: a bare capital letter proves nothing.
    #[test]
    fn an_ordinary_capitalised_word_with_no_evidence_is_never_reported() {
        assert!(rules_of("Setup is about fifteen minutes and it happens once.").is_empty());
    }

    /// A multi-word run stays one candidate, not two, and a run repeated
    /// more than once in the document is itself evidence of a name.
    #[test]
    fn multi_word_name_is_one_name() {
        let f = lint("Open Sublime Merge now. Then open Sublime Merge again.");
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f.iter().all(|x| x.excerpt == "Sublime Merge"), "{f:?}");
        assert!(
            f.iter().all(|x| x.evidence == Evidence::Statistical),
            "{f:?}"
        );
    }

    #[test]
    fn tier_2_evidence_internal_capital() {
        assert_eq!(
            rules_of("Deploy with DuckDB today."),
            vec!["unplaceable-reference"]
        );
    }

    #[test]
    fn tier_2_evidence_digit_in_token() {
        assert_eq!(
            rules_of("The build uses Log4j for output."),
            vec!["unplaceable-reference"]
        );
    }

    #[test]
    fn tier_2_evidence_domain_suffix() {
        assert_eq!(
            rules_of("Check Contentful.io for the docs."),
            vec!["unplaceable-reference"]
        );
    }

    /// A single word repeating is not evidence the way a multi-word run
    /// repeating is: "Setup" and "Developer" repeat in ordinary transcripts
    /// too, so only a multi-word run's repetition counts.
    #[test]
    fn a_repeated_single_word_capital_is_still_not_evidence_of_a_name() {
        assert!(rules_of("Vale runs fast. Vale never appears lowercase.").is_empty());
    }

    /// A must-explain finding is deterministic, and a name reported only on
    /// weak evidence is statistical: a policy that refuses to gate on
    /// statistical evidence must be able to rely on this, rule id alone is
    /// not enough to tell the two apart.
    #[test]
    fn a_must_explain_finding_is_deterministic_and_an_evidenced_one_is_statistical() {
        let cfg = WritingConfig {
            must_explain_names: vec!["Fastfix".to_string()],
            ..WritingConfig::default()
        };
        let f = lint_with(&cfg, "Use Fastfix for this. Then run DuckDB once.");
        let tier1 = f
            .iter()
            .find(|x| x.excerpt == "Fastfix" && x.rule == "unplaceable-reference")
            .expect("the curated name is reported");
        assert_eq!(tier1.level, Level::Error);
        assert_eq!(tier1.remediation, Remediation::Clarify);
        assert_eq!(tier1.evidence, Evidence::Deterministic);
        let tier2 = f
            .iter()
            .find(|x| x.excerpt == "DuckDB" && x.rule == "unplaceable-reference")
            .expect("the evidenced name is reported");
        assert_eq!(tier2.level, Level::Warning);
        assert_eq!(tier2.remediation, Remediation::Advise);
        assert_eq!(tier2.evidence, Evidence::Statistical);
    }

    /// A contraction such as "I'll" or "Don't" is never a name, even though
    /// it starts uppercase and has a lowercase tail.
    #[test]
    fn contraction_is_not_a_name() {
        assert!(
            rules_of("If you want the clean version anyway, say so and I'll do it.").is_empty()
        );
        assert!(rules_of("We'll ship it today.").is_empty());
        assert!(rules_of("Don't skip the test.").is_empty());
    }

    /// A run that opens with a known name, such as the built-in "GitHub",
    /// stays known even when the word after it is not itself in the list.
    #[test]
    fn known_name_heads_an_unknown_run() {
        let excerpts: Vec<String> = lint("Open the settings, then GitHub Apps.")
            .into_iter()
            .map(|f| f.excerpt)
            .collect();
        assert!(
            !excerpts.contains(&"GitHub Apps".to_string()),
            "{excerpts:?}"
        );
        let excerpts: Vec<String> = lint("A GitHub App shows as a bot.")
            .into_iter()
            .map(|f| f.excerpt)
            .collect();
        assert!(
            !excerpts.contains(&"GitHub App".to_string()),
            "{excerpts:?}"
        );
    }

    #[test]
    fn long_sentence() {
        let t = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty one two three four five six.";
        assert_eq!(rules_of(t), vec!["long-sentence"]);
    }

    /// A limit set in a config file must change what a rule reports, not
    /// just what `osf config show` prints.
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
        assert_eq!(rules_of("Input -> output."), vec!["arrow"]);
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

    /// Emptying `chat_local_phrases` must turn off the configured phrase
    /// list, not flood every word boundary the way a genuinely empty regex
    /// alternation would.
    #[test]
    fn an_empty_chat_local_phrases_list_never_matches() {
        let known = load_known_names(&[], None).expect("built-in names load");
        let cfg = WritingConfig {
            chat_local_phrases: Vec::new(),
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
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn numbers_in_prose() {
        assert_eq!(
            rules_of("It ran 12 axes over 3 rounds in 41 minutes."),
            vec!["numbers-in-prose", "unplaceable-reference"]
        );
    }

    /// Only a whole sentence in bold fires, not a long bold span inside an
    /// otherwise plain sentence.
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
            !rules_of(&t).contains(&"unplaceable-reference"),
            "{:?}",
            lint(&t)
        );
    }

    #[test]
    fn a_genuine_title_in_a_heading_is_still_reported() {
        // Real example from the false-positive analysis, docs/research/ux/ux-references.md:147.
        // "RimWorld" carries its own evidence, an internal capital, so it
        // is reported regardless of the heading it sits in; the internal
        // capital only earns it a warning, not the error the older design
        // gave every unrecognised capitalised heading word.
        let filler = "It ran. ".repeat(300);
        let t = format!("{filler}\n\n### RimWorld\n");
        let f = lint(&t);
        assert_eq!(f.len(), 1, "{f:?}");
        let hit = f.first().expect("one finding checked above");
        assert_eq!(hit.excerpt, "RimWorld");
        assert_eq!(hit.rule, "unplaceable-reference");
        assert_eq!(hit.level, Level::Warning);
        assert_eq!(hit.remediation, Remediation::Advise);
        assert_eq!(hit.evidence, Evidence::Statistical);
    }

    #[test]
    fn a_multi_word_name_in_a_list_item_is_not_torn_apart() {
        // Real example from the false-positive analysis,
        // docs/research/build-runners-and-compute.md:11: "Alibaba Cloud" was
        // once fragmented into a lone, meaningless "Alibaba". The run is
        // still joined whole here; a second mention supplies the repeated-
        // run evidence a single mention of two ordinary-cased words no
        // longer earns on its own.
        let t =
            "- cloud providers such as Alibaba Cloud, including Alibaba Cloud in every region;\n";
        let found = lint(t);
        let excerpt = found.first().map(|f| f.excerpt.as_str());
        assert_eq!(excerpt, Some("Alibaba Cloud"), "{found:?}");
    }

    #[test]
    fn a_multi_word_name_ending_in_an_acronym_plural_is_not_torn_apart() {
        // Real example from the false-positive analysis,
        // docs/research/ahp-acp-architecture-direction.md:296: "JetBrains
        // IDEs" was once fragmented into a lone "JetBrains" because "IDEs"
        // alone is an acronym's plural. "JetBrains" carries its own
        // internal-capital evidence, so the whole run is still reported,
        // as a warning rather than the older design's error.
        let t = "- JetBrains IDEs include a built-in client.\n";
        let found = lint(t);
        let excerpt = found.first().map(|f| f.excerpt.as_str());
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
        // TypeScript's internal capital carries the whole compound, so it
        // is still reported, as a warning from that evidence alone.
        let t = "The team is choosing between React/TypeScript for the client.\n";
        let f = lint(t);
        assert_eq!(f.len(), 1, "{f:?}");
        let hit = f.first().expect("one finding checked above");
        assert_eq!(hit.excerpt, "React/TypeScript");
        assert_eq!(hit.rule, "unplaceable-reference");
        assert_eq!(hit.level, Level::Warning);
        assert_eq!(hit.remediation, Remediation::Advise);
    }

    #[test]
    fn a_list_item_word_that_is_ordinary_elsewhere_is_not_a_name() {
        // Real example from the false-positive analysis, docs/architecture/open-questions.md:17.
        // Only "WorkItem" carries evidence, an internal capital; "Run",
        // "Execution", "Task", "Attempt" and "Session" are ordinary
        // capitalised words with none, so they are not reported at all,
        // whether or not they also happen to appear lowercase elsewhere.
        let t = "- What is the durable unit: WorkItem, Run, Execution, Task, Step, Attempt, Session?\n\nA run of the pipeline records each attempt and session in a task queue.\n";
        let found = lint(t);
        let names: Vec<&str> = found.iter().map(|f| f.excerpt.as_str()).collect();
        assert_eq!(names, vec!["WorkItem"], "{found:?}");
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
        let t = format!("| Tool | Note |\n|---|---|\n| prose | Use DuckDB for {long} |\n");
        let f = lint(&t);
        assert_eq!(
            f.iter().map(|x| x.rule).collect::<Vec<_>>(),
            vec!["unplaceable-reference"],
            "{f:?}"
        );
    }

    #[test]
    fn table_only_name_is_reported_the_factory_engine_example() {
        // A name seen only in a table is still reported, from its first appearance.
        let t = "| A | B |\n|---|---|\n| x | The Factory Engine starts in Go. |\n| y | The Factory Engine also builds. |\n";
        let f = lint(t);
        assert_eq!(rules_of(t), vec!["unplaceable-reference"]);
        assert_eq!(
            f.first().map(|x| x.excerpt.as_str()),
            Some("Factory Engine")
        );
    }

    #[test]
    fn a_name_also_in_prose_is_not_reported_from_the_table() {
        // A name also used in prose is reported there, not from the table.
        let t = "| A | B |\n|---|---|\n| x | The Factory Engine starts in Go. |\n\nThe Factory Engine has no upstream dependency.\n";
        let f = lint(t);
        let hits: Vec<_> = f.iter().filter(|x| x.excerpt == "Factory Engine").collect();
        assert_eq!(hits.len(), 1, "{f:?}");
        let hit = hits.first().expect("one hit checked above");
        assert_eq!(hit.line, 5, "reported from prose, not the table");
    }

    #[test]
    fn a_name_that_starts_a_sentence_is_still_judged_on_evidence() {
        // "DuckDB runs fast." and "Build runs fast." are told apart by
        // DuckDB's internal capital now, not by which word opens the
        // sentence: position decides nothing in the new design.
        assert_eq!(rules_of("DuckDB runs fast."), vec!["unplaceable-reference"]);
        assert!(rules_of("Build runs fast.").is_empty());
        assert!(rules_of("Fixing runs fast.").is_empty());
        assert!(rules_of("🤖 Generated with care.").is_empty());
    }

    #[test]
    fn a_word_spelled_lowercase_elsewhere_is_not_a_name() {
        // Real example from the false-positive analysis, AGENTS.md:9.
        let t = "Deterministic checks are authoritative gates. LLM judgments may augment them but should not replace deterministic verification when deterministic tooling exists.";
        assert!(rules_of(t).is_empty(), "{:?}", rules_of(t));
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
        assert_eq!(f.len(), 1, "{f:?}");
        let paragraph_scope = f
            .iter()
            .find(|x| x.rule == "unplaceable-reference")
            .expect("unplaceable-reference kept");
        // A paragraph-scope rule reports the paragraph's own start line, the
        // line the paragraph itself opened on, not the sentence's line.
        assert_eq!(paragraph_scope.line, 3);
    }

    #[test]
    fn a_suppressed_line_is_kept_but_marked() {
        let f = lint(
            "Fixed in #125 today. <!-- osf-disable-line unplaceable-reference -- tracked -->\n",
        );
        // The marker's own `-->` and ` -- ` must not lint as an arrow or an em dash.
        assert_eq!(f.len(), 1, "{f:?}");
        let unplaceable = f
            .iter()
            .find(|x| x.rule == "unplaceable-reference")
            .expect("finding kept");
        assert!(unplaceable.suppressed.is_some());
    }

    #[test]
    fn no_suppress_ignores_every_marker() {
        let known = load_known_names(&[], None).expect("built-in names load");
        let text =
            "Fixed in #125 today. <!-- osf-disable-line unplaceable-reference -- tracked -->\n";
        let f = lint_writing(
            text,
            &known,
            &WritingConfig::default(),
            Context::Transcript,
            false,
            true,
        );
        let unplaceable = f
            .iter()
            .find(|x| x.rule == "unplaceable-reference")
            .expect("finding kept");
        assert!(unplaceable.suppressed.is_none());
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

    /// A comprehension rule resolves to the level-and-remediation matrix in every context.
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
            let f = find_in(context, text, "unplaceable-reference");
            assert_eq!(f.level, level, "{context:?}");
            assert_eq!(f.remediation, remediation, "{context:?}");
        }
    }

    /// A style rule blocks outside a transcript but only warns inside one.
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

    /// heading-in-short-text never fires in a document, only in a short reply.
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

    /// Every finding's message points at its own `osf explain <rule-id>`.
    #[test]
    fn every_finding_points_at_explain() {
        let f = find_in(
            Context::Transcript,
            "Fixed in #125 today.",
            "unplaceable-reference",
        );
        assert!(
            f.message.contains("osf explain unplaceable-reference"),
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
        assert!(!is_scan_rule("unplaceable-reference"));
    }

    #[test]
    fn a_document_title_heading_is_not_a_name_introduction() {
        // False positive 1: a document's own top-level heading, a generic
        // descriptive title, was reported as introducing an undefined name.
        let filler = "It ran. ".repeat(300);
        let t = format!("# Individual Contributor License Agreement\n\n{filler}\n");
        let names: Vec<&str> = rules_of(&t)
            .into_iter()
            .filter(|r| *r == "unplaceable-reference")
            .collect();
        assert!(names.is_empty(), "{:?}", lint(&t));
    }

    #[test]
    fn a_heading_with_evidence_is_still_reported() {
        // A name introduced in a heading is reported again when prose repeats it.
        let filler = "It ran. ".repeat(300);
        let t = format!(
            "# Notes\n\n{filler}\n\n### Prison Architect\n\nStudy Prison Architect's spatial systems.\n"
        );
        let found = lint(&t);
        let hit = found
            .iter()
            .find(|f| f.excerpt == "Prison Architect")
            .unwrap_or_else(|| panic!("Prison Architect not reported: {found:?}"));
        assert_eq!(hit.rule, "unplaceable-reference");
        assert_eq!(hit.level, Level::Warning);
    }

    #[test]
    fn a_table_header_row_label_is_not_a_sentence_start() {
        // False positive 2: a table header row's short column labels, "Ran
        // before" and "Runs now", were flagged as if a sentence began there.
        let t = "| Check | Ran before | Runs now |\n|---|---|---|\n| Build | Yes | Yes |\n";
        assert!(rules_of(t).is_empty(), "{:?}", rules_of(t));
    }

    #[test]
    fn a_table_data_row_naming_a_curated_tool_is_still_reported() {
        // Only the header row is a column label; a body row is prose about
        // one specific entry, so a short cell there naming a curated,
        // unexplained tool still needs its own explanation, the same as a
        // short table header must not.
        let cfg = WritingConfig {
            must_explain_names: vec!["Canny".to_string()],
            ..WritingConfig::default()
        };
        let t = "| Tool | Category |\n|---|---|\n| Canny | feedback tool |\n";
        let f = lint_with(&cfg, t);
        assert_eq!(
            f.iter().map(|x| x.rule).collect::<Vec<_>>(),
            vec!["unplaceable-reference"],
            "{f:?}"
        );
    }

    #[test]
    fn a_full_sentence_in_a_table_cell_with_no_evidence_is_not_reported() {
        // A table cell can hold real prose, not just a short label; the
        // bare capital that opens a sentence there is not evidence of a
        // name any more than it is in running prose.
        let t =
            "| Command | Notes |\n|---|---|\n| x | Reads standard input when no file is given. |\n";
        assert!(rules_of(t).is_empty(), "{:?}", lint(t));
    }

    #[test]
    fn neither_is_a_common_sentence_starter() {
        // False positive 3: an ordinary English word capitalised only
        // because it opens a sentence.
        let t =
            "The two options were a flag and a switch. Neither can be wired up to get less code.";
        assert!(rules_of(t).is_empty(), "{:?}", rules_of(t));
    }

    #[test]
    fn a_legal_term_glossed_by_a_parenthetical_in_the_same_sentence_is_not_flagged() {
        // False positive 4: a capitalised defined term inside a legal
        // document, glossed in place by the parenthetical straight after it.
        let t = "This is a contributor agreement.\n\nContributions (present and future) that you submit to the project are licensed under the terms below.\n";
        assert!(rules_of(t).is_empty(), "{:?}", rules_of(t));
    }

    #[test]
    fn a_document_title_stays_clean_even_when_its_own_words_recur_in_the_introduction() {
        // A real legal document restates its own title, in full, in its own
        // opening paragraph. That recurrence must not count as evidence
        // that the title's ordinary words are a name: it only proves the
        // title is echoed, which almost any document's introduction does.
        let filler = "It ran. ".repeat(300);
        let t = format!(
            "# Individual Contributor License Agreement\n\n{filler}\n\nThis file reproduces the Foundation's Individual Contributor License Agreement, version 2 (\"the ICLA\").\n"
        );
        // "version 2" is its own, unrelated number candidate; only the
        // title's own recurring words are this test's concern.
        let hits = lint(&t);
        let title_hits: Vec<&Finding> = hits.iter().filter(|f| f.excerpt != "version 2").collect();
        assert!(title_hits.is_empty(), "{hits:?}");
    }

    #[test]
    fn a_merged_run_is_still_described_despite_a_possessive_breaking_it() {
        // A run's joined name is its words with a single space between
        // them; it never reappears character for character once a
        // possessive breaks it in the source, as it does here between
        // "Foundation" and "Individual". Anchoring on the run's first word
        // still finds the parenthetical explaining it. "version 2" is its
        // own, unrelated number candidate.
        let t = "This reproduces the Apache Software Foundation's Individual Contributor project agreement, version 2 (\"the ICA\").";
        let f = lint(t);
        let title_hits: Vec<&Finding> = f.iter().filter(|x| x.excerpt != "version 2").collect();
        assert!(title_hits.is_empty(), "{f:?}");
    }

    /// A curated name is the one case still worth testing this against:
    /// the known bug was that any colon in the following sentence used to
    /// count as an explanation, even one with nothing to do with the name.
    #[test]
    fn described_in_the_next_sentence_must_actually_mention_the_name() {
        let cfg = WritingConfig {
            must_explain_names: vec!["Fastfix".to_string()],
            ..WritingConfig::default()
        };
        let t = "Use Fastfix for this. Run it: `fastfix build`.";
        assert_eq!(
            lint_with(&cfg, t)
                .into_iter()
                .map(|f| f.rule)
                .collect::<Vec<_>>(),
            vec!["unplaceable-reference"]
        );
    }

    #[test]
    fn described_in_the_next_sentence_still_works_when_it_names_the_word() {
        // The fix must not stop the legitimate case: a colon that follows
        // the name itself, right there in the next sentence, still counts.
        let cfg = WritingConfig {
            must_explain_names: vec!["Fastfix".to_string()],
            ..WritingConfig::default()
        };
        let t = "Use Fastfix for this. Fastfix means a build helper for fixtures.";
        assert!(lint_with(&cfg, t).is_empty());
    }
}
