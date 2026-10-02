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
pub(super) const NON_LABEL_WORDS: &[&str] = &[
    "a", "an", "the", "at", "in", "on", "of", "by", "for", "with", "from", "since", "until",
    "during", "before", "after", "between", "about", "around", "near", "past", "over", "under",
    "than", "to", "is", "was", "were", "are", "and", "or", "but", "this", "that", "these", "those",
    "it", "he", "she", "they", "we", "you", "him", "her", "them", "his", "its", "their", "your",
    "my", "our",
];

/// A unit that follows a number: size, time, data, angle, frequency or share. The number is a quantity.
const UNIT_WORDS: &[&str] = &[
    "px", "pt", "em", "rem", "vh", "vw", "dp", "dpi", "pixel", "pixels", "ms", "s", "sec", "secs",
    "second", "seconds", "min", "mins", "minute", "minutes", "h", "hr", "hrs", "hour", "hours",
    "day", "days", "week", "weeks", "month", "months", "year", "years", "b", "kb", "mb", "gb",
    "tb", "kib", "mib", "gib", "byte", "bytes", "hz", "khz", "mhz", "ghz", "fps", "deg", "degree",
    "degrees", "percent",
];

/// Words that bring a number in as an amount, a bound or an approximation, never a label.
const QUANTITY_WORDS: &[&str] = &[
    "about",
    "approximately",
    "around",
    "roughly",
    "nearly",
    "almost",
    "circa",
    "least",
    "most",
    "more",
    "less",
    "fewer",
    "beyond",
    "within",
    "across",
    "exceed",
    "exceeds",
    "exceeding",
    "plus",
    "minus",
    "only",
    "just",
    "all",
    "both",
    "each",
    "every",
    "total",
    "totalling",
    "totaling",
    "spanning",
    "cover",
    "covers",
    "covering",
];

/// Names of a measured property, so the number after one is its value.
const DIMENSION_WORDS: &[&str] = &[
    "size",
    "width",
    "height",
    "depth",
    "length",
    "scale",
    "zoom",
    "opacity",
    "weight",
    "radius",
    "padding",
    "margin",
    "gap",
    "duration",
    "delay",
    "speed",
    "rate",
    "offset",
    "limit",
    "threshold",
    "count",
    "score",
];

/// A word that says the number after it is a release number. `version` is judged apart, since a version needs a product to be placed.
const VERSION_WORDS: &[&str] = &["ver", "v"];

/// Nouns that label one numbered thing, such as `Layer 2`. A count noun or a unit after the number never cancels one.
const LABEL_NOUNS: &[&str] = &[
    "step",
    "phase",
    "layer",
    "decision",
    "item",
    "option",
    "part",
    "point",
    "issue",
    "fix",
    "milestone",
    "task",
    "round",
    "track",
    "control",
    "stage",
    "wave",
    "sprint",
    "epic",
    "epoch",
    "case",
    "rule",
    "ticket",
    "bug",
    "section",
    "chapter",
    "figure",
    "table",
    "appendix",
    "question",
    "problem",
    "finding",
    "review",
    "scenario",
    "tier",
    "level",
    "gate",
    "check",
    "checkpoint",
    "slice",
    "story",
    "theme",
    "lane",
    "pass",
    "attempt",
    "iteration",
    "experiment",
    "trial",
    "session",
    "pillar",
    "principle",
    "goal",
    "version",
];

/// Words that end in `s` and are never a plural noun, so a number before one is no count.
const NOT_PLURAL_WORDS: &[&str] = &[
    "across",
    "always",
    "does",
    "goes",
    "has",
    "its",
    "perhaps",
    "plus",
    "sometimes",
    "thus",
    "towards",
    "unless",
    "was",
    "this",
    "yes",
    "less",
    "besides",
    "afterwards",
    "otherwise",
];

/// Developer tools a reader may not know. A bare use with no description is a name nobody explained.
const DEV_TOOLS: &[&str] = &[
    "moon",
    "bazel",
    "mise",
    "nx",
    "turborepo",
    "pnpm",
    "yarn",
    "lerna",
    "cmake",
    "meson",
    "earthly",
    "skaffold",
    "asdf",
    "direnv",
    "devbox",
    "fnm",
    "volta",
    "nvm",
    "esbuild",
    "webpack",
    "rollup",
    "biome",
    "oxlint",
    "eslint",
    "prettier",
    "ruff",
    "pytest",
    "poetry",
    "pipx",
    "nextest",
    "sccache",
    "rustup",
    "rustfmt",
    "clippy",
    "terraform",
    "pulumi",
    "ansible",
    "helm",
    "kustomize",
    "kubectl",
    "minikube",
    "podman",
    "buildah",
    "buildkit",
    "buildx",
    "lefthook",
    "husky",
    "renovate",
    "dependabot",
    "jq",
    "yq",
    "ripgrep",
    "fzf",
    "tmux",
    "zellij",
];

/// Verbs that follow a postposed `above` and take an object or a clause, such as `the table above lists the rules`.
const ABOVE_VERBS: &[&str] = &[
    "explains",
    "lists",
    "describes",
    "shows",
    "covers",
    "says",
    "states",
    "defines",
    "gives",
    "notes",
    "names",
    "introduces",
    "details",
    "summarises",
    "summarizes",
    "outlines",
    "recommends",
    "proposes",
    "uses",
    "contains",
    "includes",
    "mentions",
    "presents",
    "specifies",
    "requires",
    "applies",
    "sets",
    "makes",
    "means",
    "implies",
    "argues",
    "claims",
    "sketches",
    "documents",
    "records",
    "tracks",
    "matches",
    "repeats",
    "holds",
    "draws",
    "found",
    "said",
    "gave",
    "made",
    "took",
    "wrote",
    "ran",
    "led",
    "chose",
];

/// Whether `term` is a developer tool name from the built-in list.
pub(super) fn is_tool_term(term: &str) -> bool {
    DEV_TOOLS.contains(&term)
}

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
    out.extend(number_candidates(unit, &masked, known));
    out.extend(phrase_candidates(&masked, &unit.text, cfg));
    out.extend(time_candidates(&masked, context));
    out.extend(name_candidates(
        unit,
        &quotes,
        cfg,
        &doc.sentences,
        doc_run_counts,
    ));
    if context == Context::Document {
        out.extend(tool_candidates(unit, &quotes, &masked, known));
    }
    keep_longest_per_kind(out)
}

/// A lowercase developer tool name, in backticks or as a bare word, that the known names do not cover.
fn tool_candidates(
    unit: &TextUnit,
    quotes: &[QuotedSpan],
    masked: &str,
    known: &KnownNames,
) -> Vec<Candidate> {
    static BARE: OnceLock<Regex> = OnceLock::new();
    let mut out = Vec::new();
    for q in quotes.iter().filter(|q| q.kind == MentionKind::Backtick) {
        let content = unit.text.get(q.content.clone()).unwrap_or("").trim();
        if is_tool_term(content) && !is_known_name_head(known, content) {
            out.push(Candidate {
                kind: Kind::Name,
                text: content.to_string(),
                range: q.content.clone(),
            });
        }
    }
    let bare = BARE.get_or_init(|| {
        Regex::new(&format!(r"\b(?:{})\b", DEV_TOOLS.join("|"))).expect("tool pattern compiles")
    });
    for m in bare.find_iter(masked) {
        let before = masked.get(..m.start()).and_then(|b| b.chars().next_back());
        let after = masked.get(m.end()..).unwrap_or("");
        let joins_a_path_or_name = before.is_some_and(|c| matches!(c, '-' | '_' | '.' | '/' | '@'))
            || after.starts_with(['-', '_', '/', '@'])
            || after
                .strip_prefix('.')
                .is_some_and(|rest| rest.starts_with(char::is_alphanumeric));
        if joins_a_path_or_name || is_known_name_head(known, m.as_str()) {
            continue;
        }
        out.push(Candidate {
            kind: Kind::Name,
            text: m.as_str().to_string(),
            range: m.range(),
        });
    }
    out
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

    let backtick_re = super::rules::re(&BACKTICK, r"`([^`]*)`");
    let backtick_spans: Vec<QuotedSpan> = backtick_re
        .captures_iter(text)
        .filter_map(|c| {
            Some(QuotedSpan {
                open: c.get(0)?.range(),
                content: c.get(1)?.range(),
                kind: MentionKind::Backtick,
            })
        })
        .collect();
    let inside_code = |r: &Range<usize>| {
        backtick_spans
            .iter()
            .any(|b| b.open.start <= r.start && r.end <= b.open.end)
    };

    let mut spans = Vec::new();
    for (open_ch, close_ch) in [('"', '"'), ('\u{201C}', '\u{201D}')] {
        spans.extend(
            simple_quote_spans(text, open_ch, close_ch)
                .into_iter()
                .filter(|(open, _)| !inside_code(open))
                .map(|(open, content)| QuotedSpan {
                    open,
                    content,
                    kind: MentionKind::DoubleQuote,
                }),
        );
    }
    for (open_ch, close_ch) in [('\'', '\''), ('\u{2018}', '\u{2019}')] {
        spans.extend(
            paired_quote_spans(text, open_ch, close_ch, sentences)
                .into_iter()
                .filter(|(open, _)| !inside_code(open))
                .map(|(open, content)| QuotedSpan {
                    open,
                    content,
                    kind: MentionKind::SingleQuote,
                }),
        );
    }
    spans.extend(backtick_spans);
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

fn number_candidates(unit: &TextUnit, text: &str, known: &KnownNames) -> Vec<Candidate> {
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
            .filter_map(|c| word_number_candidate(unit, text, &c, known)),
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

/// Whether `word` is a real name for the number exclusion: any known name,
/// built in or added by a project's known-names file, unless it is a
/// generic word such as `Phase` that carries no such evidence on its own.
fn is_known_name_head(known: &KnownNames, word: &str) -> bool {
    case_variants(word)
        .into_iter()
        .any(|form| known.contains(&form) && !super::names::is_generic_word(&form))
}

/// A word and a plain integer, unless it is a version, a range, a known name, a month and day,
/// or a measure, a size or a coordinate.
fn word_number_candidate(
    unit: &TextUnit,
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
    if number.as_str().contains('.') || looks_like_a_year(number.as_str()) {
        return None;
    }
    let after = text.get(number.end()..).unwrap_or("");
    if is_percent_quantity(text, number.end()) || is_a_range_end(after) {
        return None;
    }
    let labels_one_thing = LABEL_NOUNS.contains(&word_lower.as_str());
    if continues_the_number(after)
        || is_a_citation_volume(after)
        || (is_followed_by_a_unit(after) && !labels_one_thing)
    {
        return None;
    }
    if is_a_measured_value(text, word, &word_lower) {
        return None;
    }
    if counts_a_plural_noun(text, word, &word_lower, after) {
        return None;
    }
    if word_lower == "version" && follows_a_product_name(&unit.text, word.start(), known) {
        return None;
    }
    if is_known_name_head(known, word.as_str()) {
        return None;
    }
    if is_a_label_value_item(unit, after) || is_a_defining_heading(unit, text, whole.start(), after)
    {
        return None;
    }
    Some(Candidate {
        kind: Kind::Number,
        text: whole.as_str().to_string(),
        range: whole.range(),
    })
}

/// Whether the text before `start` ends a sentence, or is only markup, so a word at `start` opens one.
fn opens_a_sentence(text: &str, start: usize) -> bool {
    let before = text.get(..start).unwrap_or("").trim_end_matches(|c: char| {
        c.is_whitespace() || matches!(c, '#' | '*' | '>' | '-' | '|' | '_' | '(' | '[' | '"')
    });
    before.is_empty() || before.ends_with(['.', '!', '?', ':'])
}

/// Whether `word` is a plural noun: it ends in `s`, and is no verb form or other word that only looks plural.
fn is_a_plural_noun(word: &str) -> bool {
    word.len() >= 4
        && word.ends_with('s')
        && !word.ends_with("ss")
        && !word.ends_with("us")
        && !word.ends_with("is")
        && !NOT_PLURAL_WORDS.contains(&word)
}

/// A digit that counts a plural noun, as in `holds 3 items`, is a count and never a numbered label.
/// A label noun or a capitalised word inside a sentence names one thing, so it never counts a plural.
fn counts_a_plural_noun(text: &str, word: regex::Match<'_>, word_lower: &str, after: &str) -> bool {
    if LABEL_NOUNS.contains(&word_lower) || !after.starts_with(char::is_whitespace) {
        return false;
    }
    let capitalised = word.as_str().starts_with(char::is_uppercase);
    if capitalised && !opens_a_sentence(text, word.start()) {
        return false;
    }
    let next: String = after
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphabetic())
        .collect();
    is_a_plural_noun(&next.to_lowercase())
}

/// Whether a product, named before `start`, places the version number that follows `version`.
/// It is a known name, a name with an inner capital, a code span, or a capitalised word inside a sentence.
fn follows_a_product_name(raw: &str, start: usize, known: &KnownNames) -> bool {
    let before = raw.get(..start).unwrap_or("");
    for token in before.split_whitespace().rev().take(3) {
        let bare = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '`');
        let lower = bare.to_lowercase();
        if NON_LABEL_WORDS.contains(&lower.as_str()) {
            continue;
        }
        if bare.starts_with('`') || bare.ends_with('`') {
            return true;
        }
        let word_start = before.rfind(token).unwrap_or(0);
        let is_a_name = is_known_name_head(known, bare)
            || bare.chars().skip(1).any(char::is_uppercase)
            || (bare.starts_with(char::is_uppercase) && !opens_a_sentence(before, word_start));
        return is_a_name;
    }
    false
}

/// A hyphenated range such as `12-15`.
fn is_a_range_end(after: &str) -> bool {
    let mut chars = after.chars();
    chars.next() == Some('-') && chars.next().is_some_and(|ch| ch.is_ascii_digit())
}

/// A digit run cut short of a longer number: `1.88M`, `1,000`, `1.x`, `30+`.
fn continues_the_number(after: &str) -> bool {
    let mut chars = after.chars();
    match chars.next() {
        Some('+') => true,
        Some('.') => chars
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == 'x' || c == '*'),
        Some(',') => after
            .get(1..4)
            .is_some_and(|d| d.chars().all(|c| c.is_ascii_digit())),
        _ => false,
    }
}

/// A journal's `volume(issue)`, such as `5(2)`.
fn is_a_citation_volume(after: &str) -> bool {
    after
        .strip_prefix('(')
        .and_then(|rest| rest.split_once(')'))
        .is_some_and(|(inner, _)| !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()))
}

/// Whether a unit follows the number: `5 s`, `44 px`, `24 hours`, `12 degrees`. A possessive
/// such as `decision 0005's` is no unit, so the unit must start with a letter.
fn is_followed_by_a_unit(after: &str) -> bool {
    let rest = after.trim_start();
    let unit_end = rest
        .find(|c: char| !c.is_alphabetic())
        .unwrap_or(rest.len());
    rest.get(..unit_end)
        .is_some_and(|unit| UNIT_WORDS.contains(&unit.to_lowercase().as_str()))
}

/// Whether the word before the number marks it as a quantity, a size, a version or a coordinate, not a label.
fn is_a_measured_value(text: &str, word: regex::Match<'_>, word_lower: &str) -> bool {
    let is_quantity_word = QUANTITY_WORDS.contains(&word_lower)
        || DIMENSION_WORDS.contains(&word_lower)
        || VERSION_WORDS.contains(&word_lower);
    let is_a_single_letter = word.as_str().chars().count() == 1;
    let joins_a_hyphenated_name = text
        .get(..word.start())
        .is_some_and(|before| before.ends_with('-') && before.len() > 1);
    is_quantity_word || is_a_single_letter || joins_a_hyphenated_name || is_a_participle(word_lower)
}

/// A list item that is only a lower-case label and a number, such as `compact rail 60`.
fn is_a_label_value_item(unit: &TextUnit, after: &str) -> bool {
    let item = unit.text.trim();
    let label_words = item
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .trim_end();
    unit.in_list_item
        && after.trim().is_empty()
        && item.chars().next().is_some_and(char::is_lowercase)
        && label_words.split(' ').count() <= 3
        && label_words.chars().all(|c| c.is_alphabetic() || c == ' ')
}

/// A heading that opens with a numbered label and then names it in three or more words, or with a link,
/// such as `Stage 1 Canonical local verification`. One or two words after the label name nothing.
fn is_a_defining_heading(unit: &TextUnit, text: &str, start: usize, after: &str) -> bool {
    let only_markup_before = text
        .get(..start)
        .is_some_and(|before| before.chars().all(|c| !c.is_alphanumeric()));
    let named = after.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    let words = named
        .split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count();
    let names_it = (named.starts_with(char::is_alphabetic) && words >= 3) || named.contains("](");
    unit.is_heading && only_markup_before && names_it
}

/// Whether `number` is a plain four-digit value in a plausible calendar-year
/// range, the shape of a citation such as "Weiss 2000" or "Devanbu 2011".
fn looks_like_a_year(number: &str) -> bool {
    number.len() == 4
        && number
            .parse::<u32>()
            .is_ok_and(|n| (1900..=2099).contains(&n))
}

/// Whether the text right after a number's end is a percent sign, with or without a space,
/// or the word "percent": a quantity, never a label, whatever word comes before it.
fn is_percent_quantity(text: &str, number_end: usize) -> bool {
    let after = text.get(number_end..).unwrap_or("").trim_start();
    if after.starts_with('%') {
        return true;
    }
    after
        .strip_prefix("percent")
        .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric()))
}

/// A word and a single letter or short number in brackets, such as `mechanism (b)`.
fn word_bracket_candidate(c: &regex::Captures<'_>, known: &KnownNames) -> Option<Candidate> {
    let whole = c.get(0)?;
    let word = c.get(1)?;
    let word_lower = word.as_str().to_lowercase();
    let attached = !whole
        .as_str()
        .get(word.as_str().len()..)
        .is_some_and(|gap| gap.starts_with(char::is_whitespace));
    if attached
        || NON_LABEL_WORDS.contains(&word_lower.as_str())
        || is_known_name_head(known, word.as_str())
    {
        return None;
    }
    Some(Candidate {
        kind: Kind::Number,
        text: whole.as_str().to_string(),
        range: whole.range(),
    })
}

fn phrase_candidates(text: &str, raw: &str, cfg: &WritingConfig) -> Vec<Candidate> {
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
    out.extend(above_reference_candidates(text, raw));
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

/// Words that can follow a reference `above` and start the rest of the clause, never an object of the preposition.
const CLAUSE_FOLLOWERS: &[&str] = &[
    "and", "or", "but", "before", "after", "for", "in", "is", "are", "was", "were", "to", "as",
    "if", "when", "which", "with", "on", "by", "from", "so", "then", "because", "since", "until",
    "while", "can", "will", "should", "must", "may", "has", "have", "had", "do", "does", "did",
    "not", "also",
];

/// `above` as a reference to earlier text, told apart from the preposition by what follows it.
/// A reference closes its clause: the text ends, a punctuation mark follows, or a conjunction or
/// auxiliary word follows. A preposition takes an object, such as `above the line`, `above it` or
/// `above everything`. A sentence that also says `below` is a spatial pair, never a reference.
fn above_reference_candidates(text: &str, raw: &str) -> Vec<Candidate> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = super::rules::re(&RE, r"(?i)\babove\b");
    re.find_iter(text)
        .filter(|m| {
            let rest = text.get(m.end()..).unwrap_or("");
            let raw_rest = raw.get(m.end()..).unwrap_or("");
            let is_a_noun = follows_a_determiner(text, m.start());
            let is_postposed = is_a_postposed_reference(text, m.start(), rest, raw_rest);
            (is_a_noun || is_postposed || closes_a_clause(rest, raw_rest)) && !is_spatial_pair(rest)
        })
        .map(|m| Candidate {
            kind: Kind::Phrase,
            text: m.as_str().to_string(),
            range: m.range(),
        })
        .collect()
}

/// Whether `rest`, the text after a word, ends the clause or opens the next one. `raw_rest` is the
/// same text before any quoted or code span was blanked, so a span right after the word is an object.
fn closes_a_clause(rest: &str, raw_rest: &str) -> bool {
    let trimmed = rest.trim_start();
    if raw_rest
        .trim_start()
        .starts_with(['`', '"', '\'', '\u{201C}', '\u{2018}'])
    {
        return false;
    }
    match trimmed.chars().next() {
        None => true,
        Some(c) if c.is_alphanumeric() => {
            let word_end = trimmed
                .find(|c: char| !c.is_alphanumeric())
                .unwrap_or(trimmed.len());
            let word = trimmed.get(..word_end).unwrap_or("").to_lowercase();
            let after_word = trimmed.get(word_end..).unwrap_or("");
            CLAUSE_FOLLOWERS.contains(&word.as_str())
                || (is_a_participle(&word) && closes_a_clause(after_word, after_word))
        }
        Some(c) => matches!(
            c,
            '.' | ',' | ';' | ':' | '!' | '?' | ')' | '|' | '\u{2014}' | '\u{2013}'
        ),
    }
}

/// A noun with a determiner before `above`, then a verb that takes an object or a clause, as in
/// `the table above lists the rules`. A verb form followed by a plain noun, as in `above recommended
/// limits`, is an object phrase and a position.
fn is_a_postposed_reference(text: &str, start: usize, rest: &str, raw_rest: &str) -> bool {
    let before: Vec<String> = text
        .get(..start)
        .unwrap_or("")
        .split_whitespace()
        .rev()
        .take(3)
        .map(str::to_lowercase)
        .collect();
    let after_a_determiner = before.iter().skip(1).any(|w| {
        matches!(
            w.as_str(),
            "the"
                | "this"
                | "that"
                | "these"
                | "those"
                | "our"
                | "your"
                | "its"
                | "their"
                | "a"
                | "an"
        )
    });
    if !after_a_determiner || raw_rest.trim_start().starts_with(['`', '"', '\'']) {
        return false;
    }
    let mut words = rest.split_whitespace();
    let Some(verb) = words.next().map(str::to_lowercase) else {
        return false;
    };
    let is_a_verb = ABOVE_VERBS.contains(&verb.as_str()) || is_a_participle(&verb);
    is_a_verb
        && words.next().is_some_and(|next| {
            NON_LABEL_WORDS.contains(&next.to_lowercase().as_str())
                || matches!(
                    next.to_lowercase().as_str(),
                    "how" | "why" | "what" | "which"
                )
        })
}

/// A lower-case word that reads as a past participle, such as `recommended` or `returned`.
fn is_a_participle(word_lower: &str) -> bool {
    word_lower.len() >= 5 && word_lower.ends_with("ed") && !word_lower.ends_with("eed")
}

/// Whether the word right before `end` is a determiner, so a word after it is a noun: `the above`.
fn follows_a_determiner(text: &str, end: usize) -> bool {
    text.get(..end)
        .and_then(|before| before.split_whitespace().next_back())
        .is_some_and(|w| {
            matches!(
                w.to_lowercase().as_str(),
                "the" | "this" | "these" | "those"
            )
        })
}

/// Whether the sentence goes on to say `below`, the other half of a pair of places.
fn is_spatial_pair(rest: &str) -> bool {
    let sentence = rest.split(['.', '!', '?', '|', '\n']).next().unwrap_or("");
    sentence
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| w.eq_ignore_ascii_case("below"))
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
            "The fix landed in open-software-factory/widgets#125 today.",
            Kind::Number,
            "open-software-factory/widgets#125"
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

    /// A commit trailer names a model tier and a version, such as this project's own `Claude Sonnet 5`.
    #[test]
    fn number_excludes_a_model_name_followed_by_a_version() {
        assert!(kinds(
            "Code-Generator: Claude Sonnet 5 <noreply@anthropic.com>",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Number));
        assert!(kinds("Reviewed by Claude Opus 5.", Context::Document)
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

    /// A name from the project's known-names file excludes the number after it, the same as a built-in name.
    #[test]
    fn number_excludes_a_project_known_name_followed_by_a_number() {
        let dir = std::env::temp_dir().join(format!(
            "osf-reference-test-known-names-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("known-names.txt");
        std::fs::write(&path, "Databricks\n").expect("known names file writes");
        let with_file =
            load_known_names(&[], Some(&path)).expect("known names with a project file load");

        assert!(candidates(
            &paragraph("Databricks 5 shipped."),
            &WritingConfig::default(),
            &with_file,
            Context::Document,
            &HashMap::new(),
        )
        .iter()
        .all(|c| c.kind != Kind::Number));

        assert!(candidates(
            &paragraph("Databricks 5 shipped."),
            &WritingConfig::default(),
            &known(),
            Context::Document,
            &HashMap::new(),
        )
        .iter()
        .any(|c| c.kind == Kind::Number && c.text == "Databricks 5"));

        std::fs::remove_dir_all(&dir).expect("temp dir cleans up");
    }

    /// A generic word in the project's known-names file still does not exclude the number after it.
    #[test]
    fn number_does_not_exclude_a_generic_word_from_a_project_known_names_file() {
        let dir = std::env::temp_dir().join(format!(
            "osf-reference-test-known-names-generic-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("known-names.txt");
        std::fs::write(&path, "Phase\n").expect("known names file writes");
        let with_file =
            load_known_names(&[], Some(&path)).expect("known names with a project file load");

        assert!(candidates(
            &paragraph("In Phase 2 we ship it."),
            &WritingConfig::default(),
            &with_file,
            Context::Document,
            &HashMap::new(),
        )
        .iter()
        .any(|c| c.kind == Kind::Number && c.text == "Phase 2"));

        std::fs::remove_dir_all(&dir).expect("temp dir cleans up");
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

    /// A citation's `Author YYYY` shape is a year, not a label, whatever the preceding word is.
    #[test]
    fn number_excludes_a_word_followed_by_a_plausible_year() {
        assert!(kinds(
            "Weiss 2000 and Devanbu 2011 both found this.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Number));
    }

    #[test]
    fn number_still_excludes_a_year() {
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

    /// A percent quantity is never a candidate, whatever word precedes it.
    #[test]
    fn number_excludes_a_percent_quantity() {
        assert!(
            kinds("The study reached 86 percent accuracy.", Context::Document)
                .iter()
                .all(|k| *k != Kind::Number)
        );
        assert!(kinds(
            "Its top two flagged hunks held 54 percent of the risky lines.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Number));
        assert!(kinds(
            "Fixing the leak cut its score by roughly 40 percent.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Number));
        assert!(kinds(
            "The suite recovered 16% of previously failed tasks.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Number));
    }

    #[test]
    fn number_excludes_a_range_opened_by_between() {
        assert!(kinds(
            "Between 21 and 24 August 2026 four sessions ran.",
            Context::Document
        )
        .iter()
        .all(|k| *k != Kind::Number));
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

    // --- number: measures, sizes and coordinates are never labels ---

    fn has_no_number(text: &str) -> bool {
        kinds(text, Context::Document)
            .iter()
            .all(|k| *k != Kind::Number)
    }

    #[test]
    fn number_excludes_a_number_followed_by_a_unit() {
        assert!(has_no_number("Elapsed time shows once it passes 5 s."));
        assert!(has_no_number("Beyond 24 hours, use the absolute date."));
        assert!(has_no_number("Dense rows may be 28 px minimum."));
        assert!(has_no_number(
            "The sector now runs 12 degrees past its end."
        ));
        assert!(has_no_number(
            "It waited a fixed 200 ms before the next frame."
        ));
    }

    #[test]
    fn number_excludes_a_percent_with_a_space() {
        assert!(has_no_number("They found 125 % zoom easier to read."));
    }

    #[test]
    fn number_excludes_a_number_after_a_quantity_or_dimension_word() {
        assert!(has_no_number("Use at most 3 significant digits."));
        assert!(has_no_number("Shell suites cover 25 rows, all passing."));
        assert!(has_no_number("Add plus 1 more for the keyed offset."));
        assert!(has_no_number("It stays pinned near opacity 0 throughout."));
        assert!(has_no_number("Set the stage width 720 first."));
    }

    #[test]
    fn number_excludes_a_digit_run_cut_short_of_a_longer_number() {
        assert!(has_no_number("The graph holds 1,000 nodes."));
        assert!(has_no_number("The corpus holds 1.88M files."));
        assert!(has_no_number("The package ships 30+ plugins."));
        assert!(has_no_number("Lint with oxlint 1.x for speed."));
    }

    #[test]
    fn number_excludes_a_release_a_coordinate_and_a_package_version() {
        assert!(has_no_number("Chrome Version 7 beta added tabs."));
        assert!(has_no_number(
            "Chrome and Edge from version 113 support it."
        ));
        assert!(has_no_number("Pin `moon` version 2 for the build."));
        assert!(has_no_number("We run DuckDB version 1 in the tests."));
        assert!(has_no_number("The panes measure L 382 and R 358."));
        assert!(has_no_number(
            "It drags in react-spring 8 and Material-UI 4."
        ));
    }

    #[test]
    fn number_excludes_a_result_after_a_past_tense_verb() {
        assert!(has_no_number("The registry returned 404 on fetch."));
    }

    #[test]
    fn number_excludes_a_citation_volume_and_issue() {
        assert!(has_no_number("See Technical Journal 5(2) for the study."));
    }

    #[test]
    fn number_excludes_a_known_framework_followed_by_a_version() {
        assert!(has_no_number("The build moves to Vite 8 and React 19 now."));
        assert!(has_no_number("Patches follow RFC 6902 exactly."));
    }

    #[test]
    fn number_excludes_a_bracket_joined_to_its_word() {
        assert!(has_no_number("The cell reads Agent(s) executing."));
        assert!(has(
            "The outage traced back to mechanism (b).",
            Kind::Number,
            "mechanism (b)"
        ));
    }

    fn paragraph_in(text: &str, in_list_item: bool, is_heading: bool) -> TextUnit {
        let mut unit = paragraph(text);
        unit.in_list_item = in_list_item;
        unit.is_heading = is_heading;
        unit
    }

    fn number_candidate_count(unit: &TextUnit) -> usize {
        candidates(
            unit,
            &WritingConfig::default(),
            &known(),
            Context::Document,
            &HashMap::new(),
        )
        .iter()
        .filter(|c| c.kind == Kind::Number)
        .count()
    }

    #[test]
    fn number_excludes_a_list_item_that_is_only_a_label_and_a_value() {
        assert_eq!(
            number_candidate_count(&paragraph_in("compact rail 60", true, false)),
            0
        );
        assert_eq!(
            number_candidate_count(&paragraph_in("centre floor 700", true, false)),
            0
        );
        assert_eq!(
            number_candidate_count(&paragraph_in("compact rail 60", false, false)),
            1
        );
        assert_eq!(
            number_candidate_count(&paragraph_in("Phase 2", true, false)),
            1
        );
        assert_eq!(
            number_candidate_count(&paragraph_in("see step 4 now", true, false)),
            1
        );
    }

    #[test]
    fn number_excludes_a_heading_that_defines_its_own_label() {
        assert_eq!(
            number_candidate_count(&paragraph_in(
                "Stage 1 Canonical local verification",
                false,
                true
            )),
            0
        );
        assert_eq!(
            number_candidate_count(&paragraph_in(
                "Phase 3: Implementation of the loop",
                false,
                true
            )),
            0
        );
        assert_eq!(
            number_candidate_count(&paragraph_in(
                "Stage 1 [Local checks](checks.md)",
                false,
                true
            )),
            0
        );
        for short in [
            "Decision 0014 Status",
            "Stage 1 Local checks",
            "Phase 3: Implementation",
        ] {
            assert_eq!(
                number_candidate_count(&paragraph_in(short, false, true)),
                1,
                "{short}"
            );
        }
        assert_eq!(
            number_candidate_count(&paragraph_in("Stage 2", false, true)),
            1
        );
        assert_eq!(
            number_candidate_count(&paragraph_in("Notes on Stage 1 checks", false, true)),
            1
        );
        assert_eq!(
            number_candidate_count(&paragraph_in("Stage 1 Local checks", false, false)),
            1
        );
    }

    /// A possessive is no unit: `decision 0005's` and `Track 02's` stay references.
    #[test]
    fn number_still_fires_on_a_label_with_a_possessive() {
        assert!(has(
            "The states are decision 0005's.",
            Kind::Number,
            "decision 0005"
        ));
        assert!(has(
            "Track 02's answer applied to the old requirement.",
            Kind::Number,
            "Track 02"
        ));
        assert!(has_no_number("The wait is 5 s."));
    }

    /// A measure word never hides a real label: the labels the fixtures name still fire.
    #[test]
    fn number_still_fires_on_a_label_next_to_a_measure() {
        assert!(has(
            "Layers 1 and 2 carry the behaviours.",
            Kind::Number,
            "Layers 1"
        ));
        assert!(has("Use Layer 2 for 5 s at most.", Kind::Number, "Layer 2"));
        assert!(has(
            "Control 1 swaps live to the constant.",
            Kind::Number,
            "Control 1"
        ));
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

    fn has_no_phrase(text: &str) -> bool {
        kinds(text, Context::Document)
            .iter()
            .all(|k| *k != Kind::Phrase)
    }

    /// A position in a stack takes an object, so `above it`, `above everything` and `above threshold` are never references.
    #[test]
    fn phrase_above_taking_any_object_is_a_position_not_a_reference() {
        assert!(has_no_phrase("Bring every branch above it up to date."));
        assert!(has_no_phrase("The orb floats above everything in the app."));
        assert!(has_no_phrase(
            "It stays above overlays but below the dialog."
        ));
        assert!(has_no_phrase("Flag a cost anomaly above threshold."));
        assert!(has_no_phrase("Node fills sit 1.5 tones above bg."));
        assert!(has_no_phrase("Panels above canvas use a shadow."));
        assert!(has_no_phrase("The word above `canvas` is a position."));
        assert!(has_no_phrase("Keep the volume above recommended limits."));
    }

    #[test]
    fn phrase_above_paired_with_below_is_a_pair_of_places() {
        assert!(has_no_phrase(
            "The nav reads as primary navigation above, and secondary surfaces below."
        ));
    }

    #[test]
    fn phrase_above_closing_its_clause_is_a_reference() {
        assert!(has(
            "It keeps a record of all of the above.",
            Kind::Phrase,
            "above"
        ));
        assert!(has(
            "Once the above is done, ship it.",
            Kind::Phrase,
            "above"
        ));
        assert!(has(
            "It is the layer described above.",
            Kind::Phrase,
            "above"
        ));
        assert!(has(
            "Use the options above, not the old ones.",
            Kind::Phrase,
            "above"
        ));
        assert!(has(
            "Snap-to-edge, which the plan above recommended, is one option.",
            Kind::Phrase,
            "above"
        ));
        assert!(has(
            "| Record | all of the above | permanent |",
            Kind::Phrase,
            "above"
        ));
    }

    /// A participle that closes its clause with a comma is a reduced relative clause, whatever follows the comma.
    #[test]
    fn phrase_above_before_a_participle_and_a_comma_is_a_reference() {
        assert!(has(
            "Snap-to-edge, which the plan above recommended, versus staying put, is one option.",
            Kind::Phrase,
            "above"
        ));
    }

    /// A determiner right before `above` makes it a noun, so what follows never turns it into a preposition.
    #[test]
    fn phrase_the_above_is_a_noun_whatever_follows() {
        assert!(has(
            "Attention items point at any of the above through related IDs.",
            Kind::Phrase,
            "above"
        ));
        assert!(has_no_phrase("Place the label above the field."));
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

    /// Double-quoted JSON keys inside a backtick code span are code, not a mention, so they never become name candidates.
    #[test]
    fn name_excludes_a_double_quoted_span_nested_inside_backticks() {
        let t = r#"That hook returns `{"decision":"block","reason":"why"}` to the caller."#;
        assert!(kinds(t, Context::Document).iter().all(|k| *k != Kind::Name));
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

    // --- review fixes: counts, versions, units, above and tools ---

    /// A digit that counts a plural noun is a count. Neither `3 items` nor `only 3 checkpoints` is a reference.
    #[test]
    fn number_excludes_a_digit_that_counts_a_plural_noun() {
        assert!(has_no_number("The cache holds 3 items."));
        assert!(has_no_number("The queue has 3 workers."));
        assert!(has_no_number("The release has only 3 checkpoints."));
        assert!(has_no_number("We show the top 10 results."));
        assert!(has_no_number("Add 3 workers to the pool."));
        assert!(has_no_number("Use at most 3 significant digits."));
        assert!(has_no_number("It runs roughly 30 workers."));
    }

    /// A label noun, or a capitalised word inside a sentence, still labels one thing before a verb that ends in `s`.
    #[test]
    fn number_still_fires_on_a_label_before_a_verb_or_a_unit_word() {
        assert!(has("Layer 2 holds the cache.", Kind::Number, "Layer 2"));
        assert!(has("Step 4 fails.", Kind::Number, "Step 4"));
        assert!(has("Phase 2 starts later.", Kind::Number, "Phase 2"));
        assert!(has(
            "Decision 0014 is final.",
            Kind::Number,
            "Decision 0014"
        ));
        assert!(has(
            "Decision 12 seconds the proposal.",
            Kind::Number,
            "Decision 12"
        ));
        assert!(has(
            "We shipped Wave 3 starts today.",
            Kind::Number,
            "Wave 3"
        ));
        assert!(has_no_number("The wait is 12 seconds."));
    }

    #[test]
    fn number_version_needs_a_product_name_before_it() {
        assert!(has("Revert to version 7.", Kind::Number, "version 7"));
        assert!(has(
            "In version 7 we dropped the old cache.",
            Kind::Number,
            "version 7"
        ));
        assert!(has_no_number("Chrome version 113 supports it."));
        assert!(has_no_number("Pin `moon` version 2 for the build."));
    }

    #[test]
    fn phrase_above_after_a_noun_and_before_a_verb_is_a_reference() {
        for t in [
            "The section above explains the exception.",
            "The table above lists the rules.",
            "The plan above recommended a fix.",
            "The pricing table above shows how it works.",
        ] {
            assert!(
                has(t, Kind::Phrase, "above"),
                "{t}: {:?}",
                find(t, Context::Document)
            );
        }
    }

    #[test]
    fn phrase_above_before_an_object_phrase_is_still_a_position() {
        assert!(has_no_phrase("Keep the volume above recommended limits."));
        assert!(has_no_phrase("Put the label above notes."));
        assert!(has_no_phrase("Place the label above the field."));
        assert!(has_no_phrase("The orb floats above everything."));
        assert!(has_no_phrase("Panels above canvas use a shadow."));
    }

    fn tool_names(text: &str, context: Context) -> Vec<String> {
        find(text, context)
            .into_iter()
            .filter(|c| c.kind == Kind::Name && is_tool_term(&c.text))
            .map(|c| c.text)
            .collect()
    }

    #[test]
    fn name_a_bare_or_backticked_tool_is_a_candidate_in_a_document() {
        assert_eq!(
            tool_names("We pinned the renderer to moon 2.5.5.", Context::Document),
            vec!["moon"]
        );
        for t in ["Run `moon` now.", "Run `bazel` now.", "Run `mise` now."] {
            assert_eq!(tool_names(t, Context::Document).len(), 1, "{t}");
        }
    }

    #[test]
    fn name_a_tool_is_no_candidate_outside_a_document_or_inside_a_path() {
        assert!(tool_names("Run `moon` now.", Context::Transcript).is_empty());
        assert!(tool_names("Run `moon` now.", Context::Commit).is_empty());
        assert!(tool_names("Edit .moon/workspace.yml and moon.yml.", Context::Document).is_empty());
        assert!(tool_names("The moonlight and Moon rise.", Context::Document).is_empty());
        assert!(tool_names("Run `moon run :test` now.", Context::Document).is_empty());
    }

    #[test]
    fn name_a_tool_a_project_already_knows_is_no_candidate() {
        let dir =
            std::env::temp_dir().join(format!("osf-reference-test-tool-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        let path = dir.join("known-names.txt");
        std::fs::write(&path, "moon\n").expect("known names file writes");
        let with_file = load_known_names(&[], Some(&path)).expect("known names load");
        let found = candidates(
            &paragraph("Run `moon` and moon again."),
            &WritingConfig::default(),
            &with_file,
            Context::Document,
            &HashMap::new(),
        );
        assert!(found.iter().all(|c| !is_tool_term(&c.text)), "{found:?}");
        std::fs::remove_dir_all(&dir).expect("temp dir cleans up");
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
