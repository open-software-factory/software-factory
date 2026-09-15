//! Each rule is a pure function from one unit of text to zero or more
//! findings, registered with the scope it needs. The lint runs every rule
//! over the units its scope asks for.

use osf_lint_core::segment::{reduce_inline, Doc, TextUnit};
use osf_lint_core::{run_rules, Finding, FnRule, KnownNames, Level, Rule};
use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

const SENTENCE_RULES: &[FnRule] = &[
    FnRule::sentence("bare-reference", bare_reference),
    FnRule::sentence("reference-without-label", reference_without_label),
    FnRule::sentence("chat-local-reference", chat_local_reference),
    FnRule::sentence("long-sentence", long_sentence),
    FnRule::sentence("em-dash", em_dash),
    FnRule::sentence("arrow", arrow),
    FnRule::sentence("semicolon", semicolon),
    FnRule::sentence("filler", filler),
    FnRule::sentence("numbers-in-prose", numbers_in_prose),
    FnRule::sentence("bold-sentence", bold_sentence),
    FnRule::sentence("parenthetical", parenthetical),
];

const MAX_WORDS: usize = 25;
const MAX_NUMERALS: usize = 2;
const SHORT_TEXT_WORDS: usize = 500;

pub fn per_sentence(doc: &Doc, fast_only: bool, out: &mut Vec<Finding>) {
    let rules: Vec<&dyn Rule> = SENTENCE_RULES.iter().map(|r| r as &dyn Rule).collect();
    out.extend(run_rules(doc, &rules, fast_only));
}

/// Every rule id this lint can report, for validating a suppression's rule id.
pub fn rule_ids() -> Vec<&'static str> {
    SENTENCE_RULES
        .iter()
        .map(Rule::id)
        .chain([
            "heading-in-short-text",
            "undefined-name",
            "undefined-name-at-start",
        ])
        .collect()
}

pub fn headings_in_short_text(doc: &Doc, out: &mut Vec<Finding>) {
    if doc.word_count >= SHORT_TEXT_WORDS {
        return;
    }
    out.extend(doc.headings.iter().map(|&line| {
        Finding::new(
            "heading-in-short-text",
            Level::Error,
            line,
            format!(
                "a text under {SHORT_TEXT_WORDS} words has a heading; use a sentence or a table instead"
            ),
            format!("heading on line {line}"),
        )
    }));
}

fn re(cell: &'static OnceLock<Regex>, pattern: &'static str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("rule pattern compiles"))
}

fn finding(
    s: &TextUnit,
    rule: &'static str,
    level: Level,
    message: String,
    excerpt: &str,
) -> Finding {
    Finding::new(rule, level, s.line, message, excerpt.to_string())
}

fn matches(s: &TextUnit, pattern: &'static Regex) -> Vec<String> {
    let text = reduce_inline(&s.text);
    pattern
        .find_iter(&text)
        .map(|m| m.as_str().to_string())
        .collect()
}

fn first_words(words: &[String], n: usize) -> String {
    words.iter().take(n).cloned().collect::<Vec<_>>().join(" ")
}

/// `#123` with no `owner/repo` in front of it.
fn bare_reference(s: &TextUnit) -> Vec<Finding> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = re(&RE, r"(?:^|[^\w/.-])(#\d+)\b");
    let text = reduce_inline(&s.text);
    re.captures_iter(&text)
        .filter_map(|c| c.get(1).map(|g| g.as_str().to_string()))
        .map(|m| {
            finding(
                s,
                "bare-reference",
                Level::Error,
                format!("write the repository before the number, as in owner/repo{m}"),
                &m,
            )
        })
        .collect()
}

/// `owner/repo#N` or `repo#N` must carry a bracketed description; a link is expected.
fn reference_without_label(s: &TextUnit) -> Vec<Finding> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = re(&RE, r"(?:[\w.-]+/)?[\w.-]+#\d+");
    let text = reduce_inline(&s.text);
    let raw = &s.text;
    re.find_iter(&text)
        .flat_map(|m| {
            let reference = m.as_str().to_string();
            let labelled = text
                .get(m.end()..)
                .is_some_and(|rest| rest.trim_start().starts_with('('));
            let linked =
                raw.contains(&format!("[{reference}")) || raw.contains(&format!("{reference}]("));
            let label = (!labelled).then(|| {
                finding(
                    s,
                    "reference-without-label",
                    Level::Error,
                    format!("say what it is in brackets, as in {reference} (the subject)"),
                    &reference,
                )
            });
            let link = (!linked).then(|| {
                finding(
                    s,
                    "reference-without-link",
                    Level::Warning,
                    "link the reference so the reader can open it".to_string(),
                    &reference,
                )
            });
            label.into_iter().chain(link)
        })
        .collect()
}

/// Words that only mean something inside one conversation.
fn chat_local_reference(s: &TextUnit) -> Vec<Finding> {
    static PHRASES: OnceLock<Regex> = OnceLock::new();
    static STEP: OnceLock<Regex> = OnceLock::new();
    let phrases = re(
        &PHRASES,
        r"(?i)\b(?:phase \d+|item \d+|option \d+|part \d+|point \d+|as discussed|as mentioned|as noted|as i said|see above|mentioned above|the previous message|earlier today|in my last message)\b",
    );
    let step = re(&STEP, r"(?i)\b(step \d+)\b[,:]?\s*(\S?)");
    let text = reduce_inline(&s.text);
    let bare_steps = step
        .captures_iter(&text)
        .filter(|c| {
            !c.get(2)
                .and_then(|g| g.as_str().chars().next())
                .is_some_and(|ch| ch.is_alphabetic() || ch == '(')
        })
        .filter_map(|c| c.get(1).map(|g| g.as_str().to_string()));
    matches(s, phrases)
        .into_iter()
        .chain(bare_steps)
        .map(|m| {
            finding(
                s,
                "chat-local-reference",
                Level::Error,
                "name the thing itself; this reference only works inside one conversation"
                    .to_string(),
                &m,
            )
        })
        .collect()
}

fn long_sentence(s: &TextUnit) -> Vec<Finding> {
    let words = s.words();
    let n = words.len();
    if s.in_table || n <= MAX_WORDS {
        return vec![];
    }
    vec![finding(
        s,
        "long-sentence",
        Level::Error,
        format!("{n} words; split it, {MAX_WORDS} is the limit"),
        &format!("{}...", first_words(&words, 8)),
    )]
}

fn em_dash(s: &TextUnit) -> Vec<Finding> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = re(&RE, r"—|–| -- ");
    matches(s, re)
        .iter()
        .map(|m| {
            finding(
                s,
                "em-dash",
                Level::Error,
                "use a full stop or a comma".to_string(),
                m.trim(),
            )
        })
        .collect()
}

fn arrow(s: &TextUnit) -> Vec<Finding> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = re(&RE, r"→|←|=>|->");
    matches(s, re)
        .iter()
        .map(|m| {
            finding(
                s,
                "arrow",
                Level::Error,
                "write the relation in words".to_string(),
                m,
            )
        })
        .collect()
}

fn semicolon(s: &TextUnit) -> Vec<Finding> {
    if s.in_table || !reduce_inline(&s.text).contains("; ") {
        return vec![];
    }
    vec![finding(
        s,
        "semicolon",
        Level::Warning,
        "start a new sentence instead".to_string(),
        ";",
    )]
}

fn filler(s: &TextUnit) -> Vec<Finding> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = re(
        &RE,
        r"(?i)\b(?:delve|it'?s worth noting|it is worth noting|in summary|in conclusion|i hope this helps|let me know if|feel free to|great question|leverage|utili[sz]e|seamless(?:ly)?|robust(?:ly)?|navigate the|the landscape of|game-?changer|cutting-edge|unlock|empower|elevate|at the end of the day|going forward|moving forward|please note|as you can see|simply put|needless to say)\b",
    );
    matches(s, re)
        .iter()
        .map(|m| {
            finding(
                s,
                "filler",
                Level::Error,
                "cut it or say the plain thing".to_string(),
                m,
            )
        })
        .collect()
}

fn numbers_in_prose(s: &TextUnit) -> Vec<Finding> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = re(&RE, r"\b\d[\d,.]*%?\b");
    let count = re.find_iter(&reduce_inline(&s.text)).count();
    if s.in_table || count <= MAX_NUMERALS {
        return vec![];
    }
    vec![finding(
        s,
        "numbers-in-prose",
        Level::Warning,
        format!("{count} numbers in one sentence; put them in a table or on their own line"),
        &first_words(&s.words(), 8),
    )]
}

/// A whole sentence in bold, more than six words.
fn bold_sentence(s: &TextUnit) -> Vec<Finding> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = re(&RE, r"\*\*([^*]{2,})\*\*");
    re.captures_iter(&s.text)
        .filter_map(|c| c.get(1).map(|g| g.as_str()))
        .filter(|inner| inner.split_whitespace().count() > 6)
        .map(|inner| {
            finding(
                s,
                "bold-sentence",
                Level::Warning,
                "bold the first few words only".to_string(),
                &inner
                    .split_whitespace()
                    .take(6)
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        })
        .collect()
}

fn parenthetical(s: &TextUnit) -> Vec<Finding> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = re(&RE, r"\(([^()]+)\)");
    let text = reduce_inline(&s.text);
    re.captures_iter(&text)
        .filter(|c| {
            c.get(1)
                .is_some_and(|g| g.as_str().split_whitespace().count() >= 4)
        })
        .filter_map(|c| c.get(0).map(|g| g.as_str().to_string()))
        .map(|whole| {
            finding(
                s,
                "parenthetical",
                Level::Warning,
                "make it its own sentence".to_string(),
                &whole,
            )
        })
        .collect()
}

/// A capitalised name on first use, with no description in that sentence or the next.
pub fn undefined_names(doc: &Doc, known: &KnownNames, out: &mut Vec<Finding>) {
    static DEFINER: OnceLock<Regex> = OnceLock::new();
    let definer = re(
        &DEFINER,
        r"(?i)\b(?:is|are|was|were)\s+(?:a|an|the|one|my|our|your|its)\b|\bmeans\b|\bstands for\b|, (?:a|an|the|one|its) |: |\(",
    );
    let sentences = &doc.sentences;
    let reduced: Vec<String> = sentences.iter().map(|s| reduce_inline(&s.text)).collect();
    let lowercase = lowercase_words(sentences);
    let is_known = |name: &str| known.contains(name) || name.split(' ').all(|w| known.contains(w));
    // A real product name is almost never also spelled in lowercase in the
    // same document. A word that opens a sentence and is spelled lowercase
    // elsewhere is ordinary English, not a name that needs a description.
    let ordinary_at_start =
        |name: &str, at_start: bool| at_start && lowercase.contains(&name.to_lowercase());
    let described = |i: usize, name: &str| {
        let after_name = reduced
            .get(i)
            .and_then(|here| here.split_once(name).map(|(_, rest)| rest))
            .unwrap_or("");
        let next = reduced.get(i + 1).map_or("", String::as_str);
        definer.is_match(after_name) || definer.is_match(next)
    };
    // A name used in a table and nowhere else gets no defining sentence from
    // its position, so it is judged there. A name that also appears in the
    // document's prose is already judged there, so its table appearance is
    // dropped rather than reported a second time.
    let prose_names: HashSet<String> = sentences
        .iter()
        .filter(|s| !s.in_table)
        .flat_map(|s| candidate_names(s, &lowercase))
        .map(|c| c.name)
        .collect();
    let first_uses = sentences
        .iter()
        .enumerate()
        .flat_map(|(i, s)| {
            candidate_names(s, &lowercase)
                .into_iter()
                .map(move |c| (i, s.in_table, c.name, c.at_start))
        })
        .filter(|(_, in_table, name, _)| !(*in_table && prose_names.contains(name)))
        .scan(HashSet::new(), |seen, (i, _, name, at_start)| {
            Some(seen.insert(name.clone()).then_some((i, name, at_start)))
        })
        .flatten();
    out.extend(
        first_uses
            .filter(|(i, name, at_start)| {
                !is_known(name) && !described(*i, name) && !ordinary_at_start(name, *at_start)
            })
            .filter_map(|(i, name, at_start)| {
                let s = sentences.get(i)?;
                Some(if at_start {
                    finding(
                        s,
                        "undefined-name-at-start",
                        Level::Warning,
                        "if this is a name, add one plain sentence saying what it is".to_string(),
                        &name,
                    )
                } else {
                    finding(
                        s,
                        "undefined-name",
                        Level::Error,
                        "first use of a name; add one plain sentence saying what it is".to_string(),
                        &name,
                    )
                })
            }),
    );
}

struct Candidate {
    name: String,
    /// The name opens the sentence, where a plain word looks the same.
    at_start: bool,
}

/// Capitalised words, joined when adjacent, taken from the reduced sentence.
/// A sentence-initial word is a candidate too, unless it is a common starter
/// or an inflected form, and it is marked so the caller can soften it.
fn candidate_names(s: &TextUnit, lowercase: &HashSet<String>) -> Vec<Candidate> {
    let words = s.words();
    let first_word = words
        .iter()
        .position(|w| w.chars().any(char::is_alphabetic))
        .unwrap_or(0);
    let tokens = words.iter().enumerate().map(|(idx, raw)| {
        let w = normalize_word(raw);
        let closes_run = !raw.ends_with(|c: char| c.is_alphanumeric());
        let opens = idx == first_word && !has_inner_capital(w);
        let at_start = opens && !plain_starter(w);
        // The first word of a heading is never a candidate. A later word in
        // a heading, or any word in a list item, is dropped only when it is
        // ordinary English: a heading title or a real name in a list still
        // needs its description.
        let ordinary = lowercase.contains(&w.to_lowercase());
        let skip = if opens {
            plain_starter(w) || s.is_heading
        } else {
            (s.is_heading || s.in_list_item) && ordinary
        };
        let is_candidate = is_name_word(w)
            && !skip
            && !is_ordinary_compound(w, lowercase)
            && !is_acronym_plural(w);
        let word = is_candidate.then(|| w.to_string());
        (word, at_start, closes_run)
    });
    let (mut names, run) = tokens.fold(
        (Vec::<Candidate>::new(), Vec::<(String, bool)>::new()),
        |(mut names, mut run), (word, at_start, closes_run)| {
            match word {
                Some(w) => run.push((w, at_start)),
                None => flush_run(&mut names, &mut run),
            }
            if closes_run {
                flush_run(&mut names, &mut run);
            }
            (names, run)
        },
    );
    let mut run = run;
    flush_run(&mut names, &mut run);
    names
}

/// Strip the punctuation a word picks up from its position in a sentence,
/// and a trailing possessive, so the bare word is left for every test.
fn normalize_word(raw: &str) -> &str {
    let w = raw.trim_matches(|c: char| !c.is_alphanumeric());
    w.strip_suffix("'s").unwrap_or(w)
}

/// Every word already spelled in lowercase somewhere in the document. A
/// capitalised word that also occurs lowercase is ordinary English; a real
/// product name is almost never written both ways in the same text.
fn lowercase_words(sentences: &[TextUnit]) -> HashSet<String> {
    sentences
        .iter()
        .flat_map(TextUnit::words)
        .filter_map(|raw| {
            let w = normalize_word(&raw);
            let starts_lower = w.chars().next().is_some_and(char::is_lowercase);
            starts_lower.then(|| w.to_string())
        })
        .collect()
}

/// A hyphen or slash compound headed by an ordinary word, such as
/// `Repository-native` or `Test/gate`, is a descriptive adjective, not a
/// name. `React/TypeScript` is kept, because `react` is never spelled
/// lowercase in running text.
fn is_ordinary_compound(w: &str, lowercase: &HashSet<String>) -> bool {
    w.split_once(['-', '/'])
        .is_some_and(|(head, _)| lowercase.contains(&head.to_lowercase()))
}

/// An acronym's plural, such as `APIs` or `PRs`, is a grammatical form of a
/// known abbreviation, not a name that needs its own description.
fn is_acronym_plural(w: &str) -> bool {
    w.strip_suffix('s')
        .is_some_and(|stem| stem.len() >= 2 && stem.chars().all(char::is_uppercase))
}

fn flush_run(names: &mut Vec<Candidate>, run: &mut Vec<(String, bool)>) {
    if run.is_empty() {
        return;
    }
    let at_start = run.first().is_some_and(|(_, s)| *s);
    let name = run
        .iter()
        .map(|(w, _)| w.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    names.push(Candidate { name, at_start });
    run.clear();
}

fn plain_starter(w: &str) -> bool {
    super::names::SENTENCE_STARTERS.contains(&w)
        || w.ends_with("ing")
        || w.ends_with("ed")
        || w.ends_with("ly")
}

fn has_inner_capital(w: &str) -> bool {
    w.chars().skip(1).any(char::is_uppercase)
}

fn is_name_word(w: &str) -> bool {
    let mut chars = w.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let rest: Vec<char> = chars.collect();
    let has_lower = rest.iter().any(|c| c.is_lowercase());
    first.is_uppercase() && !rest.is_empty() && has_lower
}
