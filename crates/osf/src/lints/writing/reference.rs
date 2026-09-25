//! Finds text in one paragraph that a reader elsewhere could not resolve.

use crate::config::WritingConfig;
use osf_lint_core::segment::{self, TextUnit};
use osf_lint_core::{Context, KnownNames};
use regex::Regex;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Number,
    Phrase,
    Time,
    Name,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub kind: Kind,
    pub text: String,
    pub range: Range<usize>,
}

/// Words that never label a referent, so a number after one is a quantity, a year, or a clock time.
const NON_LABEL_WORDS: &[&str] = &[
    "a", "an", "the", "at", "in", "on", "of", "by", "for", "with", "from", "since", "until",
    "during", "before", "after", "about", "around", "near", "past", "over", "under", "to", "is",
    "was", "were", "are", "and", "or", "but", "this", "that", "these", "those", "it",
];

const MONTHS: &[&str] = &[
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

const ORDINAL_ALTERNATION: &str = "first|second|third|fourth|fifth|sixth|seventh|eighth|ninth|\
    tenth|eleventh|twelfth|thirteenth|fourteenth|fifteenth|sixteenth|seventeenth|eighteenth|\
    nineteenth|twentieth|twenty-first|twenty-second|twenty-third|twenty-fourth|twenty-fifth|\
    twenty-sixth|twenty-seventh|twenty-eighth|twenty-ninth|thirtieth|thirty-first";

const RELATIVE_DAY_PATTERN: &str = r"(?i)\b(?:yesterday|today|tomorrow|last week|next week|\
    last month|next month|last year|next year|this week|this month)\b";

/// One quoted or backticked span: a mention, never a number, phrase or time candidate.
struct QuotedSpan {
    open: Range<usize>,
    content: Range<usize>,
    is_backtick: bool,
}

/// Every candidate in `unit`'s own text, with its kind, text and byte range within it.
///
/// # Panics
/// Panics only if a built-in regex pattern fails to compile, which never happens.
pub fn candidates(
    unit: &TextUnit,
    cfg: &WritingConfig,
    known: &KnownNames,
    context: Context,
) -> Vec<Candidate> {
    let quotes = quoted_spans(&unit.text);
    let masked = mask_quotes(&unit.text, &quotes);
    let mut out = Vec::new();
    out.extend(number_candidates(&masked, known));
    out.extend(phrase_candidates(&masked, cfg));
    out.extend(time_candidates(&masked, context));
    out.extend(name_candidates(unit, &quotes, cfg));
    out
}

fn quoted_spans(text: &str) -> Vec<QuotedSpan> {
    static QUOTE: OnceLock<Regex> = OnceLock::new();
    static BACKTICK: OnceLock<Regex> = OnceLock::new();
    let quote_re = super::rules::re(&QUOTE, r#""([^"]*)""#);
    let backtick_re = super::rules::re(&BACKTICK, r"`([^`]*)`");
    let mut spans: Vec<QuotedSpan> = quote_re
        .captures_iter(text)
        .filter_map(|c| {
            Some(QuotedSpan {
                open: c.get(0)?.range(),
                content: c.get(1)?.range(),
                is_backtick: false,
            })
        })
        .collect();
    spans.extend(backtick_re.captures_iter(text).filter_map(|c| {
        Some(QuotedSpan {
            open: c.get(0)?.range(),
            content: c.get(1)?.range(),
            is_backtick: true,
        })
    }));
    spans
}

/// Blanks every quoted or backticked span so its content is never matched as a number, phrase or time.
fn mask_quotes(text: &str, spans: &[QuotedSpan]) -> String {
    let ranges: Vec<Range<usize>> = spans.iter().map(|s| s.open.clone()).collect();
    super::rules::mask_ranges(text, &ranges)
}

fn number_candidates(text: &str, known: &KnownNames) -> Vec<Candidate> {
    static HASH_REPO: OnceLock<Regex> = OnceLock::new();
    static HASH_BARE: OnceLock<Regex> = OnceLock::new();
    static WORD_NUMBER: OnceLock<Regex> = OnceLock::new();
    static WORD_BRACKET: OnceLock<Regex> = OnceLock::new();

    let mut out = Vec::new();
    let hash_repo = super::rules::re(&HASH_REPO, r"(?:[\w.-]+/)?[\w.-]+#\d+");
    out.extend(hash_repo.find_iter(text).map(|m| Candidate {
        kind: Kind::Number,
        text: m.as_str().to_string(),
        range: m.range(),
    }));

    let hash_bare = super::rules::re(&HASH_BARE, r"(?:^|[^\w/.-])(#\d+)\b");
    out.extend(hash_bare.captures_iter(text).filter_map(|c| {
        let g = c.get(1)?;
        Some(Candidate {
            kind: Kind::Number,
            text: g.as_str().to_string(),
            range: g.range(),
        })
    }));

    let word_number = super::rules::re(&WORD_NUMBER, r"\b([A-Za-z]+)\s+(\d+(?:\.\d+)*)\b");
    out.extend(
        word_number
            .captures_iter(text)
            .filter_map(|c| word_number_candidate(text, &c, known)),
    );

    let word_bracket = super::rules::re(
        &WORD_BRACKET,
        r"\b([A-Za-z][A-Za-z-]*)\s*\(([A-Za-z]|\d{1,2})\)",
    );
    out.extend(
        word_bracket
            .captures_iter(text)
            .filter_map(|c| word_bracket_candidate(&c, known)),
    );

    out
}

/// A word and a plain integer, unless it is a version, a range, a known name, or a month and day.
fn word_number_candidate(
    text: &str,
    c: &regex::Captures<'_>,
    known: &KnownNames,
) -> Option<Candidate> {
    let whole = c.get(0)?;
    let word = c.get(1)?;
    let number = c.get(2)?;
    let word_lower = word.as_str().to_lowercase();
    if NON_LABEL_WORDS.contains(&word_lower.as_str()) || MONTHS.contains(&word_lower.as_str()) {
        return None;
    }
    if number.as_str().contains('.') {
        return None;
    }
    let mut after = text.get(number.end()..).unwrap_or("").chars();
    if after.next() == Some('-') && after.next().is_some_and(|ch| ch.is_ascii_digit()) {
        return None;
    }
    if known.contains(word.as_str()) {
        return None;
    }
    Some(Candidate {
        kind: Kind::Number,
        text: whole.as_str().to_string(),
        range: whole.range(),
    })
}

/// A word and a single letter or short number in brackets, such as `mechanism (b)`.
fn word_bracket_candidate(c: &regex::Captures<'_>, known: &KnownNames) -> Option<Candidate> {
    let whole = c.get(0)?;
    let word = c.get(1)?;
    let word_lower = word.as_str().to_lowercase();
    if NON_LABEL_WORDS.contains(&word_lower.as_str()) || known.contains(word.as_str()) {
        return None;
    }
    Some(Candidate {
        kind: Kind::Number,
        text: whole.as_str().to_string(),
        range: whole.range(),
    })
}

fn phrase_candidates(text: &str, cfg: &WritingConfig) -> Vec<Candidate> {
    let mut alternatives: Vec<String> = cfg
        .chat_local_phrases
        .iter()
        .map(|p| regex::escape(p))
        .collect();
    alternatives.push(regex::escape("the previous"));
    let mut out = Vec::new();
    if let Some(re) = super::rules::word_boundary_alternation(&alternatives) {
        out.extend(re.find_iter(text).map(|m| Candidate {
            kind: Kind::Phrase,
            text: m.as_str().to_string(),
            range: m.range(),
        }));
    }
    out.extend(above_reference_candidates(text));
    out
}

/// `above` as a reference, told apart from the preposition by a determiner or number right after it.
fn above_reference_candidates(text: &str) -> Vec<Candidate> {
    const OBJECT_DETERMINERS: &[&str] = &["a", "an", "the"];
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = super::rules::re(&RE, r"(?i)\babove\b");
    re.find_iter(text)
        .filter_map(|m| {
            let rest = text.get(m.end()..).unwrap_or("").trim_start();
            let next_word = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            let is_object = OBJECT_DETERMINERS.contains(&next_word.as_str())
                || next_word.chars().next().is_some_and(|c| c.is_ascii_digit());
            (!is_object).then(|| Candidate {
                kind: Kind::Phrase,
                text: m.as_str().to_string(),
                range: m.range(),
            })
        })
        .collect()
}

fn time_candidates(text: &str, context: Context) -> Vec<Candidate> {
    static ORDINAL_WORD: OnceLock<Regex> = OnceLock::new();
    static ORDINAL_NUMERAL: OnceLock<Regex> = OnceLock::new();
    static WEEKDAY: OnceLock<Regex> = OnceLock::new();
    static RELATIVE_DAY: OnceLock<Regex> = OnceLock::new();

    let mut out = Vec::new();
    let ordinal_word = ORDINAL_WORD.get_or_init(|| {
        Regex::new(&format!(r"(?i)\bon the (?:{ORDINAL_ALTERNATION})\b"))
            .expect("ordinal-day pattern compiles")
    });
    out.extend(
        ordinal_word
            .find_iter(text)
            .filter(|m| is_bare_ordinal_date(text, m.end()))
            .map(time_candidate),
    );

    let ordinal_numeral = super::rules::re(&ORDINAL_NUMERAL, r"(?i)\bthe \d{1,2}(?:st|nd|rd|th)\b");
    out.extend(
        ordinal_numeral
            .find_iter(text)
            .filter(|m| is_bare_ordinal_date(text, m.end()))
            .map(time_candidate),
    );

    let weekday = super::rules::re(
        &WEEKDAY,
        r"(?i)\bon (?:Monday|Tuesday|Wednesday|Thursday|Friday|Saturday|Sunday)\b",
    );
    out.extend(weekday.find_iter(text).map(time_candidate));

    if context != Context::Transcript {
        let relative = super::rules::re(&RELATIVE_DAY, RELATIVE_DAY_PATTERN);
        out.extend(relative.find_iter(text).map(time_candidate));
    }

    out
}

/// An ordinal names a day only when punctuation, `of`, or nothing follows it, not another noun.
fn is_bare_ordinal_date(text: &str, end: usize) -> bool {
    let rest = text.get(end..).unwrap_or("").trim_start();
    match rest.chars().next() {
        None => true,
        Some(c) if !c.is_alphanumeric() => true,
        _ => rest
            .split_whitespace()
            .next()
            .is_some_and(|w| w.eq_ignore_ascii_case("of")),
    }
}

fn time_candidate(m: regex::Match<'_>) -> Candidate {
    Candidate {
        kind: Kind::Time,
        text: m.as_str().to_string(),
        range: m.range(),
    }
}

/// A capitalised run with real evidence, plus a lowercase quoted or code-free backticked term.
fn name_candidates(unit: &TextUnit, quotes: &[QuotedSpan], cfg: &WritingConfig) -> Vec<Candidate> {
    let mut out = Vec::new();
    out.extend(capitalised_name_candidates(unit, cfg));
    for q in quotes {
        let content = unit.text.get(q.content.clone()).unwrap_or("");
        if q.is_backtick && is_code_like(content) {
            continue;
        }
        if is_lowercase_term(content) {
            out.push(Candidate {
                kind: Kind::Name,
                text: content.to_string(),
                range: q.content.clone(),
            });
        }
    }
    out
}

/// Re-parses `unit`'s text into sentences so each one's own start is judged, not the paragraph's.
fn capitalised_name_candidates(unit: &TextUnit, cfg: &WritingConfig) -> Vec<Candidate> {
    let doc = segment::parse(&unit.text);
    let found: Vec<(Range<usize>, String)> = doc
        .sentences
        .iter()
        .flat_map(|s| {
            let start = s.span.start;
            super::rules::candidate_names(s)
                .into_iter()
                .filter_map(move |name| {
                    let local = s.text.find(&name)?;
                    Some((start + local..start + local + name.len(), name))
                })
        })
        .collect();
    let mut run_counts: HashMap<String, usize> = HashMap::new();
    for (_, name) in &found {
        if name.contains(' ') {
            *run_counts.entry(name.clone()).or_insert(0) += 1;
        }
    }
    found
        .into_iter()
        .filter(|(_, name)| {
            cfg.must_explain_names.contains(name)
                || super::rules::looks_like_a_name(name, &run_counts)
        })
        .map(|(range, text)| Candidate {
            kind: Kind::Name,
            text,
            range,
        })
        .collect()
}

/// A backtick span with a code character or a leading `--` is an identifier or a flag, not a phrase.
fn is_code_like(s: &str) -> bool {
    const FORBIDDEN: &[char] = &['/', '.', '_', '-', ':', '=', '(', ')', '$', '#'];
    s.starts_with("--") || s.chars().any(|c| FORBIDDEN.contains(&c))
}

fn is_lowercase_term(s: &str) -> bool {
    let word_count = s.split_whitespace().count();
    (1..=4).contains(&word_count)
        && s.chars().any(char::is_alphabetic)
        && !s.chars().any(char::is_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lints::load_known_names;
    use std::path::Path;

    fn known() -> KnownNames {
        load_known_names(&[], None).expect("built-in names load")
    }

    fn paragraph(text: &str) -> TextUnit {
        segment::parse(text)
            .paragraphs
            .into_iter()
            .next()
            .expect("one paragraph")
    }

    fn find(text: &str, context: Context) -> Vec<Candidate> {
        candidates(
            &paragraph(text),
            &WritingConfig::default(),
            &known(),
            context,
        )
    }

    fn kinds(text: &str, context: Context) -> Vec<Kind> {
        find(text, context).into_iter().map(|c| c.kind).collect()
    }

    fn has(text: &str, kind: Kind, excerpt: &str) -> bool {
        find(text, Context::Document)
            .iter()
            .any(|c| c.kind == kind && c.text == excerpt)
    }

    // --- number: shapes ---

    #[test]
    fn number_bare_hash() {
        assert!(has("Fixed in #12 today.", Kind::Number, "#12"));
    }

    #[test]
    fn number_repo_qualified_hash() {
        assert!(has(
            "The fix landed in acme/widgets#125 today.",
            Kind::Number,
            "acme/widgets#125"
        ));
    }

    #[test]
    fn number_word_and_number() {
        assert!(has(
            "We tracked it to issue 31 today.",
            Kind::Number,
            "issue 31"
        ));
        assert!(has("Ship Milestone 3 next.", Kind::Number, "Milestone 3"));
    }

    #[test]
    fn number_word_and_bracket_letter() {
        assert!(has(
            "The outage traced back to mechanism (b).",
            Kind::Number,
            "mechanism (b)"
        ));
    }

    // --- number: exclusions ---

    #[test]
    fn number_excludes_a_version() {
        assert!(kinds(
            "We pinned the renderer to moon 2.5.5 today.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Number));
    }

    #[test]
    fn number_excludes_a_known_name_followed_by_a_number() {
        assert!(kinds(
            "The crash only reproduces on Windows 11.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Number));
    }

    #[test]
    fn number_excludes_a_year() {
        assert!(
            kinds("The report was published in 2026.", Context::Document)
                .iter()
                .all(|k| *k != Kind::Number)
        );
    }

    #[test]
    fn number_excludes_a_clock_time() {
        assert!(
            kinds("We will regroup at 5 this afternoon.", Context::Document)
                .iter()
                .all(|k| *k != Kind::Number)
        );
    }

    #[test]
    fn number_excludes_a_hyphenated_range() {
        assert!(
            kinds("See issues 12-15 for the full list.", Context::Document)
                .iter()
                .all(|k| *k != Kind::Number)
        );
    }

    #[test]
    fn number_excludes_a_month_and_day() {
        assert!(kinds(
            "The change shipped on May 3 after the freeze lifted.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Number));
    }

    // --- phrase: shapes ---

    #[test]
    fn phrase_configured_chat_local_phrase() {
        assert!(has(
            "As discussed, we will hold the release.",
            Kind::Phrase,
            "As discussed"
        ));
    }

    #[test]
    fn phrase_the_previous() {
        assert!(has(
            "Ship the previous fix before touching anything new.",
            Kind::Phrase,
            "the previous"
        ));
    }

    #[test]
    fn phrase_above_as_a_reference() {
        assert!(has(
            "Run the migration above before opening the pull request.",
            Kind::Phrase,
            "above"
        ));
    }

    // --- phrase: exclusions ---

    #[test]
    fn phrase_above_is_not_a_reference_before_an_object() {
        assert!(kinds(
            "Keep the volume above the recommended limit.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Phrase));
        assert!(kinds("It runs fine above 10 minutes.", Context::Document)
            .iter()
            .all(|k| *k != Kind::Phrase));
    }

    #[test]
    fn phrase_a_quoted_mention_is_never_a_phrase_candidate() {
        let t = r#"We removed the stock opener "as discussed" from every reply template."#;
        assert!(kinds(t, Context::Document)
            .iter()
            .all(|k| *k != Kind::Phrase));
        assert!(has(t, Kind::Name, "as discussed"));
    }

    // --- time: shapes ---

    #[test]
    fn time_ordinal_day_word() {
        assert!(has(
            "The report went out on the eighteenth, right after the review closed.",
            Kind::Time,
            "on the eighteenth"
        ));
    }

    #[test]
    fn time_ordinal_day_numeral() {
        assert!(has(
            "It shipped on the 18th of the month.",
            Kind::Time,
            "the 18th"
        ));
    }

    #[test]
    fn time_weekday_alone() {
        assert!(has(
            "Ship it on Monday once reviews land.",
            Kind::Time,
            "on Monday"
        ));
    }

    #[test]
    fn time_excludes_an_ordinal_adjective_for_an_unrelated_noun() {
        assert!(kinds(
            "We still need a repro on the third build before we file it.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Time));
    }

    #[test]
    fn time_relative_day_in_durable_text() {
        assert!(has(
            "The bug appeared yesterday after the deploy.",
            Kind::Time,
            "yesterday"
        ));
    }

    // --- time: exclusion ---

    #[test]
    fn time_relative_day_is_not_a_candidate_in_a_transcript() {
        assert!(kinds(
            "The bug appeared yesterday after the deploy.",
            Context::Transcript
        )
        .iter()
        .all(|k| *k != Kind::Time));
    }

    // --- name: shapes ---

    #[test]
    fn name_capitalised_with_internal_capital_evidence() {
        assert!(has(
            "We moved the analytics job onto DuckDB last night.",
            Kind::Name,
            "DuckDB"
        ));
    }

    #[test]
    fn name_quoted_lowercase_term() {
        assert!(has(
            r#"We closed out "the done wave" this morning."#,
            Kind::Name,
            "the done wave"
        ));
    }

    #[test]
    fn name_backtick_lowercase_term() {
        assert!(has(
            "The rollout used a `dark launch` for the new page.",
            Kind::Name,
            "dark launch"
        ));
    }

    // --- name: exclusions ---

    #[test]
    fn name_excludes_an_ordinary_capitalised_word_with_no_evidence() {
        assert!(
            kinds("The build passed. Then it shipped.", Context::Document)
                .iter()
                .all(|k| *k != Kind::Name)
        );
    }

    #[test]
    fn name_excludes_a_backtick_span_that_looks_like_code() {
        assert!(
            kinds("Read the plan at `docs/plan.md` first.", Context::Document)
                .iter()
                .all(|k| *k != Kind::Name)
        );
        assert!(kinds("Run it with `--verbose` on.", Context::Document)
            .iter()
            .all(|k| *k != Kind::Name));
    }

    // --- the fixture check ---

    fn fixture_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/writing/unplaceable")
    }

    fn fixture_header(text: &str, path: &Path) -> (String, String) {
        let marker = "<!-- osf-unplaceable";
        let start = text
            .find(marker)
            .unwrap_or_else(|| panic!("{}: no osf-unplaceable header", path.display()));
        let after = text.get(start + marker.len()..).unwrap_or("");
        let end = after
            .find("-->")
            .unwrap_or_else(|| panic!("{}: unterminated header", path.display()));
        let block = after.get(..end).unwrap_or("");
        let mut label = None;
        let mut kind = None;
        for line in block.lines() {
            let Some((key, value)) = line.trim().split_once(':') else {
                continue;
            };
            match key.trim() {
                "label" => label = Some(value.trim().to_string()),
                "kind" => kind = Some(value.trim().to_string()),
                _ => {}
            }
        }
        (
            label.unwrap_or_else(|| panic!("{}: no label", path.display())),
            kind.unwrap_or_else(|| panic!("{}: no kind", path.display())),
        )
    }

    fn kind_name(k: Kind) -> &'static str {
        match k {
            Kind::Number => "number",
            Kind::Phrase => "phrase",
            Kind::Time => "time",
            Kind::Name => "name",
        }
    }

    /// Every non-model unplaceable fixture must yield a candidate of its own kind.
    #[test]
    fn every_unplaceable_fixture_yields_a_candidate_of_its_kind() {
        let dir = fixture_dir();
        let cfg = WritingConfig::default();
        let known_names = known();
        let mut checked = 0;
        let mut placeable_with_candidates: Vec<String> = Vec::new();

        let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{} reads: {e}", dir.display()))
            .map(|entry| entry.expect("directory entry reads").path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
            .collect();
        entries.sort();

        for path in entries {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{} reads: {e}", path.display()));
            let (label, kind) = fixture_header(&text, &path);
            let doc = segment::parse(&text);
            let found: Vec<Candidate> = doc
                .paragraphs
                .iter()
                .flat_map(|p| candidates(p, &cfg, &known_names, Context::Document))
                .collect();

            if label == "unplaceable" && kind != "model" {
                checked += 1;
                assert!(
                    found.iter().any(|c| kind_name(c.kind) == kind),
                    "{}: expected a {kind} candidate, found {found:?}",
                    path.display()
                );
            }
            if label == "placeable" && !found.is_empty() {
                placeable_with_candidates.push(format!(
                    "{} ({} candidate(s))",
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    found.len()
                ));
            }
        }

        assert!(checked > 0, "no unplaceable fixtures were checked");
        if !placeable_with_candidates.is_empty() {
            println!("placeable fixtures that still yield a candidate (resolution clears these):");
            for line in &placeable_with_candidates {
                println!("  {line}");
            }
        }
    }
}
