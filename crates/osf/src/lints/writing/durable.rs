//! Two rules for text that has to stay true after it is written: a count
//! word, such as "three stages", and a relative time word, such as "now".
//! Both read the sentence's words, not a bare pattern over its characters.
//! The caller skips both in a transcript, which is not kept.

use crate::config::WritingConfig;
use osf_lint_core::segment::{reduce_inline, TextUnit};
use osf_lint_core::{Finding, Level, Rule, Scope};
use regex::Regex;
use std::ops::Range;
use std::sync::OnceLock;

/// The remediation the owner asked for, said the same way every time.
const COUNT_REMEDIATION: &str =
    "Name the things, or say each, every or these, so the text stays true when the list changes.";
const TIME_REMEDIATION: &str = "State the date, or state the fact without a time word.";

fn re(cell: &'static OnceLock<Regex>, pattern: &'static str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("rule pattern compiles"))
}

/// One whitespace-separated word of a reduced sentence.
struct Tok<'a> {
    start: usize,
    raw: &'a str,
    /// The word without the punctuation around it, lower case.
    core: String,
    /// What came before the word's letters, such as an opening bracket.
    prefix: &'a str,
    /// What came after the word's letters, such as a comma.
    suffix: &'a str,
}

fn tokens(text: &str) -> Vec<Tok<'_>> {
    static WORD: OnceLock<Regex> = OnceLock::new();
    re(&WORD, r"\S+")
        .find_iter(text)
        .map(|m| {
            let raw = m.as_str();
            let first = raw.find(char::is_alphanumeric);
            let last = raw
                .char_indices()
                .rev()
                .find(|(_, c)| c.is_alphanumeric())
                .map(|(i, c)| i + c.len_utf8());
            let (prefix, core, suffix) = match (first, last) {
                (Some(a), Some(b)) if a < b => (&raw[..a], &raw[a..b], &raw[b..]),
                _ => ("", "", raw),
            };
            Tok {
                start: m.start(),
                raw,
                core: core.to_lowercase(),
                prefix,
                suffix,
            }
        })
        .collect()
}

/// Byte ranges of the text inside straight or curly double quotes.
fn quoted_spans(text: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut open: Option<usize> = None;
    for (i, c) in text.char_indices() {
        match (c, open) {
            ('"' | '\u{201c}', None) => open = Some(i),
            ('"' | '\u{201d}', Some(start)) => {
                spans.push(start..i + c.len_utf8());
                open = None;
            }
            _ => {}
        }
    }
    spans
}

/// The source text without the web address of each Markdown link, so a
/// date inside an address is never read as a date in the prose.
fn without_link_targets(raw: &str) -> String {
    static TARGET: OnceLock<Regex> = OnceLock::new();
    re(&TARGET, r"\]\([^)]*\)")
        .replace_all(raw, "]")
        .into_owned()
}

/// A link to a source, a footnote, a numbered citation, or a bracketed
/// `reported:` or `source:` note: the text then reports a fact that someone
/// else stated, and the figure is theirs.
fn cites_a_source(raw: &str) -> bool {
    static CITE: OnceLock<Regex> = OnceLock::new();
    re(
        &CITE,
        r"(?i)\]\(https?://|\[\^[^\]]+\]|\[\d+(?:\s*[,-]\s*\d+)*\]|\[(?:reported|source|sources|cited)\b[^\]]*\]",
    )
    .is_match(raw)
}

/// Whether the text right after a sentence opens with a citation, such as
/// `[reported: source]`, the way a note follows the claim it supports.
fn leads_a_citation(after: &str) -> bool {
    static NOTE: OnceLock<Regex> = OnceLock::new();
    re(
        &NOTE,
        r"(?i)^\s*(?:\[(?:reported|source|sources|cited)\b|\[\d+(?:\s*[,-]\s*\d+)*\]|\[\^)",
    )
    .is_match(after)
}

/// Whether the text carries an absolute date: a year-month-day date, a
/// month name with a day or a year, or a four-digit year.
fn has_absolute_date(text: &str) -> bool {
    static MONTH: OnceLock<Regex> = OnceLock::new();
    static ISO: OnceLock<Regex> = OnceLock::new();
    if re(&ISO, r"\b\d{4}-\d{2}-\d{2}\b").is_match(text) {
        return true;
    }
    let month = re(
        &MONTH,
        r"(?i)\b(?:(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)\.?\s+(?:\d{1,2}(?:st|nd|rd|th)?\b|\d{4}\b)|\d{1,2}(?:st|nd|rd|th)?\s+(?:of\s+)?(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)\b)",
    );
    if month.is_match(text) {
        return true;
    }
    tokens(text).iter().any(|t| {
        t.suffix.chars().all(|c| !c.is_alphanumeric())
            && t.prefix.chars().all(|c| !c.is_alphanumeric())
            && t.core.len() == 4
            && t.core
                .parse::<u32>()
                .is_ok_and(|n| (1900..=2100).contains(&n))
    })
}

// ---------------------------------------------------------------------------
// count-word
// ---------------------------------------------------------------------------

/// Spelled numbers from two up. "One" is left out on purpose: "one place"
/// names a single thing, it does not count a list, and "one of" and
/// "each one" are everyday grammar. A later count in the same sentence is
/// still read.
fn is_spelled_count(core: &str) -> bool {
    static SPELLED: OnceLock<Regex> = OnceLock::new();
    re(
        &SPELLED,
        r"^(?:two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|thirteen|fourteen|fifteen|sixteen|seventeen|eighteen|nineteen|(?:twenty|thirty|forty|fifty|sixty|seventy|eighty|ninety)(?:-(?:one|two|three|four|five|six|seven|eight|nine))?)$",
    )
    .is_match(core)
}

/// Digits that read as a count: one to three digits, or comma groups, with
/// no leading zero (a decision number such as 0006 is a name, not a count).
/// A four-digit number that reads as a year is not a count.
fn is_digit_count(core: &str) -> bool {
    static DIGITS: OnceLock<Regex> = OnceLock::new();
    if core.starts_with('0') || !re(&DIGITS, r"^(?:\d{1,3}(?:,\d{3})*|\d{4,})$").is_match(core) {
        return false;
    }
    match core.replace(',', "").parse::<u32>() {
        Ok(n) => n >= 2 && !(1900..=2100).contains(&n),
        Err(_) => true,
    }
}

/// Any number the bound and range checks read, including "one".
fn is_numberish(core: &str) -> bool {
    core == "one" || is_spelled_count(core) || is_digit_count(core) || core == "1"
}

/// Words that cannot sit between a number and the thing it counts.
const STOP: &[&str] = &[
    "a", "an", "the", "of", "in", "on", "at", "to", "for", "with", "by", "from", "and", "or",
    "but", "as", "if", "is", "are", "was", "were", "be", "been", "has", "have", "had", "do",
    "does", "did", "will", "would", "can", "could", "may", "might", "must", "should", "that",
    "which", "who", "whom", "whose", "this", "these", "those", "it", "its", "their", "our", "your",
    "my", "his", "her", "they", "we", "you", "he", "she", "not", "no", "than", "then", "so",
    "into", "onto", "over", "under", "per", "via", "vs", "about", "between", "within", "without",
    "after", "before", "during", "since", "while", "when", "where", "what", "how", "why", "also",
    "only", "just", "very", "more", "most", "less", "least", "much", "many", "few", "some", "any",
    "all", "each", "every", "other", "another", "such", "own", "same", "both", "either", "neither",
    "one", "apart", "together", "away", "out", "up", "down", "back", "off", "already", "still",
    "now", "again", "ago", "first", "second", "third",
];

/// Words that end in "s" and are not plural nouns.
const NOT_PLURAL: &[&str] = &[
    "this",
    "its",
    "has",
    "was",
    "does",
    "yes",
    "always",
    "perhaps",
    "sometimes",
    "across",
    "plus",
    "versus",
    "thus",
    "towards",
    "afterwards",
    "upwards",
    "besides",
    "whereas",
    "whilst",
    "nevertheless",
    "series",
    "species",
    "news",
    "means",
    "thanks",
    "his",
    "hers",
    "ours",
    "theirs",
    "yours",
    "us",
    "as",
    "is",
];

const IRREGULAR_PLURALS: &[&str] = &["people", "children", "men", "women"];

fn is_plural_noun(core: &str) -> bool {
    if core.len() < 3 || !core.chars().all(|c| c.is_ascii_alphabetic() || c == '-') {
        return false;
    }
    if IRREGULAR_PLURALS.contains(&core) {
        return true;
    }
    if NOT_PLURAL.contains(&core) || STOP.contains(&core) {
        return false;
    }
    core.ends_with('s') && !(core.ends_with("ss") || core.ends_with("us") || core.ends_with("is"))
}

/// What a number measures rather than counts: time, size, a limit on text,
/// a rate. A measure is true for as long as it is measured, not as long as
/// a list stays the same.
const MEASURES: &[&str] = &[
    "seconds",
    "minutes",
    "hours",
    "days",
    "weeks",
    "months",
    "years",
    "decades",
    "centuries",
    "milliseconds",
    "microseconds",
    "nanoseconds",
    "bytes",
    "kilobytes",
    "megabytes",
    "gigabytes",
    "terabytes",
    "bits",
    "words",
    "lines",
    "characters",
    "chars",
    "tokens",
    "digits",
    "places",
    "times",
    "points",
    "pixels",
    "degrees",
    "cents",
    "dollars",
    "euros",
    "miles",
    "meters",
    "metres",
    "centimeters",
    "centimetres",
    "millimeters",
    "millimetres",
    "kilometers",
    "kilometres",
    "grams",
    "kilograms",
    "litres",
    "liters",
    "inches",
    "feet",
    "percent",
    "hertz",
    "bps",
];

/// Unit abbreviations that can follow a number: "20 px", "5 s".
const UNITS: &[&str] = &[
    "px",
    "pt",
    "em",
    "rem",
    "ms",
    "s",
    "h",
    "m",
    "kb",
    "mb",
    "gb",
    "tb",
    "hz",
    "fps",
    "dpi",
    "vh",
    "vw",
    "cm",
    "mm",
    "kg",
    "g",
    "x",
    "percent",
    "percentage",
];

/// Words that name a position when they stand before a number: "Phase 2".
const LABELS: &[&str] = &[
    "phase",
    "step",
    "round",
    "part",
    "section",
    "stage",
    "chapter",
    "level",
    "tier",
    "wave",
    "epic",
    "rule",
    "issue",
    "pr",
    "number",
    "no",
    "version",
    "v",
    "release",
    "day",
    "week",
    "month",
    "year",
    "quarter",
    "q",
    "figure",
    "fig",
    "table",
    "appendix",
    "slide",
    "page",
    "row",
    "column",
    "line",
    "item",
    "option",
    "case",
    "group",
    "cohort",
    "batch",
    "track",
    "gate",
    "checkpoint",
    "ticket",
    "bug",
    "adr",
    "axis",
    "attempt",
    "iteration",
    "sprint",
    "milestone",
    "rfc",
    "layer",
];

/// Whether a bound, an estimate or a range sits around the number at `i`.
/// "At least four" stays true when a fifth is added. "At most four" and "up
/// to four" state a limit, not a count. "About four" and "30 to 100" are
/// estimates. None of these goes stale when a list changes.
fn is_bounded(toks: &[Tok<'_>], i: usize) -> bool {
    let at = |n: Option<usize>| n.and_then(|n| toks.get(n));
    let prev = at(i.checked_sub(1));
    let prev2 = at(i.checked_sub(2));
    if let Some(p) = prev {
        let word = p.core.as_str();
        if matches!(
            word,
            "than"
                | "under"
                | "over"
                | "below"
                | "above"
                | "approximately"
                | "roughly"
                | "nearly"
                | "almost"
                | "exceed"
                | "exceeding"
                | "exceeds"
        ) || matches!(
            p.raw,
            "\u{2264}" | "\u{2265}" | "<" | ">" | "~" | "<=" | ">=" | "\u{2248}"
        ) {
            return true;
        }
        // "About 20" is an estimate. "Built around two loops" is not, so a
        // spelled number after "about" or "around" is still read as a count.
        let digits = toks
            .get(i)
            .is_some_and(|t| t.core.starts_with(|c: char| c.is_ascii_digit()));
        if matches!(word, "about" | "around") && digits {
            return true;
        }
        let prev2_word = prev2.map(|t| t.core.as_str());
        if (matches!(word, "most" | "least") && prev2_word == Some("at"))
            || (word == "to" && prev2_word == Some("up"))
            || (matches!(word, "to" | "and" | "or") && prev2_word.is_some_and(is_numberish))
        {
            return true;
        }
    }
    let next = at(Some(i + 1));
    let next2 = at(Some(i + 2));
    next.is_some_and(|n| matches!(n.core.as_str(), "to" | "or"))
        && next2.is_some_and(|n| is_numberish(&n.core))
}

/// Whether the noun at `end` is followed by a bound, as in "15 files or
/// fewer": a threshold, which stays true when a list changes.
fn is_bounded_after(toks: &[Tok<'_>], end: usize) -> bool {
    toks.get(end + 1).is_some_and(|t| t.core == "or")
        && toks.get(end + 2).is_some_and(|t| {
            matches!(
                t.core.as_str(),
                "fewer" | "more" | "less" | "greater" | "higher" | "lower" | "above" | "below"
            )
        })
}

/// Facts that cannot change, so a count of them is safe. Each entry is a
/// fact in maths, physics, or everyday language, named with the words that
/// must appear in the counted phrase or its sentence.
fn is_fixed_fact(number: &str, phrase: &str, sentence: &str) -> bool {
    static BINARY: OnceLock<Regex> = OnceLock::new();
    const PHRASES: &[&str] = &[
        "primary colo",
        "cardinal direction",
        "laws of motion",
        "laws of thermodynamics",
        "states of matter",
    ];
    if PHRASES
        .iter()
        .any(|p| phrase.contains(p) || sentence.contains(p))
    {
        return true;
    }
    let binary = re(&BINARY, r"\b(?:binary|boolean)\b");
    number == "two"
        && binary.is_match(sentence)
        && [
            "states", "values", "digits", "symbols", "outcomes", "choices",
        ]
        .iter()
        .any(|n| phrase.ends_with(n))
}

/// A count word is flagged: a spelled or written number from two up, then
/// up to two ordinary words, then a plural noun, as in "three stages".
///
/// It holds the whole text, so a count in a table cell is excused when a
/// link to a source sits anywhere in the same row, and a count is excused
/// when a citation note follows its sentence.
pub struct CountWordRule<'a> {
    text: &'a str,
    lines: Vec<&'a str>,
}

impl<'a> CountWordRule<'a> {
    pub fn new(text: &'a str) -> Self {
        CountWordRule {
            text,
            lines: text.lines().collect(),
        }
    }

    fn cited(&self, s: &TextUnit) -> bool {
        if cites_a_source(&s.text) || self.text.get(s.span.end..).is_some_and(leads_a_citation) {
            return true;
        }
        s.in_table
            && s.line
                .checked_sub(1)
                .and_then(|n| self.lines.get(n))
                .is_some_and(|row| cites_a_source(row))
    }
}

impl Rule<WritingConfig> for CountWordRule<'_> {
    fn id(&self) -> &'static str {
        "count-word"
    }

    fn scope(&self) -> Scope {
        Scope::Sentence
    }

    fn check(&self, s: &TextUnit, _cfg: &WritingConfig) -> Vec<Finding> {
        if self.cited(s) || has_absolute_date(&without_link_targets(&s.text)) {
            return vec![];
        }
        let text = reduce_inline(&s.text);
        let quotes = quoted_spans(&text);
        let toks = tokens(&text);
        let lower = text.to_lowercase();
        let mut out = Vec::new();
        for (i, tok) in toks.iter().enumerate() {
            if !(is_spelled_count(&tok.core) || is_digit_count(&tok.core)) || !tok.suffix.is_empty()
            {
                continue;
            }
            if !tok
                .prefix
                .chars()
                .all(|c| "([{\"'\u{201c}\u{2018}*_".contains(c))
                || quotes.iter().any(|q| q.contains(&tok.start))
                || is_bounded(&toks, i)
            {
                continue;
            }
            if let Some(prev) = i.checked_sub(1).and_then(|p| toks.get(p)) {
                if LABELS.contains(&prev.core.as_str()) || prev.raw.ends_with('#') {
                    continue;
                }
            }
            let Some(end) = counted_noun(&toks, i) else {
                continue;
            };
            let Some(noun) = toks.get(end) else { continue };
            if MEASURES.contains(&noun.core.as_str()) || is_bounded_after(&toks, end) {
                continue;
            }
            let phrase: Vec<&str> = toks
                .get(i..=end)
                .unwrap_or_default()
                .iter()
                .map(|t| t.core.as_str())
                .collect();
            if is_fixed_fact(&tok.core, &phrase.join(" "), &lower) {
                continue;
            }
            let stop = noun.start + noun.raw.len() - noun.suffix.len();
            let excerpt = text.get(tok.start..stop).unwrap_or(&tok.core);
            out.push(Finding::new(
                "count-word",
                Level::Error,
                s.line,
                COUNT_REMEDIATION.to_string(),
                excerpt.to_string(),
            ));
        }
        out
    }
}

/// The index of the plural noun a number at `i` counts: the first plural
/// noun within three words, with only ordinary words between. A unit, a
/// stop word, or punctuation between the two ends the search.
fn counted_noun(toks: &[Tok<'_>], i: usize) -> Option<usize> {
    for k in 1..=3 {
        let next = toks.get(i + k)?;
        let before = toks.get(i + k - 1)?;
        if (k > 1 && !before.suffix.is_empty()) || !next.prefix.is_empty() {
            return None;
        }
        if UNITS.contains(&next.core.as_str()) {
            return None;
        }
        if is_plural_noun(&next.core) {
            return Some(i + k);
        }
        let plain = !next.core.is_empty()
            && !STOP.contains(&next.core.as_str())
            && !next.core.ends_with("ly")
            && next
                .core
                .chars()
                .all(|c| c.is_ascii_alphabetic() || c == '-');
        if !plain {
            return None;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// relative-time
// ---------------------------------------------------------------------------

const NEGATIONS: &[&str] = &[
    "not",
    "no",
    "nothing",
    "never",
    "none",
    "hasn't",
    "haven't",
    "isn't",
    "aren't",
    "wasn't",
    "weren't",
    "don't",
    "doesn't",
    "didn't",
    "can't",
    "cannot",
    "won't",
    "couldn't",
    "wouldn't",
    "shouldn't",
    "needn't",
    "anything",
    "ever",
    "any",
];

const STILL_COMPARATIVE: &[&str] = &[
    "more", "less", "better", "worse", "further", "another", "other", "greater", "larger",
    "smaller", "fewer", "higher", "lower", "bigger", "longer", "shorter", "harder", "easier",
    "simpler", "faster", "slower", "stronger", "weaker",
];

const STILL_ADJECTIVE_AFTER: &[&str] = &[
    "stand", "stands", "stood", "hold", "holds", "keep", "keeps", "stay", "stays", "remain",
    "remains", "sit", "sits", "lie", "lies", "standing", "a", "an", "the", "water", "image",
    "images", "life",
];

/// Whether the word the match stands for is about time, given the words
/// around it. `before` is the sentence's earlier words, nearest first, and
/// `after` its later words, nearest first. Both are lower case.
fn is_time_sense(word: &str, before: &[String], after: &[String]) -> bool {
    let next = after.first().map(String::as_str);
    let prev = before.first().map(String::as_str);
    match word {
        "now" => {
            let that = next == Some("that");
            let idiom = matches!(
                (next, after.get(1).map(String::as_str)),
                (Some("and"), Some("then" | "again")) | (Some("or"), Some("never"))
            );
            !(that || idiom)
        }
        "to date" => prev != Some("up"),
        "soon" => !(prev == Some("as") || next == Some("as")),
        "yet" => {
            let negated = before
                .iter()
                .take(3)
                .any(|w| NEGATIONS.contains(&w.as_str()));
            let as_yet = prev == Some("as");
            let yet_to = next == Some("to");
            let last = after.is_empty();
            negated || as_yet || yet_to || last
        }
        "still" => {
            let comparative = next.is_some_and(|n| STILL_COMPARATIVE.contains(&n));
            let adjective = prev.is_some_and(|p| STILL_ADJECTIVE_AFTER.contains(&p));
            // "Keep it still." ends on the word: that is the adjective, not a time.
            !(comparative || adjective || after.is_empty())
        }
        _ => true,
    }
}

/// A relative time word is flagged: it means a different time every time
/// someone reads the sentence, so the sentence is true only on the day it
/// was written.
pub fn relative_time(s: &TextUnit, _cfg: &WritingConfig) -> Vec<Finding> {
    static TIME: OnceLock<Regex> = OnceLock::new();
    let pattern = re(
        &TIME,
        r"(?i)\b(?:as of now|at the moment|right now|for now|at present|these days|to date|(?:this|last|next) (?:week|month|year|quarter)|today|tonight|yesterday|tomorrow|currently|recently|soon|lately|nowadays|presently|now|yet|still)\b",
    );
    if has_absolute_date(&without_link_targets(&s.text)) {
        return vec![];
    }
    let text = reduce_inline(&s.text);
    let toks = tokens(&text);
    pattern
        .find_iter(&text)
        .filter(|m| {
            let word = m.as_str().to_lowercase();
            let Some(idx) = toks.iter().position(|t| t.start + t.raw.len() > m.start()) else {
                return true;
            };
            let before: Vec<String> = toks
                .get(..idx)
                .unwrap_or_default()
                .iter()
                .rev()
                .map(|t| t.core.clone())
                .collect();
            let phrase_words = word.split_whitespace().count();
            let after: Vec<String> = toks
                .get(idx + phrase_words..)
                .unwrap_or_default()
                .iter()
                .map(|t| t.core.clone())
                .collect();
            let comma_after = toks
                .get(idx + phrase_words - 1)
                .is_some_and(|t| t.suffix.starts_with(','));
            // A sentence-opening "Now," or "Still," is a connective, not a time.
            if before.is_empty() && comma_after && matches!(word.as_str(), "now" | "still") {
                return false;
            }
            is_time_sense(&word, &before, &after)
        })
        .map(|m| {
            Finding::new(
                "relative-time",
                Level::Warning,
                s.line,
                TIME_REMEDIATION.to_string(),
                m.as_str().to_string(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::config::WritingConfig;
    use crate::lints::writing::lint_writing;
    use crate::lints::{load_known_names, Context};

    fn excerpts(rule: &str, text: &str) -> Vec<String> {
        let known = load_known_names(&[], None).expect("built-in names load");
        lint_writing(
            text,
            &known,
            &WritingConfig::default(),
            Context::Document,
            false,
            false,
        )
        .into_iter()
        .filter(|f| f.rule == rule)
        .map(|f| f.excerpt)
        .collect()
    }

    #[test]
    fn a_count_in_a_table_row_that_links_a_source_is_left_alone() {
        let cited = "| Source | Fact |\n|---|---|\n| [Study](https://example.com/s) | 86 tasks across domains |\n";
        assert!(excerpts("count-word", cited).is_empty());
        let bare = "| Source | Fact |\n|---|---|\n| Study | 86 tasks across domains |\n";
        assert_eq!(excerpts("count-word", bare), vec!["86 tasks"]);
    }

    #[test]
    fn a_decision_number_is_a_name_not_a_count() {
        assert!(excerpts("count-word", "Decision 0006 covers the binary.").is_empty());
    }

    #[test]
    fn a_unit_between_a_number_and_a_noun_ends_the_count() {
        assert!(excerpts("count-word", "A 20 px badge appears here.").is_empty());
    }

    #[test]
    fn a_bound_or_a_range_is_not_a_count() {
        assert!(excerpts("count-word", "There are at least four reasons.").is_empty());
        assert!(excerpts("count-word", "Allow at most three retries.").is_empty());
        assert_eq!(
            excerpts("count-word", "There are four reasons."),
            vec!["four reasons"]
        );
    }

    #[test]
    fn a_threshold_after_the_noun_is_not_a_count() {
        assert!(excerpts("count-word", "With 15 files or fewer, each file has a row.").is_empty());
        assert!(excerpts("count-word", "Review a list of four items or more.").is_empty());
    }

    #[test]
    fn about_a_digit_estimate_is_not_a_count_but_around_a_spelled_number_is() {
        assert!(excerpts("count-word", "It has about 20 children.").is_empty());
        assert_eq!(
            excerpts("count-word", "It is built around two concurrent loops."),
            vec!["two concurrent loops"]
        );
    }

    #[test]
    fn a_dated_record_is_left_alone() {
        assert!(excerpts("count-word", "On 2026-10-03 four consoles passed.").is_empty());
        assert!(excerpts(
            "relative-time",
            "As of 2026-10-03 the cache is currently off."
        )
        .is_empty());
    }

    #[test]
    fn the_word_one_is_never_a_count() {
        assert!(excerpts("count-word", "Pick one of the places, each one is fine.").is_empty());
    }

    #[test]
    fn a_hyphenated_compound_is_not_read() {
        assert!(excerpts("count-word", "A two-factor check guards the login.").is_empty());
    }

    #[test]
    fn a_quoted_figure_is_left_alone() {
        assert!(excerpts("count-word", "The slogan was \"five whys\" in the source.").is_empty());
    }

    #[test]
    fn an_opening_now_with_a_comma_is_not_about_time() {
        assert!(excerpts("relative-time", "Now, run the build.").is_empty());
        assert_eq!(excerpts("relative-time", "Run the build now."), vec!["now"]);
    }

    #[test]
    fn yet_as_a_conjunction_is_not_about_time() {
        assert!(excerpts("relative-time", "It is small yet fast.").is_empty());
        assert_eq!(
            excerpts("relative-time", "It has not shipped yet."),
            vec!["yet"]
        );
    }
}
