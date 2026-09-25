//! Finds text in one paragraph that a reader elsewhere could not resolve.

use crate::config::WritingConfig;
use osf_lint_core::segment::{self, TextUnit};
use osf_lint_core::{Context, KnownNames};
use regex::Regex;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Mutex, OnceLock, PoisonError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    "during", "before", "after", "about", "around", "near", "past", "over", "under", "than", "to",
    "is", "was", "were", "are", "and", "or", "but", "this", "that", "these", "those", "it",
];

pub(super) const MONTHS: &[&str] = &[
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

/// The delimiter a mention span is wrapped in; only a double quote ever yields a name candidate.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MentionKind {
    DoubleQuote,
    SingleQuote,
    Backtick,
}

/// One quoted or backticked span: a mention, never a number, phrase or time candidate.
struct QuotedSpan {
    open: Range<usize>,
    content: Range<usize>,
    kind: MentionKind,
}

/// Every candidate in `unit`'s own text, with its kind, text and byte range within it.
/// `doc_run_counts` is the whole document's multi-word name-repeat evidence.
#[allow(clippy::implicit_hasher)]
pub fn candidates(
    unit: &TextUnit,
    cfg: &WritingConfig,
    known: &KnownNames,
    context: Context,
    doc_run_counts: &HashMap<String, usize>,
) -> Vec<Candidate> {
    let doc = segment::parse(&unit.text);
    let sentence_spans: Vec<Range<usize>> = doc.sentences.iter().map(|s| s.span.clone()).collect();
    let quotes = quoted_spans(&unit.text, &sentence_spans);
    let masked = mask_quotes(&unit.text, &quotes);
    let mut out = Vec::new();
    out.extend(number_candidates(&masked, known));
    out.extend(phrase_candidates(&masked, cfg));
    out.extend(time_candidates(&masked, context));
    out.extend(name_candidates(
        unit,
        &quotes,
        cfg,
        &doc.sentences,
        doc_run_counts,
    ));
    keep_longest_per_kind(out)
}

/// Multi-word name-repeat counts gathered once over the whole document, so a name split across paragraphs still counts as seen more than once.
#[must_use]
pub fn document_run_counts(doc_sentences: &[TextUnit]) -> HashMap<String, usize> {
    let mut run_counts = HashMap::new();
    for s in doc_sentences {
        for name in super::rules::candidate_names(s) {
            if name.contains(' ') {
                *run_counts.entry(name).or_insert(0) += 1;
            }
        }
    }
    run_counts
}

/// When two candidates of the same kind overlap, keeps only the longer one (`see above` over `above`).
fn keep_longest_per_kind(mut candidates: Vec<Candidate>) -> Vec<Candidate> {
    candidates.sort_by_key(|c| c.range.end - c.range.start);
    candidates.reverse();
    let mut kept: Vec<Candidate> = Vec::new();
    for c in candidates {
        let overlaps = kept.iter().any(|k| {
            k.kind == c.kind && k.range.start < c.range.end && c.range.start < k.range.end
        });
        if !overlaps {
            kept.push(c);
        }
    }
    kept.sort_by_key(|c| c.range.start);
    kept
}

fn quoted_spans(text: &str, sentences: &[Range<usize>]) -> Vec<QuotedSpan> {
    static BACKTICK: OnceLock<Regex> = OnceLock::new();

    let mut spans = Vec::new();
    for (open_ch, close_ch) in [('"', '"'), ('\u{201C}', '\u{201D}')] {
        spans.extend(simple_quote_spans(text, open_ch, close_ch).into_iter().map(
            |(open, content)| QuotedSpan {
                open,
                content,
                kind: MentionKind::DoubleQuote,
            },
        ));
    }
    for (open_ch, close_ch) in [('\'', '\''), ('\u{2018}', '\u{2019}')] {
        spans.extend(
            paired_quote_spans(text, open_ch, close_ch, sentences)
                .into_iter()
                .map(|(open, content)| QuotedSpan {
                    open,
                    content,
                    kind: MentionKind::SingleQuote,
                }),
        );
    }
    let backtick_re = super::rules::re(&BACKTICK, r"`([^`]*)`");
    spans.extend(backtick_re.captures_iter(text).filter_map(|c| {
        Some(QuotedSpan {
            open: c.get(0)?.range(),
            content: c.get(1)?.range(),
            kind: MentionKind::Backtick,
        })
    }));
    spans
}

/// Pairs of `open_ch ... close_ch` taken in order, with no adjacency check: a double quote is never an apostrophe.
fn simple_quote_spans(
    text: &str,
    open_ch: char,
    close_ch: char,
) -> Vec<(Range<usize>, Range<usize>)> {
    let mut spans = Vec::new();
    let mut search_from = 0;
    while let Some(open_rel) = text.get(search_from..).and_then(|s| s.find(open_ch)) {
        let open_start = search_from + open_rel;
        let content_start = open_start + open_ch.len_utf8();
        let Some(close_rel) = text.get(content_start..).and_then(|s| s.find(close_ch)) else {
            break;
        };
        let close_start = content_start + close_rel;
        spans.push((
            open_start..close_start + close_ch.len_utf8(),
            content_start..close_start,
        ));
        search_from = close_start + close_ch.len_utf8();
    }
    spans
}

/// A same-sentence, six-word-or-fewer `open_ch ... close_ch` pair with no other single-quote mark inside, so `don't`, `it's`, `teams'` and a wide-spanning pair of apostrophes never mask real text.
fn paired_quote_spans(
    text: &str,
    open_ch: char,
    close_ch: char,
    sentences: &[Range<usize>],
) -> Vec<(Range<usize>, Range<usize>)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let is_word = |idx: usize| chars.get(idx).is_some_and(|&(_, c)| c.is_alphanumeric());
    let mut spans = Vec::new();
    let mut i = 0;
    while let Some(&(start, ch)) = chars.get(i) {
        let opens = ch == open_ch && !(i > 0 && is_word(i - 1)) && is_word(i + 1);
        if !opens {
            i += 1;
            continue;
        }
        let content_start = start + ch.len_utf8();
        let mut close_at = None;
        for (j, &(pos, c)) in chars.iter().enumerate().skip(i + 1) {
            if c == close_ch && is_word(j - 1) && !is_word(j + 1) {
                close_at = Some((j, pos, c));
                break;
            }
        }
        if let Some((j, close_start, close_char)) = close_at {
            let content = text.get(content_start..close_start).unwrap_or("");
            let safe_to_mask = same_sentence(sentences, start, close_start)
                && content.split_whitespace().count() <= 6
                && !content.chars().any(is_single_quote_mark);
            if safe_to_mask {
                spans.push((
                    start..close_start + close_char.len_utf8(),
                    content_start..close_start,
                ));
                i = j;
            }
        }
        i += 1;
    }
    spans
}

/// Whether `a` and `b` fall inside the same one of `sentences`.
fn same_sentence(sentences: &[Range<usize>], a: usize, b: usize) -> bool {
    sentences.iter().any(|s| s.contains(&a) && s.contains(&b))
}

fn is_single_quote_mark(c: char) -> bool {
    matches!(c, '\'' | '\u{2018}' | '\u{2019}')
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

/// `word` itself, Title-cased, and fully upper-cased: every form a case-blind known-name lookup must try.
fn case_variants(word: &str) -> [String; 3] {
    let lower = word.to_lowercase();
    let mut chars = lower.chars();
    let title = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    };
    [word.to_string(), title, word.to_uppercase()]
}

/// Whether `word` is also a real name by `names::is_name_head`, not just a known word.
fn is_known_name_head(known: &KnownNames, word: &str) -> bool {
    case_variants(word)
        .into_iter()
        .any(|form| known.contains(&form) && super::names::is_name_head(&form))
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
    if is_known_name_head(known, word.as_str()) {
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
    if NON_LABEL_WORDS.contains(&word_lower.as_str()) || is_known_name_head(known, word.as_str()) {
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
    if let Some(re) = cached_phrase_regex(&alternatives) {
        out.extend(re.find_iter(text).map(|m| Candidate {
            kind: Kind::Phrase,
            text: m.as_str().to_string(),
            range: m.range(),
        }));
    }
    out.extend(above_reference_candidates(text));
    out
}

/// Compiling this pattern costs a few milliseconds; the last-built regex is kept so a lint run
/// that calls this once per paragraph with the same configured phrases only pays that cost once.
fn cached_phrase_regex(alternatives: &[String]) -> Option<Regex> {
    static CACHE: Mutex<Option<(Vec<String>, Regex)>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((key, re)) = cache.as_ref() {
        if key == alternatives {
            return Some(re.clone());
        }
    }
    let re = super::rules::word_boundary_alternation(alternatives)?;
    *cache = Some((alternatives.to_vec(), re.clone()));
    Some(re)
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

/// A capitalised run with real evidence, plus a lowercase double-quoted term; backticks mark code.
fn name_candidates(
    unit: &TextUnit,
    quotes: &[QuotedSpan],
    cfg: &WritingConfig,
    sentences: &[TextUnit],
    doc_run_counts: &HashMap<String, usize>,
) -> Vec<Candidate> {
    let mut out = Vec::new();
    out.extend(capitalised_name_candidates(
        cfg,
        unit,
        sentences,
        doc_run_counts,
    ));
    for q in quotes {
        if q.kind != MentionKind::DoubleQuote {
            continue;
        }
        let content = unit.text.get(q.content.clone()).unwrap_or("");
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

/// Reads `sentences` with `unit`'s own heading and list-item flags, so a re-parsed paragraph does not lose them.
fn capitalised_name_candidates(
    cfg: &WritingConfig,
    unit: &TextUnit,
    sentences: &[TextUnit],
    doc_run_counts: &HashMap<String, usize>,
) -> Vec<Candidate> {
    let found: Vec<(Range<usize>, String)> = sentences
        .iter()
        .flat_map(|s| {
            let start = s.span.start;
            let mut flagged = s.clone();
            flagged.is_heading = unit.is_heading;
            flagged.in_list_item = unit.in_list_item;
            let mut cursor = 0usize;
            super::rules::candidate_names(&flagged)
                .into_iter()
                .filter_map(move |name| {
                    let local = s.text.get(cursor..)?.find(&name)?;
                    let offset = cursor + local;
                    cursor = offset + name.len();
                    Some((start + offset..start + offset + name.len(), name))
                })
        })
        .collect();
    found
        .into_iter()
        .filter(|(_, name)| {
            cfg.must_explain_names.contains(name)
                || super::rules::looks_like_a_name(name, doc_run_counts)
        })
        .map(|(range, text)| Candidate {
            kind: Kind::Name,
            text,
            range,
        })
        .collect()
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
        let run_counts = document_run_counts(&segment::parse(text).sentences);
        candidates(
            &paragraph(text),
            &WritingConfig::default(),
            &known(),
            context,
            &run_counts,
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

    /// Documents today's behaviour: a technical pair stays a candidate until a later sweep measures it.
    #[test]
    fn number_word_and_number_technical_pairs_are_candidates_for_now() {
        assert!(has(
            "The request failed with HTTP 404 today.",
            Kind::Number,
            "HTTP 404"
        ));
        assert!(has(
            "The service listens on port 8080 now.",
            Kind::Number,
            "port 8080"
        ));
        assert!(has(
            "We only show the top 10 results.",
            Kind::Number,
            "top 10"
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

    /// A generic label word from `names.rs`, such as `Phase` or `Item`, is not a known name, so the number after it must still be a candidate.
    #[test]
    fn number_does_not_exclude_a_generic_label_word_followed_by_a_number() {
        assert!(has("In Phase 2 we ship it.", Kind::Number, "Phase 2"));
        assert!(has("In Item 3 we ship it.", Kind::Number, "Item 3"));
        assert!(has("Do Step 4 next.", Kind::Number, "Step 4"));
    }

    #[test]
    fn number_excludes_a_known_name_regardless_of_case() {
        assert!(kinds(
            "The crash only reproduces on windows 11.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Number));
    }

    /// A byte-slicing title-case would panic or silently miss a multi-byte first letter; char-based case mapping does not.
    #[test]
    fn is_known_name_title_cases_a_multi_byte_first_letter() {
        let known = load_known_names(&["\u{c9}clair".to_string()], None)
            .expect("known names with a multi-byte entry load");
        assert!(case_variants("\u{e9}clair")
            .iter()
            .any(|form| known.contains(form)));
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
    fn number_excludes_a_comparison_quantity() {
        assert!(
            kinds("No more than 20 words in a procedure.", Context::Document)
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

    /// "See above" once yielded both "See above" and "above"; only the longer one survives now.
    #[test]
    fn phrase_overlapping_candidates_of_the_same_kind_keep_only_the_longest() {
        let found = find("See above for the full list.", Context::Document);
        let phrases: Vec<&str> = found
            .iter()
            .filter(|c| c.kind == Kind::Phrase)
            .map(|c| c.text.as_str())
            .collect();
        assert_eq!(phrases, vec!["See above"], "{found:?}");
    }

    // --- mentions: every quote style masks every other shape ---

    #[test]
    fn straight_double_quotes_mask_every_shape() {
        assert!(
            kinds(r#"We saw "as discussed" in the notes."#, Context::Document)
                .iter()
                .all(|k| *k != Kind::Phrase)
        );
        assert!(
            kinds(r#"We saw "issue 31" in the notes."#, Context::Document)
                .iter()
                .all(|k| *k != Kind::Number)
        );
        assert!(
            kinds(r#"We saw "on Monday" in the notes."#, Context::Document)
                .iter()
                .all(|k| *k != Kind::Time)
        );
    }

    #[test]
    fn curly_double_quotes_mask_every_shape() {
        assert!(kinds(
            "We saw \u{201C}as discussed\u{201D} in the notes.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Phrase));
        assert!(kinds(
            "We saw \u{201C}issue 31\u{201D} in the notes.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Number));
        assert!(kinds(
            "We saw \u{201C}on Monday\u{201D} in the notes.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Time));
    }

    #[test]
    fn straight_single_quotes_mask_every_shape() {
        assert!(
            kinds("We saw 'as discussed' in the notes.", Context::Document)
                .iter()
                .all(|k| *k != Kind::Phrase)
        );
        assert!(kinds("We saw 'issue 31' in the notes.", Context::Document)
            .iter()
            .all(|k| *k != Kind::Number));
        assert!(kinds("We saw 'on Monday' in the notes.", Context::Document)
            .iter()
            .all(|k| *k != Kind::Time));
    }

    #[test]
    fn curly_single_quotes_mask_every_shape() {
        assert!(kinds(
            "We saw \u{2018}as discussed\u{2019} in the notes.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Phrase));
        assert!(kinds(
            "We saw \u{2018}issue 31\u{2019} in the notes.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Number));
        assert!(kinds(
            "We saw \u{2018}on Monday\u{2019} in the notes.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Time));
    }

    /// A word's own apostrophe never opens or closes a mention span, so a candidate right after it still shows up.
    #[test]
    fn an_apostrophe_never_opens_or_closes_a_mention_span() {
        assert!(has("Don't ship issue 31 today.", Kind::Number, "issue 31"));
        assert!(has(
            "It's tracked as issue 31 now.",
            Kind::Number,
            "issue 31"
        ));
        assert!(has(
            "The team's issue 31 is still open.",
            Kind::Number,
            "issue 31"
        ));
        assert!(has(
            "The teams' issue 31 is still open.",
            Kind::Number,
            "issue 31"
        ));
    }

    /// Two apostrophes far apart, one opening a decade and one closing a plural possessive, must not pair up and mask everything between them.
    #[test]
    fn a_decade_apostrophe_and_a_plural_possessive_never_pair_up() {
        assert!(has(
            "The '90s had issue 31 fixed, and the teams' report doesn't mention it.",
            Kind::Number,
            "issue 31"
        ));
    }

    /// A single-quote mark inside the span, as in 'n' inside a longer quote, rules out that pairing rather than widening it.
    #[test]
    fn a_span_with_an_inner_single_quote_mark_is_never_masked() {
        assert!(has(
            "That was 'rock 'n' roll' to us, and issue 31 shipped fine.",
            Kind::Number,
            "issue 31"
        ));
    }

    /// A rejected outer pair must not jump past the real, valid quote it was hiding.
    #[test]
    fn a_rejected_pair_still_lets_a_later_open_in_the_same_sentence_mask() {
        let t = "The 'quote here never closes and so we lose 'fix 5' entirely in one sentence.";
        assert!(
            !has(t, Kind::Number, "fix 5"),
            "{:?}",
            find(t, Context::Document)
        );
    }

    /// The same failure, but the rejected pair spans two sentences instead of one.
    #[test]
    fn a_rejected_pair_across_sentences_still_lets_a_later_open_mask() {
        let t = "The 'promo never closes here and just keeps going. Later they said 'fix 5' aloud.";
        assert!(
            !has(t, Kind::Number, "fix 5"),
            "{:?}",
            find(t, Context::Document)
        );
    }

    #[test]
    fn a_single_quote_that_never_closes_masks_nothing() {
        assert!(has(
            "The 'end of the story never closes, but issue 31 still ships.",
            Kind::Number,
            "issue 31"
        ));
    }

    #[test]
    fn a_single_quoted_span_of_more_than_six_words_is_never_masked() {
        assert!(has(
            "She called it 'a phrase about issue 31 that runs on for entirely too long here' today.",
            Kind::Number,
            "issue 31"
        ));
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
    fn name_quoted_lowercase_term_with_curly_double_quotes() {
        assert!(has(
            "We closed out \u{201C}the done wave\u{201D} this morning.",
            Kind::Name,
            "the done wave"
        ));
    }

    /// A double quote closes on the next quote mark, whatever sits right before it.
    #[test]
    fn name_quoted_lowercase_term_with_punctuation_before_the_close() {
        assert!(has(
            r#"The glossary lists, for example, "the done wave," a phrase the team coined."#,
            Kind::Name,
            "the done wave,"
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
    fn name_excludes_every_backtick_span_even_a_natural_language_one() {
        assert!(
            kinds("Read the plan at `docs/plan.md` first.", Context::Document)
                .iter()
                .all(|k| *k != Kind::Name)
        );
        assert!(kinds("Run it with `--verbose` on.", Context::Document)
            .iter()
            .all(|k| *k != Kind::Name));
        assert!(kinds(
            "The rollout used a `dark launch` for the new page.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Name));
    }

    #[test]
    fn name_excludes_a_single_quoted_term() {
        assert!(kinds(
            "We called it 'the done wave' this morning.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Name));
    }

    #[test]
    fn name_gives_a_repeated_run_its_own_range_each_time() {
        let all = find("DuckDB beat DuckDB in every benchmark.", Context::Document);
        let names: Vec<&Candidate> = all.iter().filter(|c| c.kind == Kind::Name).collect();
        let [first, second] = names.as_slice() else {
            panic!("expected exactly two name candidates, found {all:?}");
        };
        assert_ne!(first.range, second.range, "{all:?}");
        assert!(second.range.start > first.range.start, "{all:?}");
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
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let doc = segment::parse(&text);
            let run_counts = document_run_counts(&doc.sentences);
            let found: Vec<Candidate> = doc
                .paragraphs
                .iter()
                .flat_map(|p| candidates(p, &cfg, &known_names, Context::Document, &run_counts))
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
                placeable_with_candidates
                    .push(format!("{file_name} ({} candidate(s))", found.len()));
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
