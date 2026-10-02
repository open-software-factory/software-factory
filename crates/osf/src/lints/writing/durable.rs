//! Two rules for text that has to stay true after it is written: a count
//! word, such as "three stages", and a relative time word, such as "now".
//! Both read the sentence's words, not a bare pattern over its characters.
//! The caller skips both in a transcript, which is not kept.
//!
//! Every exemption is local. A date, a source and a fixed fact excuse only
//! the count or the time word they sit with, never the rest of the sentence.

use crate::config::WritingConfig;
use osf_lint_core::segment::TextUnit;
use osf_lint_core::{Finding, Level, Rule, Scope};
use regex::{Captures, Regex};
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

impl Tok<'_> {
    /// Where the word's letters end, before any punctuation after them.
    fn end(&self) -> usize {
        self.start + self.raw.len() - self.suffix.len()
    }
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

// ---------------------------------------------------------------------------
// The prepared sentence: dates, links, notes and clauses
// ---------------------------------------------------------------------------

/// Replaces an absolute date in the prepared text, so a clause can tell
/// whether it carries one.
const DATE_MARK: &str = "DATEMARK";

/// The tool-written blocks that are a snapshot of a head commit.
const TOOL_BLOCKS: &[&str] = &["status", "tree", "pr-lens"];

/// A source note: a footnote, a numbered citation, or a bracketed `reported:`
/// or `source:` note.
macro_rules! note_pattern {
    () => {
        r"\[\^[^\]]+\]|\[\d+(?:\s*[,-]\s*\d+)*\]|\[(?:reported|source|sources|cited)\b[^\]]*\]"
    };
}

/// A link or a note in the prepared text, and whether it is a note.
struct Source {
    range: Range<usize>,
    note: bool,
}

/// A sentence with its code spans, link addresses and dates dealt with.
struct Prepared {
    text: String,
    sources: Vec<Source>,
}

/// The text with every absolute date replaced by [`DATE_MARK`]: a year-month-day
/// date, a month name with a day or a year, or a four-digit year.
fn mark_dates(text: &str) -> String {
    static MONTH: OnceLock<Regex> = OnceLock::new();
    static ISO: OnceLock<Regex> = OnceLock::new();
    static YEAR: OnceLock<Regex> = OnceLock::new();
    let iso = re(&ISO, r"\b\d{4}-\d{2}-\d{2}\b").replace_all(text, DATE_MARK);
    let month = re(
        &MONTH,
        r"(?i)\b(?:(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)\.?\s+(?:\d{1,2}(?:st|nd|rd|th)?\b|\d{4}\b)|\d{1,2}(?:st|nd|rd|th)?\s+(?:of\s+)?(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)\b)",
    )
    .replace_all(&iso, DATE_MARK);
    re(&YEAR, r"(^|[^\w-])(?:19\d\d|20\d\d|2100)([^\w-]|$)")
        .replace_all(&month, |c: &Captures| {
            format!("{}{DATE_MARK}{}", &c[1], &c[2])
        })
        .into_owned()
}

/// Whether the text of a code span reads as English words, such as
/// `three stages`, and not as an identifier or a command line.
fn prose_like(code: &str) -> bool {
    let words: Vec<&str> = code.split_whitespace().collect();
    words.len() >= 2
        && words.iter().all(|w| {
            w.starts_with(|c: char| c.is_ascii_alphanumeric())
                && w.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "'-,.".contains(c))
        })
        && words
            .iter()
            .any(|w| w.chars().all(|c| c.is_ascii_alphabetic()))
}

/// The sentence as the rules read it. A code span becomes the word "code",
/// unless it holds a date, or holds prose and `expand_code` is set. A link
/// keeps its text and loses its address, and its text is remembered as a
/// source. A bracketed note is remembered as a source too. Each date becomes
/// [`DATE_MARK`].
fn prepare(raw: &str, expand_code: bool) -> Prepared {
    static CODE: OnceLock<Regex> = OnceLock::new();
    static LINK: OnceLock<Regex> = OnceLock::new();
    static STARS: OnceLock<Regex> = OnceLock::new();
    static UNDERSCORE: OnceLock<Regex> = OnceLock::new();
    static NOTE: OnceLock<Regex> = OnceLock::new();
    let spans = re(&CODE, r"`[^`]*`").replace_all(raw, |c: &Captures| {
        let inner = c[0].trim_matches('`');
        if mark_dates(inner) != inner || (expand_code && prose_like(inner)) {
            inner.to_string()
        } else {
            "code".to_string()
        }
    });
    let linked = re(&LINK, r"\[([^\]]*)\]\([^)]*\)").replace_all(&spans, "\u{1}$1\u{2}");
    let plain = re(&STARS, r"\*+").replace_all(&linked, "");
    let plain = re(&UNDERSCORE, r"(^|[^\w])_+|_+([^\w]|$)").replace_all(&plain, "$1$2");
    let marked = mark_dates(&plain);
    let mut text = String::with_capacity(marked.len());
    let mut sources = Vec::new();
    let mut open = 0;
    for c in marked.chars() {
        match c {
            '\u{1}' => open = text.len(),
            '\u{2}' => sources.push(Source {
                range: open..text.len(),
                note: false,
            }),
            _ => text.push(c),
        }
    }
    let notes = re(&NOTE, concat!("(?i)", note_pattern!()));
    sources.extend(notes.find_iter(&text).map(|m| Source {
        range: m.range(),
        note: true,
    }));
    Prepared { text, sources }
}

/// The clauses of a sentence: a semicolon, a colon, a dash, or a comma
/// before a conjunction ends one. A date excuses only the clause it is in.
fn clause_bounds(text: &str) -> Vec<Range<usize>> {
    static SPLIT: OnceLock<Regex> = OnceLock::new();
    let split = re(
        &SPLIT,
        r"(?i);|:\s|[\u{2014}\u{2013}]|,\s+(?:but|and|or|yet|so|while|whereas|although|though)\b",
    );
    let mut out = Vec::new();
    let mut from = 0;
    for m in split.find_iter(text) {
        out.push(from..m.start());
        from = m.end();
    }
    out.push(from..text.len());
    out
}

fn clause_at(clauses: &[Range<usize>], pos: usize) -> Range<usize> {
    clauses
        .iter()
        .find(|r| r.contains(&pos))
        .or_else(|| clauses.last())
        .cloned()
        .unwrap_or(0..0)
}

/// Whether the clause around `pos` carries an absolute date.
fn clause_has_date(text: &str, clauses: &[Range<usize>], pos: usize) -> bool {
    text.get(clause_at(clauses, pos))
        .is_some_and(|c| c.contains(DATE_MARK))
}

/// Whether a table row links a source or carries a note.
fn row_has_source(row: &str) -> bool {
    static SOURCE: OnceLock<Regex> = OnceLock::new();
    re(&SOURCE, concat!(r"(?i)\]\(https?://|", note_pattern!())).is_match(row)
}

/// Whether the text right after a sentence is a source note that ends its
/// line, as in `[reported: source]`. A note followed by more text on its
/// line belongs to that text, not to the sentence before it.
fn trailing_note(after: &str) -> bool {
    static NOTE: OnceLock<Regex> = OnceLock::new();
    re(
        &NOTE,
        concat!(
            r"(?i)^[ \t]*(?:\r?\n[ \t]*)?(?:",
            note_pattern!(),
            r")[ \t]*(?:\r?\n|$)"
        ),
    )
    .is_match(after)
}

/// The byte ranges of every block a tool wrote between `<!-- osf:NAME:start
/// head=SHA -->` and `<!-- osf:NAME:end -->`, for the names in
/// [`TOOL_BLOCKS`]. The start marker names the head commit the block was
/// written for, so a count or a time word in it is a snapshot of that commit,
/// not a claim for a reader later. A block with any other name is read.
fn tool_blocks(text: &str) -> Vec<Range<usize>> {
    static START: OnceLock<Regex> = OnceLock::new();
    let start = re(
        &START,
        r"<!--\s*osf:([A-Za-z0-9_-]+):start\s+head=\S+\s*-->",
    );
    let mut out = Vec::new();
    for caps in start.captures_iter(text) {
        let (Some(open), Some(name)) = (caps.get(0), caps.get(1)) else {
            continue;
        };
        if !TOOL_BLOCKS.contains(&name.as_str()) {
            continue;
        }
        let close = format!("<!-- osf:{}:end -->", name.as_str());
        let from = open.end();
        if let Some(len) = text.get(from..).and_then(|rest| rest.find(&close)) {
            out.push(from..from + len);
        }
    }
    out
}

/// Whether a sentence sits inside one of the tool-written blocks.
fn inside_a_block(blocks: &[Range<usize>], s: &TextUnit) -> bool {
    blocks.iter().any(|b| b.contains(&s.span.start))
}

// ---------------------------------------------------------------------------
// count-word
// ---------------------------------------------------------------------------

/// Spelled numbers from two up. "One" is read only in a narrow pattern, see
/// [`one_count`].
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
/// a list stays the same. "Points" is not here: it counts a list.
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
    "frames",
];

/// What a policy caps or a run consumes. A bound or a range before one of
/// these is a limit and passes: "at most 3 retries", "up to 20 files".
/// Before any other plural noun a bound is a count, because the list it
/// counts can change and break the bound.
const LIMIT_NOUNS: &[&str] = &[
    "retries",
    "attempts",
    "tries",
    "requests",
    "calls",
    "files",
    "items",
    "rows",
    "entries",
    "results",
    "records",
    "jobs",
    "runs",
    "workers",
    "threads",
    "connections",
    "failures",
    "errors",
    "warnings",
    "findings",
    "comments",
    "messages",
    "events",
    "commits",
    "approvals",
    "approvers",
    "reviewers",
    "reviews",
    "votes",
    "units",
    "nodes",
    "elements",
    "iterations",
    "rounds",
    "uses",
    "cycles",
    "questions",
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

/// Words that name a position when they stand right before a number: "Phase 2".
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

/// Nouns that label a number only when a verb ends the label: "Choice 3
/// works best." They are ordinary nouns before a plural, as in "plan three
/// stages", so they do not label a number that counts something.
const WEAK_LABELS: &[&str] = &[
    "choice",
    "plan",
    "approach",
    "scenario",
    "example",
    "model",
    "design",
    "proposal",
    "variant",
    "alternative",
    "experiment",
    "test",
    "trial",
    "candidate",
    "direction",
    "strategy",
];

/// Third-person verbs that can follow a label and a number.
const VERBS_S: &[&str] = &[
    "works", "fits", "wins", "holds", "applies", "matters", "performs", "passes", "fails", "runs",
    "beats", "loses", "needs", "uses", "shows", "makes", "gives", "takes", "offers", "looks",
    "seems", "stays", "remains", "covers", "handles", "requires", "suits", "serves", "supports",
    "solves", "helps", "becomes", "comes", "goes", "suffices",
];

/// Singular nouns for the things a document lists. "Has one manual gate"
/// counts them. "One place" and "one of" do not.
const ONE_NOUNS: &[&str] = &[
    "stage",
    "gate",
    "step",
    "check",
    "rule",
    "key",
    "tier",
    "layer",
    "phase",
    "family",
    "surface",
    "runner",
    "reviewer",
    "worker",
    "queue",
    "model",
    "agent",
    "harness",
    "provider",
    "adapter",
    "option",
    "mode",
    "state",
    "role",
    "field",
    "column",
    "tab",
    "panel",
    "view",
    "screen",
    "flow",
    "loop",
    "track",
    "service",
    "environment",
    "deployment",
    "product",
    "tool",
    "command",
    "test",
    "reason",
    "requirement",
    "criterion",
    "approach",
    "alternative",
    "owner",
];

/// Words that put a number in a count: "has one", "with one", "needs one".
const ONE_INTRO: &[&str] = &[
    "has", "have", "with", "contains", "includes", "needs", "requires", "uses", "holds", "defines",
    "names", "lists", "adds", "offers",
];

/// The noun after a hyphen that makes a compound a count: "five-step",
/// "3-way". A compound such as "two-factor" is a name and passes.
const HYPHEN_NOUNS: &[&str] = &[
    "step", "stage", "phase", "tier", "layer", "gate", "way", "level", "pass", "part",
];

/// Past-tense verbs that do not end in "ed".
const IRREGULAR_PAST: &[&str] = &[
    "ran",
    "held",
    "built",
    "made",
    "found",
    "wrote",
    "saw",
    "took",
    "gave",
    "chose",
    "sent",
    "kept",
    "left",
    "met",
    "led",
    "lost",
    "won",
    "paid",
    "said",
    "told",
    "knew",
    "began",
    "brought",
    "bought",
    "caught",
    "drew",
    "drove",
    "felt",
    "grew",
    "heard",
    "sold",
    "spent",
    "taught",
    "thought",
    "understood",
];

/// Words that end in "ed" and are not past-tense verbs.
const NOT_PAST: &[&str] = &[
    "speed", "indeed", "exceed", "proceed", "succeed", "hundred", "embed",
];

/// A past-tense verb: it reports an event that is over, so a count after it
/// is a fact about the past and cannot go stale.
fn is_past_verb(word: &str) -> bool {
    IRREGULAR_PAST.contains(&word)
        || (word.len() >= 5 && word.ends_with("ed") && !NOT_PAST.contains(&word))
}

/// Whether an estimate sits around the number at `i`: "about 20", "roughly
/// four". An estimate is not a count.
fn is_estimate(toks: &[Tok<'_>], i: usize) -> bool {
    let Some(p) = i.checked_sub(1).and_then(|n| toks.get(n)) else {
        return false;
    };
    let digits = toks
        .get(i)
        .is_some_and(|t| t.core.starts_with(|c: char| c.is_ascii_digit()));
    matches!(
        p.core.as_str(),
        "approximately" | "roughly" | "nearly" | "almost"
    ) || matches!(p.raw, "\u{2248}" | "~")
        || (matches!(p.core.as_str(), "about" | "around") && digits)
}

/// Whether a bound or a range sits around the number at `i`: "at least
/// four", "up to 20", "more than 10", "30 to 100", "two or three".
fn is_bound(toks: &[Tok<'_>], i: usize) -> bool {
    let at = |n: Option<usize>| n.and_then(|n| toks.get(n));
    let prev = at(i.checked_sub(1));
    let prev2 = at(i.checked_sub(2));
    if let Some(p) = prev {
        let word = p.core.as_str();
        if matches!(
            word,
            "than" | "under" | "over" | "below" | "above" | "exceed" | "exceeding" | "exceeds"
        ) || matches!(p.raw, "\u{2264}" | "\u{2265}" | "<" | ">" | "<=" | ">=")
        {
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
/// fewer".
fn is_bounded_after(toks: &[Tok<'_>], end: usize) -> bool {
    toks.get(end + 1).is_some_and(|t| t.core == "or")
        && toks.get(end + 2).is_some_and(|t| {
            matches!(
                t.core.as_str(),
                "fewer" | "more" | "less" | "greater" | "higher" | "lower" | "above" | "below"
            )
        })
}

/// Whether the text says the count is a limit: "limit of two families", or
/// "two families total".
fn is_stated_limit(toks: &[Tok<'_>], hit: &Hit) -> bool {
    let word = |n: Option<usize>| n.and_then(|n| toks.get(n)).map(|t| t.core.as_str());
    let before = word(hit.first.checked_sub(1));
    let before2 = word(hit.first.checked_sub(2));
    let after = word(Some(hit.noun + 1));
    (before == Some("of")
        && matches!(
            before2,
            Some("limit" | "maximum" | "max" | "minimum" | "min" | "cap" | "budget")
        ))
        || (before == Some("at") && before2 == Some("capped"))
        || matches!(after, Some("total" | "maximum" | "max"))
        || (after == Some("in") && word(Some(hit.noun + 2)) == Some("total"))
}

/// Facts that cannot change, so a count of them is safe: maths, geometry,
/// physics, the calendar and everyday language. Only the counted phrase is
/// excused. `phrase` runs from the number to the noun, `extended` adds an
/// "of" and the word after it, and `clause` is the clause around them.
fn is_fixed_fact(number: &str, noun: &str, phrase: &str, extended: &str, clause: &str) -> bool {
    static BINARY: OnceLock<Regex> = OnceLock::new();
    static SHAPE: OnceLock<Regex> = OnceLock::new();
    const PHRASES: &[&str] = &[
        "primary colo",
        "cardinal direction",
        "laws of motion",
        "laws of thermodynamics",
        "states of matter",
        "pair of keys",
        "seasons",
        "continents",
        "hemispheres",
        "quadrants",
    ];
    const SHAPE_PARTS: &[&str] = &["sides", "angles", "corners", "vertices", "edges", "faces"];
    const PAIRS: &[&str] = &[
        "directions",
        "ways",
        "sides",
        "ends",
        "axes",
        "hands",
        "eyes",
    ];
    if PHRASES.iter().any(|p| extended.contains(p)) || (number == "both" && PAIRS.contains(&noun)) {
        return true;
    }
    let shape = re(
        &SHAPE,
        r"\b(?:triangle|square|rectangle|rhombus|parallelogram|trapezoid|trapezium|quadrilateral|pentagon|hexagon|octagon|polygon|cube|cuboid|tetrahedron)s?\b",
    );
    if SHAPE_PARTS.contains(&noun) && shape.is_match(clause) {
        return true;
    }
    let binary = re(&BINARY, r"\b(?:binary|boolean)\b");
    number == "two"
        && binary.is_match(clause)
        && [
            "states", "values", "digits", "symbols", "outcomes", "choices",
        ]
        .iter()
        .any(|n| phrase.ends_with(n))
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// A number then a noun, such as "three stages".
    Number,
    /// "A dozen", "both", "a pair of" or "one" then a noun.
    Other,
    /// A compound such as "five-step".
    Hyphen,
}

/// A count the rule found: the token it starts at, the token that holds the
/// number, and the token of the noun.
struct Hit {
    first: usize,
    num: usize,
    noun: usize,
    kind: Kind,
}

fn prefix_ok(tok: &Tok<'_>) -> bool {
    tok.prefix
        .chars()
        .all(|c| "([{\"'\u{201c}\u{2018}*_".contains(c))
}

/// Whether the word before a number labels it: "Phase 2", "#3". A label
/// labels the number only when no punctuation sits between them, so "At
/// this stage, three reviewers approve" counts reviewers. A weak label
/// ("Choice 3 works") labels the number only when a verb follows it.
fn labels_the_number(toks: &[Tok<'_>], i: usize, prev: &Tok<'_>) -> bool {
    if prev.raw.ends_with('#') {
        return true;
    }
    if !prev.suffix.is_empty() {
        return false;
    }
    let word = prev.core.as_str();
    LABELS.contains(&word)
        || (WEAK_LABELS.contains(&word)
            && toks
                .get(i + 1)
                .is_some_and(|n| VERBS_S.contains(&n.core.as_str())))
}

/// A compound such as "five-step" or "3-way".
fn hyphen_count(tok: &Tok<'_>, i: usize) -> Option<Hit> {
    let (number, noun) = tok.core.split_once('-')?;
    (prefix_ok(tok)
        && (is_spelled_count(number) || is_digit_count(number))
        && HYPHEN_NOUNS.contains(&noun))
    .then_some(Hit {
        first: i,
        num: i,
        noun: i,
        kind: Kind::Hyphen,
    })
}

/// "One" counts a list-like noun after a word that introduces a count, as in
/// "has one manual gate". It is not a count in "one of", "each one" or "one
/// place".
fn one_count(toks: &[Tok<'_>], i: usize, prev: Option<&Tok<'_>>) -> Option<Hit> {
    if !prev.is_some_and(|p| ONE_INTRO.contains(&p.core.as_str())) {
        return None;
    }
    for k in 1..=2 {
        let next = toks.get(i + k)?;
        let before = toks.get(i + k - 1)?;
        if (k > 1 && !before.suffix.is_empty()) || !next.prefix.is_empty() {
            return None;
        }
        if ONE_NOUNS.contains(&next.core.as_str()) {
            return Some(Hit {
                first: i,
                num: i,
                noun: i + k,
                kind: Kind::Other,
            });
        }
        if STOP.contains(&next.core.as_str()) || !is_plain_word(&next.core) {
            return None;
        }
    }
    None
}

fn is_plain_word(core: &str) -> bool {
    !core.is_empty() && core.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
}

/// A "both" that floats after its subject, as in "they both carry" or "ACP
/// and AHP both carry", is not a count.
fn is_floating_both(toks: &[Tok<'_>], i: usize) -> bool {
    let Some(prev) = i.checked_sub(1).and_then(|n| toks.get(n)) else {
        return false;
    };
    let pronoun = matches!(
        prev.core.as_str(),
        "they" | "we" | "you" | "these" | "those" | "them" | "us" | "it"
    );
    let name = i > 1
        && prev
            .raw
            .chars()
            .find(|c| c.is_alphabetic())
            .is_some_and(char::is_uppercase);
    pronoun || name
}

/// The count that starts at the token `i`, if any.
fn find_count(toks: &[Tok<'_>], i: usize) -> Option<Hit> {
    let tok = toks.get(i)?;
    if let Some(hit) = hyphen_count(tok, i) {
        return Some(hit);
    }
    if !prefix_ok(tok) || !tok.suffix.is_empty() {
        return None;
    }
    let prev = i.checked_sub(1).and_then(|p| toks.get(p));
    let other = |first: usize, noun: Option<usize>| {
        noun.map(|noun| Hit {
            first,
            num: i,
            noun,
            kind: Kind::Other,
        })
    };
    match tok.core.as_str() {
        "dozen" if prev.is_some_and(|p| p.core == "a") => {
            other(i.saturating_sub(1), counted_noun(toks, i))
        }
        "pair"
            if prev.is_some_and(|p| p.core == "a")
                && toks.get(i + 1).is_some_and(|n| n.core == "of") =>
        {
            other(i.saturating_sub(1), counted_noun(toks, i + 1))
        }
        "both" if !is_floating_both(toks, i) => other(i, counted_noun(toks, i)),
        "one" => one_count(toks, i, prev),
        core if is_spelled_count(core) || is_digit_count(core) => {
            if prev.is_some_and(|p| labels_the_number(toks, i, p)) {
                return None;
            }
            counted_noun(toks, i).map(|noun| Hit {
                first: i,
                num: i,
                noun,
                kind: Kind::Number,
            })
        }
        _ => None,
    }
}

fn is_adverb(core: &str) -> bool {
    const NOT_ADVERBS: &[&str] = &["family", "supply", "assembly", "reply", "anomaly"];
    core.len() > 4 && core.ends_with("ly") && !NOT_ADVERBS.contains(&core)
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
            // A plural right after an adverb is a verb: "80 universally predicts".
            return (!(k > 1 && is_adverb(&before.core))).then_some(i + k);
        }
        if STOP.contains(&next.core.as_str()) || !is_plain_word(&next.core) {
            return None;
        }
    }
    None
}

/// A count word is flagged: a spelled or written number from two up, then
/// up to two ordinary words, then a plural noun, as in "three stages". The
/// shapes "a dozen", "both", "a pair of", a compound such as "five-step", and
/// "one" before a list-like noun are counts too.
///
/// It holds the whole text, so a note after a sentence can source it. Every
/// exemption is local to the count it covers.
pub struct CountWordRule<'a> {
    text: &'a str,
    lines: Vec<&'a str>,
    blocks: Vec<Range<usize>>,
}

/// One sentence, prepared, with what is needed to excuse a count in it.
struct Scan<'a> {
    prep: &'a Prepared,
    toks: &'a [Tok<'a>],
    clauses: Vec<Range<usize>>,
    quotes: Vec<Range<usize>>,
    /// A source note ends the line right after the sentence.
    trailing_note: bool,
    /// The table row of the sentence links a source.
    row_sourced: bool,
}

impl Scan<'_> {
    /// Whether a link or a note directly sources the count: the count sits
    /// inside the link text, or the link or note sits right next to it.
    fn sourced(&self, hit: &Hit) -> bool {
        let (Some(first), Some(noun)) = (self.toks.get(hit.first), self.toks.get(hit.noun)) else {
            return false;
        };
        let text = self.prep.text.as_str();
        let num_at = self.toks.get(hit.num).map_or(first.start, |t| t.start);
        let only = |gap: Option<&str>, allowed: &str| {
            gap.is_some_and(|g| g.chars().all(|c| c.is_whitespace() || allowed.contains(c)))
        };
        self.prep.sources.iter().any(|src| {
            src.range.contains(&num_at)
                || src.range.contains(&noun.start)
                || (src.range.start >= noun.end()
                    && only(text.get(noun.end()..src.range.start), "(),.:;"))
                || (!src.note
                    && src.range.end <= first.start
                    && only(text.get(src.range.end..first.start), "(),:"))
        })
    }

    /// Whether the count sits in a table cell that opens with digits, such
    /// as "11 products", in a row that links a source.
    fn sourced_figure(&self) -> bool {
        self.row_sourced && self.toks.first().is_some_and(|t| is_digit_count(&t.core))
    }

    fn phrase(&self, hit: &Hit) -> (String, String) {
        let words = |end: usize| {
            self.toks
                .get(hit.first..=end)
                .unwrap_or_default()
                .iter()
                .map(|t| t.core.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        };
        let phrase = words(hit.noun);
        let of = self.toks.get(hit.noun + 1).is_some_and(|t| t.core == "of")
            && self.toks.get(hit.noun + 2).is_some();
        let extended = if of {
            words(hit.noun + 2)
        } else {
            phrase.clone()
        };
        (phrase, extended)
    }

    /// Whether anything excuses the count.
    fn excused(&self, hit: &Hit) -> bool {
        let (Some(first), Some(noun)) = (self.toks.get(hit.first), self.toks.get(hit.noun)) else {
            return true;
        };
        let text = self.prep.text.as_str();
        if self.quotes.iter().any(|q| q.contains(&first.start))
            || clause_has_date(text, &self.clauses, first.start)
            || self.sourced(hit)
            || self.sourced_figure()
        {
            return true;
        }
        let clause = clause_at(&self.clauses, first.start);
        if self.trailing_note && self.clauses.last() == Some(&clause) {
            return true;
        }
        let past = hit
            .first
            .checked_sub(1)
            .and_then(|p| self.toks.get(p))
            .is_some_and(|p| is_past_verb(&p.core));
        if past || (hit.kind == Kind::Number && is_estimate(self.toks, hit.num)) {
            return true;
        }
        let noun_core = noun.core.as_str();
        let number = self.toks.get(hit.num).map_or("", |t| t.core.as_str());
        // "41 points" is a score. "Three points" counts a list.
        let score = noun_core == "points" && number.starts_with(|c: char| c.is_ascii_digit());
        if hit.kind != Kind::Hyphen
            && (MEASURES.contains(&noun_core)
                || score
                || is_stated_limit(self.toks, hit)
                || (LIMIT_NOUNS.contains(&noun_core)
                    && (is_bound(self.toks, hit.num) || is_bounded_after(self.toks, hit.noun))))
        {
            return true;
        }
        let (phrase, extended) = self.phrase(hit);
        let clause_text = text.get(clause).unwrap_or_default().to_lowercase();
        is_fixed_fact(number, noun_core, &phrase, &extended, &clause_text)
    }
}

impl<'a> CountWordRule<'a> {
    pub fn new(text: &'a str) -> Self {
        CountWordRule {
            text,
            lines: text.lines().collect(),
            blocks: tool_blocks(text),
        }
    }

    fn row_sourced(&self, s: &TextUnit) -> bool {
        s.in_table
            && s.line
                .checked_sub(1)
                .and_then(|n| self.lines.get(n))
                .is_some_and(|row| row_has_source(row))
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
        if inside_a_block(&self.blocks, s) {
            return vec![];
        }
        let prep = prepare(&s.text, true);
        let toks = tokens(&prep.text);
        let scan = Scan {
            prep: &prep,
            toks: &toks,
            clauses: clause_bounds(&prep.text),
            quotes: quoted_spans(&prep.text),
            trailing_note: self.text.get(s.span.end..).is_some_and(trailing_note),
            row_sourced: self.row_sourced(s),
        };
        (0..toks.len())
            .filter_map(|i| find_count(&toks, i))
            .filter(|hit| !scan.excused(hit))
            .filter_map(|hit| {
                let first = toks.get(hit.first)?;
                let noun = toks.get(hit.noun)?;
                let excerpt = prep.text.get(first.start..noun.end())?;
                Some(Finding::new(
                    "count-word",
                    Level::Error,
                    s.line,
                    COUNT_REMEDIATION.to_string(),
                    excerpt.to_string(),
                ))
            })
            .collect()
    }
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

const MODALS: &[&str] = &["may", "might", "could", "can", "will", "would", "should"];

const COPULAS: &[&str] = &["is", "are", "was", "were", "be", "been", "being", "am"];

const STILL_COMPARATIVE: &[&str] = &[
    "more", "less", "better", "worse", "further", "another", "other", "greater", "larger",
    "smaller", "fewer", "higher", "lower", "bigger", "longer", "shorter", "harder", "easier",
    "simpler", "faster", "slower", "stronger", "weaker",
];

/// Words before "still" that make it the adjective, as in "keep still".
const STILL_ADJECTIVE_AFTER: &[&str] = &[
    "stand", "stands", "stood", "hold", "holds", "keep", "keeps", "stay", "stays", "remain",
    "remains", "sit", "sits", "lie", "lies", "standing", "a", "an", "the", "water", "image",
    "images", "life",
];

/// Words after "still" that make it the adjective, as in "still images".
const STILL_ADJECTIVE_BEFORE: &[&str] = &[
    "image",
    "images",
    "photo",
    "photos",
    "photograph",
    "photographs",
    "picture",
    "pictures",
    "frame",
    "frames",
    "shot",
    "shots",
    "life",
    "water",
    "waters",
    "air",
    "wine",
    "pond",
];

/// Words after a sentence-opening "Now" that make it a discourse opener:
/// "Now we turn to installation."
const NOW_OPENER_NEXT: &[&str] = &[
    "we", "let", "let's", "lets", "i", "you", "consider", "look", "see", "read", "run", "turn",
    "move", "go", "take", "open", "add", "check", "install",
];

/// A third-person present verb, as in "reports": it ends in "s" and is not a
/// plural noun's look-alike such as "always".
fn is_present_verb(word: &str) -> bool {
    word.len() >= 3
        && word.ends_with('s')
        && !(word.ends_with("ss") || word.ends_with("us") || word.ends_with("is"))
        && !NOT_PLURAL.contains(&word)
        && !STOP.contains(&word)
}

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
            // "Never retries yet reports": the word joins two clauses.
            let joins_clauses =
                negated && prev.is_some_and(is_present_verb) && next.is_some_and(is_present_verb);
            let as_yet = prev == Some("as");
            let yet_to = next == Some("to");
            let modal = prev.is_some_and(|p| MODALS.contains(&p));
            let last = after.is_empty();
            (negated && !joins_clauses) || as_yet || yet_to || modal || last
        }
        "still" => {
            // "Is still slower than" says the state continues.
            let continuing = prev.is_some_and(|p| COPULAS.contains(&p));
            let comparative = !continuing && next.is_some_and(|n| STILL_COMPARATIVE.contains(&n));
            let adjective = prev.is_some_and(|p| STILL_ADJECTIVE_AFTER.contains(&p))
                || next.is_some_and(|n| STILL_ADJECTIVE_BEFORE.contains(&n));
            // "Keep it still." ends on the word: that is the adjective, not a time.
            !(comparative || adjective || after.is_empty())
        }
        _ => true,
    }
}

/// A relative time word is flagged: it means a different time every time
/// someone reads the sentence, so the sentence is true only on the day it
/// was written. A block a tool wrote for a named head commit is skipped.
pub struct RelativeTimeRule {
    blocks: Vec<Range<usize>>,
}

impl RelativeTimeRule {
    pub fn new(text: &str) -> Self {
        RelativeTimeRule {
            blocks: tool_blocks(text),
        }
    }
}

impl Rule<WritingConfig> for RelativeTimeRule {
    fn id(&self) -> &'static str {
        "relative-time"
    }

    fn scope(&self) -> Scope {
        Scope::Sentence
    }

    fn check(&self, s: &TextUnit, _cfg: &WritingConfig) -> Vec<Finding> {
        if inside_a_block(&self.blocks, s) {
            return vec![];
        }
        relative_time(s)
    }
}

/// Whether a match at token `idx` opens its sentence as a connective, not a
/// time: "Now, run the build.", "Now we turn to installation.", "Still, it
/// works."
fn is_opener(toks: &[Tok<'_>], idx: usize, word: &str, after: &[String]) -> bool {
    let opens = toks
        .get(..idx)
        .is_some_and(|b| b.iter().all(|t| t.core.is_empty()));
    let comma_after = toks.get(idx).is_some_and(|t| t.suffix.starts_with(','));
    opens
        && ((comma_after && matches!(word, "now" | "still"))
            || (word == "now"
                && after
                    .first()
                    .is_some_and(|n| NOW_OPENER_NEXT.contains(&n.as_str()))))
}

fn relative_time(s: &TextUnit) -> Vec<Finding> {
    static TIME: OnceLock<Regex> = OnceLock::new();
    let pattern = re(
        &TIME,
        r"(?i)\b(?:as of now|at the moment|right now|for now|at present|these days|to date|(?:this|last|next) (?:week|month|year|quarter)|today|tonight|yesterday|tomorrow|currently|recently|soon|lately|nowadays|presently|now|yet|still)\b",
    );
    let prep = prepare(&s.text, false);
    let text = prep.text.as_str();
    let toks = tokens(text);
    let clauses = clause_bounds(text);
    pattern
        .find_iter(text)
        .filter(|m| {
            if clause_has_date(text, &clauses, m.start()) {
                return false;
            }
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
            let last_word = idx + phrase_words - 1;
            if is_opener(&toks, last_word, &word, &after) && phrase_words == 1 {
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

    fn counts(text: &str) -> Vec<String> {
        excerpts("count-word", text)
    }

    fn times(text: &str) -> Vec<String> {
        excerpts("relative-time", text)
    }

    #[test]
    fn a_count_in_a_table_row_that_links_a_source_is_left_alone() {
        let cited = "| Source | Fact |\n|---|---|\n| [Study](https://example.com/s) | 86 tasks across domains |\n";
        assert!(counts(cited).is_empty());
        let bare = "| Source | Fact |\n|---|---|\n| Study | 86 tasks across domains |\n";
        assert_eq!(counts(bare), vec!["86 tasks"]);
    }

    #[test]
    fn only_a_figure_cell_in_a_row_with_a_source_link_is_excused() {
        let head = "| Source | Fact |\n|---|---|\n";
        let label =
            format!("{head}| [Authentication](https://example.com/auth) | Three stages |\n");
        assert_eq!(counts(&label), vec!["Three stages"]);
        let figure = format!("{head}| [Study](https://example.com/s) | 11 products |\n");
        assert!(counts(&figure).is_empty());
        let sentence = format!("{head}| [Study](https://example.com/s) | Covers 11 products |\n");
        assert_eq!(counts(&sentence), vec!["11 products"]);
    }

    #[test]
    fn a_digit_cell_in_a_row_with_a_source_link_is_excused_whole() {
        let head = "| Source | Fact |\n|---|---|\n";
        let cell = format!(
            "{head}| [Study](https://example.com/s) | 86 tasks across 11 domains and 7,308 runs |\n"
        );
        assert!(counts(&cell).is_empty());
    }

    #[test]
    fn a_stated_limit_passes_but_a_bare_count_does_not() {
        assert!(counts("Use two families total.").is_empty());
        assert!(counts("The limit of two families holds.").is_empty());
        assert!(counts("Keep a maximum of three stages.").is_empty());
        assert!(counts("Accent has at most 2 uses per screen.").is_empty());
        assert_eq!(counts("Use two families."), vec!["two families"]);
    }

    #[test]
    fn a_score_in_points_is_a_measure_but_spelled_points_count_a_list() {
        assert!(counts("The score is 74 points.").is_empty());
        assert_eq!(
            counts("The proposal has three points."),
            vec!["three points"]
        );
    }

    #[test]
    fn a_natural_pair_is_a_fixed_fact() {
        assert!(counts("The link works in both directions.").is_empty());
        assert!(counts("Pan on both axes.").is_empty());
        assert_eq!(counts("It supports both themes."), vec!["both themes"]);
    }

    #[test]
    fn a_floating_both_is_not_a_count() {
        assert!(counts("ACP and AHP both carry permissions.").is_empty());
        assert!(counts("They both carry permissions.").is_empty());
        assert_eq!(
            counts("Carry both adaptations forward."),
            vec!["both adaptations"]
        );
    }

    #[test]
    fn a_verb_after_an_adverb_is_not_a_counted_noun() {
        assert!(counts("No evidence shows that 80 universally predicts success.").is_empty());
        assert!(counts("A limit of three stages and a minimum of two gates hold.").is_empty());
    }

    #[test]
    fn a_decision_number_is_a_name_not_a_count() {
        assert!(counts("Decision 0006 covers the binary.").is_empty());
    }

    #[test]
    fn a_unit_between_a_number_and_a_noun_ends_the_count() {
        assert!(counts("A 20 px badge appears here.").is_empty());
    }

    #[test]
    fn a_limit_is_not_a_count_but_a_bound_on_a_list_is() {
        assert!(counts("Allow at most three retries.").is_empty());
        assert!(counts("A run reads up to 20 files per run.").is_empty());
        assert!(counts("Any list with more than 10 items gets a search.").is_empty());
        assert!(counts("With 15 files or fewer, each file has a row.").is_empty());
        assert_eq!(counts("There are four reasons."), vec!["four reasons"]);
    }

    #[test]
    fn a_bound_or_a_range_on_a_list_is_a_count() {
        assert_eq!(
            counts("The pipeline has at least three stages."),
            vec!["three stages"]
        );
        assert_eq!(
            counts("The pipeline has three to five stages."),
            vec!["five stages"]
        );
        assert_eq!(
            counts("The pipeline has two or three stages."),
            vec!["three stages"]
        );
        assert_eq!(
            counts("A review needs at least two reviewers, and no more than four."),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_block_a_tool_wrote_for_a_head_commit_is_left_alone() {
        let text = "<!-- osf:tree:start head=abc123 -->\nThe tree holds 65 files and is final now.\n<!-- osf:tree:end -->\n\nThe change adds 65 files and is final now.\n";
        assert_eq!(counts(text), vec!["65 files"]);
        assert_eq!(times(text), vec!["now"]);
    }

    #[test]
    fn a_block_without_a_head_commit_is_still_read() {
        let text = "<!-- osf:tree:start -->\nThe tree holds 65 files.\n<!-- osf:tree:end -->\n";
        assert_eq!(counts(text), vec!["65 files"]);
    }

    #[test]
    fn only_the_blocks_the_tools_write_are_skipped() {
        let custom = "<!-- osf:custom:start head=abc -->\nThe deployment currently has three active runners.\n<!-- osf:custom:end -->\n";
        assert_eq!(counts(custom), vec!["three active runners"]);
        assert_eq!(times(custom), vec!["currently"]);
        for name in ["status", "tree", "pr-lens"] {
            let text = format!(
                "<!-- osf:{name}:start head=abc -->\nThe deployment currently has three active runners.\n<!-- osf:{name}:end -->\n"
            );
            assert!(counts(&text).is_empty(), "{name}");
            assert!(times(&text).is_empty(), "{name}");
        }
    }

    #[test]
    fn about_a_digit_estimate_is_not_a_count_but_around_a_spelled_number_is() {
        assert!(counts("It has about 20 children.").is_empty());
        assert_eq!(
            counts("It is built around two concurrent loops."),
            vec!["two concurrent loops"]
        );
    }

    #[test]
    fn a_dated_record_is_left_alone() {
        assert!(counts("On 2026-10-03 four consoles passed.").is_empty());
        assert!(times("As of 2026-10-03 the cache is currently off.").is_empty());
    }

    #[test]
    fn a_date_excuses_only_the_clause_it_is_in() {
        let text = "The 2026 roadmap is archived, but the pipeline currently has three stages.";
        assert_eq!(counts(text), vec!["three stages"]);
        assert_eq!(times(text), vec!["currently"]);
        let one = "The 2026 roadmap, with three stages, is archived.";
        assert!(counts(one).is_empty());
        let after = "The pipeline has three stages; the 2026 roadmap is archived.";
        assert_eq!(counts(after), vec!["three stages"]);
    }

    #[test]
    fn a_link_excuses_only_the_count_it_sources() {
        let far = "The pipeline has three stages, with setup covered by [the authentication guide](https://example.com/auth).";
        assert_eq!(counts(far), vec!["three stages"]);
        let inside = "The pipeline has [three stages](https://example.com/auth).";
        assert!(counts(inside).is_empty());
        let next = "The pipeline has three stages ([guide](https://example.com/auth)).";
        assert!(counts(next).is_empty());
        let before = "Per [the guide](https://example.com/auth), three stages run.";
        assert!(counts(before).is_empty());
    }

    #[test]
    fn a_source_note_excuses_a_sentence_only_when_it_ends_its_line() {
        let more = "The pipeline has three stages. [Source: authentication guide] Token setup is documented separately for users.\n";
        assert_eq!(counts(more), vec!["three stages"]);
        let ends = "The pipeline has three stages. [Source: authentication guide]\n";
        assert!(counts(ends).is_empty());
        let inline = "The pipeline has three stages [1] and more.";
        assert!(counts(inline).is_empty());
        let far = "The pipeline has three stages and runs daily with care [1].";
        assert_eq!(counts(far), vec!["three stages"]);
    }

    #[test]
    fn a_fixed_fact_excuses_only_its_own_phrase() {
        let text = "The guide explains three primary colors and supports seven plugins.";
        assert_eq!(counts(text), vec!["seven plugins"]);
        let swapped = "The guide supports seven plugins and explains three primary colors.";
        assert_eq!(counts(swapped), vec!["seven plugins"]);
        assert!(counts("The three primary colours are red, yellow and blue.").is_empty());
        assert!(counts("The three laws of motion apply here.").is_empty());
    }

    #[test]
    fn geometry_and_calendar_facts_are_fixed() {
        assert!(counts("A triangle has three sides.").is_empty());
        assert!(counts("A cube has six faces.").is_empty());
        assert!(counts("The year has four seasons.").is_empty());
        assert!(counts("A pair of keys signs and verifies.").is_empty());
        assert_eq!(counts("The tool has three sides."), vec!["three sides"]);
    }

    #[test]
    fn a_past_event_is_a_fact() {
        let text = "The committee interviewed three candidates before choosing Lee.";
        assert!(counts(text).is_empty());
        assert_eq!(
            counts("The committee interviews three candidates."),
            vec!["three candidates"]
        );
    }

    #[test]
    fn a_label_labels_only_the_number_next_to_it() {
        assert!(counts("Choice 3 works best.").is_empty());
        assert!(counts("Phase 2 tasks run after Layer 3 builds.").is_empty());
        assert_eq!(
            counts("At this stage, three reviewers approve the release."),
            vec!["three reviewers"]
        );
        assert_eq!(counts("We plan three stages."), vec!["three stages"]);
    }

    #[test]
    fn the_word_one_counts_a_list_like_noun_after_a_determiner_pattern() {
        assert_eq!(
            counts("The pipeline has one manual gate."),
            vec!["one manual gate"]
        );
        assert!(counts("Pick one of the places, each one is fine.").is_empty());
        assert!(counts("Keep the rules in one place.").is_empty());
    }

    #[test]
    fn other_count_shapes_are_counts() {
        assert_eq!(
            counts("The release invokes a dozen checks."),
            vec!["a dozen checks"]
        );
        assert_eq!(
            counts("Both validation stages run before deployment."),
            vec!["Both validation stages"]
        );
        assert_eq!(
            counts("The pipeline uses a pair of validation stages."),
            vec!["a pair of validation stages"]
        );
        assert_eq!(
            counts("The pipeline uses a five-step process."),
            vec!["five-step"]
        );
        assert_eq!(counts("The dispatcher uses 3-way routing."), vec!["3-way"]);
        assert_eq!(counts("It is a three-stage."), vec!["three-stage"]);
        assert_eq!(
            counts("The pipeline has three independently maintained stages."),
            vec!["three independently maintained stages"]
        );
        assert_eq!(
            counts("The proposal has three points."),
            vec!["three points"]
        );
    }

    #[test]
    fn a_code_span_is_read_only_when_it_holds_prose() {
        assert_eq!(
            counts("The pipeline has `three stages`."),
            vec!["three stages"]
        );
        assert!(counts("Call `three_stages` and `get_two_items()`.").is_empty());
        assert!(counts("Run `head -n 20 files` first.").is_empty());
    }

    #[test]
    fn a_hyphenated_name_is_not_a_count() {
        assert!(counts("A two-factor check guards the login.").is_empty());
    }

    #[test]
    fn a_quoted_figure_is_left_alone() {
        assert!(counts("The slogan was \"five whys\" in the source.").is_empty());
    }

    #[test]
    fn an_opening_now_is_not_about_time() {
        assert!(times("Now, run the build.").is_empty());
        assert!(times("Now we turn to installation.").is_empty());
        assert!(times("Now, we turn to installation.").is_empty());
        assert_eq!(times("Run the build now."), vec!["now"]);
        assert_eq!(times("Now the cache is warm."), vec!["Now"]);
    }

    #[test]
    fn yet_as_a_conjunction_is_not_about_time() {
        assert!(times("It is small yet fast.").is_empty());
        assert!(times("The service never retries yet reports every failure.").is_empty());
        assert_eq!(times("It has not shipped yet."), vec!["yet"]);
        assert_eq!(times("It may yet succeed."), vec!["yet"]);
    }

    #[test]
    fn still_is_time_only_when_it_means_continuing() {
        assert!(times("The camera captures still images.").is_empty());
        assert_eq!(times("The server still accepts requests."), vec!["still"]);
        assert_eq!(
            times("The new build is still slower than the old one."),
            vec!["still"]
        );
        assert!(times("Keep the pointer still.").is_empty());
    }

    #[test]
    fn a_date_excuses_a_time_word_only_in_its_clause() {
        let text = "The 2026 roadmap is archived, but the cache is still read.";
        assert_eq!(times(text), vec!["still"]);
        assert!(times("The format was frozen in 2025 and is still read.").is_empty());
    }
}
