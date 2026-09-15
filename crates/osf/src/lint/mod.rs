//! Writing lint: deterministic checks on prose written for people.
//!
//! Text is split into paragraphs and sentences. Fenced code, tables, HTML
//! comments, inline code and a `---`-delimited front-matter block are never
//! checked. Every rule has an id, a level and a one-line message that names
//! the offending text.

mod names;
mod rules;

pub use osf_lint_core::{Finding, KnownNames, Level};

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

/// What the text is. A message is a reply to a person, where a heading in a
/// short text is noise. A document follows a template that may require them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Message,
    Document,
}

/// Lints a document or a message. With `fast_only`, only the deterministic
/// fast tier runs; the stop hook uses this, since it must stay fast on
/// every turn end. The command line runs every tier.
///
/// With `no_suppress`, every `osf-disable`-family marker is ignored, so
/// every finding it would have silenced is reported. Continuous integration
/// runs with this set.
#[must_use]
pub fn lint_writing(
    text: &str,
    known: &KnownNames,
    kind: Kind,
    fast_only: bool,
    no_suppress: bool,
) -> Vec<Finding> {
    let doc = osf_lint_core::segment::parse(text);
    let mut findings = Vec::new();
    if kind == Kind::Message {
        rules::headings_in_short_text(&doc, &mut findings);
    }
    rules::per_sentence(&doc, fast_only, &mut findings);
    rules::undefined_names(&doc, known, &mut findings);
    let mut findings = if no_suppress {
        findings
    } else {
        osf_lint_core::apply_suppressions(text, findings, &rules::rule_ids())
    };
    osf_lint_core::sort_findings(&mut findings);
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint(text: &str) -> Vec<Finding> {
        lint_writing(
            text,
            &load_known_names(&[], None).expect("built-in names load"),
            Kind::Message,
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
            Kind::Document,
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

    #[test]
    fn dash_arrow_semicolon() {
        assert_eq!(rules_of("A thing — another thing."), vec!["em-dash"]);
        assert_eq!(errors_of("Input -> output."), vec!["arrow"]);
        assert_eq!(rules_of("It ran; it passed."), vec!["semicolon"]);
    }

    #[test]
    fn filler_phrases() {
        assert_eq!(rules_of("Let me know if that helps."), vec!["filler"]);
        assert_eq!(rules_of("We should leverage the cache."), vec!["filler"]);
    }

    #[test]
    fn numbers_in_prose() {
        assert_eq!(
            rules_of("It ran 12 axes over 3 rounds in 41 minutes."),
            vec!["numbers-in-prose"]
        );
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
        let f = lint_writing(text, &known, Kind::Message, false, true);
        let bare = f
            .iter()
            .find(|x| x.rule == "bare-reference")
            .expect("finding kept");
        assert!(bare.suppressed.is_none());
    }
}
