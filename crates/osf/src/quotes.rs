//! Checks each answer's quoted findings against the file and line each one
//! names: a finding counts only when the text is really there, and the
//! only way to get a reviewer answer the reducer can score is through
//! [`check`], which builds a [`VerifiedAnswer`] that nothing outside this
//! module can construct any other way.
//!
//! A quote is kept when its text, with internal whitespace runs collapsed
//! to one space, is found within [`LINE_TOLERANCE`] lines of the line the
//! finding names, so a model's own line count drifting by a line or two
//! does not lose a real finding. Line endings are read through
//! [`str::lines`], which reads a trailing `\r` the same as none: a quote
//! typed against an LF checkout still matches a CRLF file, and the other
//! way round. A short quote is a weak claim on its own: a single
//! punctuation mark or a common keyword sits near almost every line in a
//! real file, so a one-line quote needs at least twelve non-whitespace
//! characters to be checked as a substring of a line; between six and
//! eleven it must equal a whole line exactly, not just appear inside one;
//! fewer than six is never checked at all and is dropped as too short to
//! verify. A quote of several lines is matched as that whole block, line
//! by line, with no minimum length, since spanning lines is itself a
//! specific claim a short token cannot make by accident. A path that
//! resolves outside the repository, or that names no file under it, is
//! dropped without reading any line at all.

use crate::answer::{Answer, AnswerFinding};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// How many lines a quote's named line may drift and still be found: the
/// line itself, and up to two lines either side.
const LINE_TOLERANCE: usize = 2;

/// The fewest non-whitespace characters a one-line quote needs before it is checked as a substring of a line.
const SUBSTRING_MIN_NON_WHITESPACE: usize = 12;

/// The fewest non-whitespace characters a one-line quote needs to be checked at all, as an exact whole-line match.
const WHOLE_LINE_MIN_NON_WHITESPACE: usize = 6;

/// One reviewer's answer with its findings checked against the files:
/// buildable only by [`check`], because its fields are private to this
/// module. A [`crate::reducer::LensAnswer`] can only hold one of these, not
/// a raw [`Answer`], so a lens can never be scored from findings that were
/// never checked.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedAnswer {
    lens: String,
    scores: BTreeMap<String, f64>,
    findings: Vec<AnswerFinding>,
}

impl VerifiedAnswer {
    #[must_use]
    pub fn lens(&self) -> &str {
        &self.lens
    }

    #[must_use]
    pub fn scores(&self) -> &BTreeMap<String, f64> {
        &self.scores
    }

    #[must_use]
    pub fn findings(&self) -> &[AnswerFinding] {
        &self.findings
    }
}

/// The verified answer that came out of checking `answer`, and the
/// findings dropped along the way, each with the reason.
pub struct Checked {
    pub kept: VerifiedAnswer,
    pub dropped: Vec<(AnswerFinding, String)>,
}

/// Checks every finding in `answer` against the files under `root`, and
/// returns the only kind of value [`crate::reducer::decide_lens`] can score: a [`VerifiedAnswer`].
#[must_use]
pub fn check(root: &Path, answer: Answer) -> Checked {
    let mut kept_findings = Vec::new();
    let mut dropped = Vec::new();
    for finding in answer.findings {
        match verify(root, &finding) {
            Ok(()) => kept_findings.push(finding),
            Err(reason) => {
                let message = format!("{}:{}: {reason}", finding.path, finding.line);
                dropped.push((finding, message));
            }
        }
    }
    Checked {
        kept: VerifiedAnswer {
            lens: answer.lens,
            scores: answer.scores,
            findings: kept_findings,
        },
        dropped,
    }
}

/// `Ok(())` when `finding`'s quote is really at the file and line it names, `Err` naming why not otherwise.
fn verify(root: &Path, finding: &AnswerFinding) -> Result<(), String> {
    let resolved = resolve_under_root(root, &finding.path)?;
    let content =
        std::fs::read_to_string(&resolved).map_err(|e| format!("could not be read: {e}"))?;
    let file_lines: Vec<&str> = content.lines().collect();
    match quote_found(&file_lines, finding.line, &finding.quote) {
        QuoteOutcome::Found => Ok(()),
        QuoteOutcome::TooShort => Err("too short to verify".to_string()),
        QuoteOutcome::NotFound => Err("quoted text not found near this line".to_string()),
    }
}

/// `path` resolved under `root`, as a file that exists there without leaving `root`.
fn resolve_under_root(root: &Path, path: &str) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(path.replace('\\', "/"));
    if candidate.is_absolute() {
        return Err("outside the repository".to_string());
    }
    let root_collapsed = lexically_normalize(root);
    let joined_collapsed = lexically_normalize(&root.join(&candidate));
    if !joined_collapsed.starts_with(&root_collapsed) {
        return Err("outside the repository".to_string());
    }
    if !joined_collapsed.is_file() {
        return Err("no such file".to_string());
    }
    Ok(joined_collapsed)
}

/// `path`'s `.` and `..` components collapsed left to right, without touching the filesystem.
fn lexically_normalize(path: &Path) -> PathBuf {
    let mut out: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out.into_iter().collect()
}

/// Whether a quote was found near its named line, not found, or too short a claim to check at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuoteOutcome {
    Found,
    NotFound,
    TooShort,
}

/// `s` with every run of whitespace, of any kind Unicode calls whitespace,
/// collapsed to one space and its own leading and trailing whitespace
/// dropped: a tab, doubled alignment spaces and a non-breaking space all
/// read the same as a single ordinary space.
fn collapse_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How many characters of `s` are not whitespace.
fn non_whitespace_count(s: &str) -> usize {
    s.chars().filter(|c| !c.is_whitespace()).count()
}

/// Whether `quote` is really in `file_lines` within [`LINE_TOLERANCE`] lines of `named_line`, given the shape rules a one-line quote must meet.
fn quote_found(file_lines: &[&str], named_line: u64, quote: &str) -> QuoteOutcome {
    let quote_lines: Vec<String> = quote.lines().map(collapse_whitespace).collect();
    if quote_lines.iter().all(String::is_empty) {
        return QuoteOutcome::TooShort;
    }
    let width = quote_lines.len();
    let joined_quote = quote_lines.join("\n");

    let require_whole_line = if width == 1 {
        let non_whitespace = non_whitespace_count(&joined_quote);
        if non_whitespace < WHOLE_LINE_MIN_NON_WHITESPACE {
            return QuoteOutcome::TooShort;
        }
        non_whitespace < SUBSTRING_MIN_NON_WHITESPACE
    } else {
        false
    };

    let named_index = usize::try_from(named_line.saturating_sub(1)).unwrap_or(usize::MAX);
    let first = named_index.saturating_sub(LINE_TOLERANCE);
    let last = named_index.saturating_add(LINE_TOLERANCE);

    let found = (first..=last).any(|start| {
        file_lines
            .get(start..start.saturating_add(width))
            .is_some_and(|window| block_matches(window, &joined_quote, require_whole_line))
    });

    if found {
        QuoteOutcome::Found
    } else {
        QuoteOutcome::NotFound
    }
}

/// Whether `window`, each line collapsed the same way as the quote, holds
/// `joined_quote`: for one line, a substring unless `require_whole_line`
/// asks for the line and the quote to be equal; for several lines, the
/// whole block must be equal.
fn block_matches(window: &[&str], joined_quote: &str, require_whole_line: bool) -> bool {
    let joined_window = window
        .iter()
        .map(|line| collapse_whitespace(line))
        .collect::<Vec<_>>()
        .join("\n");
    if window.len() == 1 && !require_whole_line {
        joined_window.contains(joined_quote)
    } else {
        joined_window == joined_quote
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::answer::{Action, Severity};
    use crate::test_support::TempDir;

    fn finding(path: &str, line: u64, quote: &str) -> AnswerFinding {
        AnswerFinding {
            path: path.to_string(),
            line,
            quote: quote.to_string(),
            severity: Severity::Minor,
            action: Action::Justify,
            body: "note".to_string(),
        }
    }

    fn answer(findings: Vec<AnswerFinding>) -> Answer {
        Answer {
            lens: "correctness".to_string(),
            scores: BTreeMap::new(),
            findings,
        }
    }

    /// A fresh temp repository holding one file, its parent directories created first.
    fn root_with_file(name: &str, file_name: &str, content: &str) -> TempDir {
        let root = TempDir::new(name);
        let path = root.join(file_name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("dirs");
        }
        std::fs::write(&path, content).expect("write");
        root
    }

    #[test]
    fn a_quote_at_its_line_is_kept_and_an_invented_one_is_dropped() {
        let root = root_with_file(
            "quotes-basic",
            "src/lib.rs",
            "fn one() {}\nfn two() {}\nfn three() {}\n",
        );
        let findings = vec![
            finding("src/lib.rs", 2, "fn two() {}"),
            finding("src/lib.rs", 2, "fn invented() {}"),
        ];
        let checked = check(&root, answer(findings));
        assert_eq!(checked.kept.findings().len(), 1, "{:?}", checked.dropped);
        assert_eq!(checked.dropped.len(), 1);
        let kept = checked.kept.findings().first().expect("one kept");
        assert_eq!(kept.quote, "fn two() {}");
        let (dropped_finding, reason) = checked.dropped.first().expect("one dropped");
        assert_eq!(dropped_finding.quote, "fn invented() {}");
        assert!(reason.contains("src/lib.rs"), "{reason}");
        assert!(reason.contains('2'), "{reason}");
    }

    #[test]
    fn trailing_whitespace_is_tolerated_on_either_side() {
        let root = root_with_file(
            "quotes-trailing-ws",
            "src/lib.rs",
            "fn one() {}\nlet x = 1;   \nfn three() {}\n",
        );
        let findings = vec![finding("src/lib.rs", 2, "let x = 1;")];
        let checked = check(&root, answer(findings));
        assert_eq!(checked.kept.findings().len(), 1, "{:?}", checked.dropped);
    }

    #[test]
    fn a_crlf_file_matches_an_lf_quote() {
        let root = root_with_file(
            "quotes-crlf",
            "src/lib.rs",
            "fn one() {}\r\nlet y = 2;\r\nfn three() {}\r\n",
        );
        let findings = vec![finding("src/lib.rs", 2, "let y = 2;")];
        let checked = check(&root, answer(findings));
        assert_eq!(checked.kept.findings().len(), 1, "{:?}", checked.dropped);
    }

    #[test]
    fn a_quote_spanning_several_lines_is_kept() {
        let root = root_with_file(
            "quotes-multiline",
            "src/lib.rs",
            "fn one() {}\nfn two() {\n    let x = 1;\n    x + 1\n}\n",
        );
        let findings = vec![finding(
            "src/lib.rs",
            2,
            "fn two() {\n    let x = 1;\n    x + 1\n}",
        )];
        let checked = check(&root, answer(findings));
        assert_eq!(checked.kept.findings().len(), 1, "{:?}", checked.dropped);
    }

    #[test]
    fn a_quote_two_lines_off_is_kept_but_three_lines_off_is_dropped() {
        let root = root_with_file(
            "quotes-drift",
            "src/lib.rs",
            "one\ntwo\nthree\nfour\ntarget line\nsix\nseven\neight\nnine\nten\n",
        );
        let close = finding("src/lib.rs", 7, "target line");
        let far = finding("src/lib.rs", 8, "target line");
        let checked = check(&root, answer(vec![close, far]));
        assert_eq!(checked.kept.findings().len(), 1, "{:?}", checked.dropped);
        assert_eq!(checked.dropped.len(), 1);
        let kept = checked.kept.findings().first().expect("one kept");
        assert_eq!(kept.line, 7);
        let (dropped_finding, _) = checked.dropped.first().expect("one dropped");
        assert_eq!(dropped_finding.line, 8);
    }

    #[test]
    fn a_quote_of_a_nonexistent_path_is_dropped() {
        let root = TempDir::new("quotes-missing-file");
        let findings = vec![finding("src/missing.rs", 1, "anything at all")];
        let checked = check(&root, answer(findings));
        assert_eq!(checked.kept.findings().len(), 0);
        let (_, reason) = checked.dropped.first().expect("one dropped");
        assert!(reason.contains("no such file"), "{reason}");
        assert!(reason.contains("src/missing.rs"), "{reason}");
    }

    #[test]
    fn a_quote_of_a_path_outside_the_repository_is_dropped() {
        let root = TempDir::new("quotes-outside-root");
        let findings = vec![
            finding("../outside.rs", 1, "anything at all"),
            finding("/etc/passwd", 1, "anything at all"),
        ];
        let checked = check(&root, answer(findings));
        assert_eq!(checked.kept.findings().len(), 0);
        assert_eq!(checked.dropped.len(), 2);
        for (_, reason) in &checked.dropped {
            assert!(reason.contains("outside the repository"), "{reason}");
        }
    }

    #[test]
    fn a_quote_under_six_non_whitespace_characters_is_dropped_as_too_short() {
        let root = root_with_file(
            "quotes-too-short",
            "src/lib.rs",
            "one (\ntwo )\nthree }\nfour let\n",
        );
        for (line, quote) in [(1, "("), (2, ")"), (3, "}"), (4, "let")] {
            let checked = check(&root, answer(vec![finding("src/lib.rs", line, quote)]));
            assert_eq!(
                checked.kept.findings().len(),
                0,
                "quote {quote:?} at line {line} should be dropped"
            );
            let (_, reason) = checked.dropped.first().expect("one dropped");
            assert!(reason.contains("too short to verify"), "{reason}");
        }
    }

    #[test]
    fn a_medium_quote_matches_a_whole_line_exactly() {
        let root = root_with_file(
            "quotes-medium-whole-line",
            "src/lib.rs",
            "one\nanswer\nthree\n",
        );
        let checked = check(&root, answer(vec![finding("src/lib.rs", 2, "answer")]));
        assert_eq!(checked.kept.findings().len(), 1, "{:?}", checked.dropped);
    }

    #[test]
    fn a_medium_quote_that_is_only_a_fragment_of_a_line_is_dropped() {
        let root = root_with_file(
            "quotes-medium-fragment",
            "src/lib.rs",
            "zero\nthe answer is here\ntwo\nthree\nfour\n",
        );
        let checked = check(&root, answer(vec![finding("src/lib.rs", 2, "answer")]));
        assert_eq!(checked.kept.findings().len(), 0);
        let (_, reason) = checked.dropped.first().expect("one dropped");
        assert!(reason.contains("quoted text not found"), "{reason}");
    }

    #[test]
    fn a_long_quote_still_matches_as_a_substring() {
        let root = root_with_file(
            "quotes-long-substring",
            "src/lib.rs",
            "fn one() {}\nlet identifier_name = compute_the_value();\n",
        );
        let checked = check(
            &root,
            answer(vec![finding(
                "src/lib.rs",
                2,
                "identifier_name = compute_the_value()",
            )]),
        );
        assert_eq!(checked.kept.findings().len(), 1, "{:?}", checked.dropped);
    }

    #[test]
    fn a_tab_a_doubled_space_and_a_non_breaking_space_are_all_collapsed() {
        let root = root_with_file(
            "quotes-collapse-whitespace",
            "src/lib.rs",
            "one\nlet\tx  =\u{a0}1;\nthree\n",
        );
        let findings = vec![finding("src/lib.rs", 2, "let x = 1;")];
        let checked = check(&root, answer(findings));
        assert_eq!(checked.kept.findings().len(), 1, "{:?}", checked.dropped);
    }
}
