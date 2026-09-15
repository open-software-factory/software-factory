//! Writing lint: deterministic checks on prose written for people.
//!
//! Text is split into paragraphs and sentences. Fenced code, tables, HTML
//! comments and inline code are never checked. Every rule has an id, a level
//! and a one-line message that names the offending text.

mod names;
mod rules;
mod segment;

use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warning,
}

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub rule: &'static str,
    pub level: Level,
    pub line: usize,
    pub message: String,
    pub excerpt: String,
}

impl Finding {
    pub fn render(&self, name: &str, level: Level) -> String {
        let level = match level {
            Level::Error => "error",
            Level::Warning => "warning",
        };
        format!(
            "{}:{}: {} [{}] {}: \"{}\"",
            name, self.line, level, self.rule, self.message, self.excerpt
        )
    }

    pub fn to_json(&self, name: &str, level: Level) -> String {
        #[derive(Serialize)]
        struct Row<'a> {
            file: &'a str,
            line: usize,
            level: Level,
            rule: &'a str,
            message: &'a str,
            excerpt: &'a str,
        }
        serde_json::to_string(&Row {
            file: name,
            line: self.line,
            level,
            rule: self.rule,
            message: &self.message,
            excerpt: &self.excerpt,
        })
        .expect("a finding serialises")
    }
}

/// Names that need no description on first use: everyday tools and words.
/// Project names come from a file outside the repo, see `--known-names`.
pub struct KnownNames(HashSet<String>);

impl KnownNames {
    pub fn contains(&self, name: &str) -> bool {
        self.0.contains(name)
    }
}

pub fn load_known_names(path: Option<&Path>) -> Result<KnownNames, String> {
    let mut set: HashSet<String> = names::BUILT_IN.iter().map(ToString::to_string).collect();
    if let Some(p) = path {
        let text = std::fs::read_to_string(p)
            .map_err(|e| format!("cannot read known names {}: {e}", p.display()))?;
        for line in text.lines() {
            let t = line.trim();
            if !t.is_empty() && !t.starts_with('#') {
                set.insert(t.to_string());
            }
        }
    }
    Ok(KnownNames(set))
}

/// What the text is. A message is a reply to a person, where a heading in a
/// short text is noise. A document follows a template that may require them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Message,
    Document,
}

pub fn lint_writing(text: &str, known: &KnownNames, kind: Kind) -> Vec<Finding> {
    let doc = segment::parse(text);
    let mut findings = Vec::new();
    if kind == Kind::Message {
        rules::headings_in_short_text(&doc, &mut findings);
    }
    rules::per_sentence(&doc, &mut findings);
    rules::undefined_names(&doc, known, &mut findings);
    findings.sort_by_key(|f| (f.line, f.rule));
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint(text: &str) -> Vec<Finding> {
        lint_writing(
            text,
            &load_known_names(None).expect("built-in names load"),
            Kind::Message,
        )
    }

    #[test]
    fn a_document_may_have_headings() {
        let known = load_known_names(None).expect("built-in names load");
        assert!(lint_writing("## Result\n\nIt passed.\n", &known, Kind::Document).is_empty());
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
        let t = "The build failed on the test step. The fix is in [owner/repo#12 (the build fix)](https://example.com/12).\n";
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
            rules_of("Fixed in owner/repo#125 today."),
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
    fn table_cells_check_names_but_not_length() {
        let long = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty one two three four five six";
        let t = format!("| Tool | Note |\n|---|---|\n| prose | Use Vale for {long} |\n");
        assert_eq!(errors_of(&t), vec!["undefined-name"]);
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
}
