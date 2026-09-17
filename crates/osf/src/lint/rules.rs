//! Each rule is a pure function from one unit of text to zero or more
//! findings, registered with the scope it needs. The lint runs every rule
//! over the units its scope asks for. A rule that reads a configurable
//! limit or word list takes the resolved [`WritingConfig`] as well; a rule
//! that does not still takes it, unused, so every entry has one shape.

use crate::config::WritingConfig;
use osf_lint_core::segment::{reduce_inline, Doc, TextUnit};
use osf_lint_core::{run_rules, Finding, FnRule, KnownNames, Level, Rule};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::OnceLock;

const SENTENCE_RULES: &[FnRule<WritingConfig>] = &[
    FnRule::sentence("bare-reference", bare_reference),
    FnRule::sentence("reference-without-label", reference_without_label),
    FnRule::sentence("long-sentence", long_sentence),
    FnRule::sentence("em-dash", em_dash),
    FnRule::sentence("arrow", arrow),
    FnRule::sentence("semicolon", semicolon),
    FnRule::sentence("numbers-in-prose", numbers_in_prose),
    FnRule::sentence("bold-sentence", bold_sentence),
    FnRule::sentence("parenthetical", parenthetical),
];

pub fn per_sentence(doc: &Doc, cfg: &WritingConfig, fast_only: bool, out: &mut Vec<Finding>) {
    let filler_rule = FillerRule::new(&cfg.filler);
    let chat_local_rule = ChatLocalRule::new(&cfg.chat_local_phrases, &cfg.chat_local_labels);
    let mut rules: Vec<&dyn Rule<WritingConfig>> = SENTENCE_RULES
        .iter()
        .map(|r| r as &dyn Rule<WritingConfig>)
        .collect();
    rules.push(&filler_rule);
    rules.push(&chat_local_rule);
    out.extend(run_rules(doc, &rules, cfg, fast_only));
}

/// Every rule id this lint can report, for validating a suppression's rule id.
pub fn rule_ids() -> Vec<&'static str> {
    SENTENCE_RULES
        .iter()
        .map(Rule::id)
        .chain([
            "filler",
            "chat-local-reference",
            "heading-in-short-text",
            "undefined-name",
            "undefined-name-at-start",
        ])
        .collect()
}

pub fn headings_in_short_text(doc: &Doc, cfg: &WritingConfig, out: &mut Vec<Finding>) {
    let limit = cfg.short_text_words;
    if doc.word_count >= limit {
        return;
    }
    out.extend(doc.headings.iter().map(|&line| {
        Finding::new(
            "heading-in-short-text",
            Level::Error,
            line,
            format!("a text under {limit} words has a heading; use a sentence or a table instead"),
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

fn matches(s: &TextUnit, pattern: &Regex) -> Vec<String> {
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
fn bare_reference(s: &TextUnit, _cfg: &WritingConfig) -> Vec<Finding> {
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

/// Replaces every byte inside a range in `ranges` with an ASCII space, one
/// space per byte of the original character, so the result stays the same
/// length and every other byte offset in `text` still lines up.
fn mask_ranges(text: &str, ranges: &[Range<usize>]) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, ch) in text.char_indices() {
        if ranges.iter().any(|r| r.contains(&i)) {
            out.extend(std::iter::repeat_n(' ', ch.len_utf8()));
        } else {
            out.push(ch);
        }
    }
    out
}

/// `owner/repo#N` or `repo#N` must carry a bracketed description; a link is
/// expected. Judged per occurrence: a reference matched once inside a link
/// and again bare, in the same sentence, is linked only the first time.
fn reference_without_label(s: &TextUnit, _cfg: &WritingConfig) -> Vec<Finding> {
    static REF: OnceLock<Regex> = OnceLock::new();
    static LINK: OnceLock<Regex> = OnceLock::new();
    static CODE: OnceLock<Regex> = OnceLock::new();
    let ref_pattern = re(&REF, r"(?:[\w.-]+/)?[\w.-]+#\d+");
    let link_pattern = re(&LINK, r"\[([^\]]*)\]\([^)]*\)");
    let code_pattern = re(&CODE, r"`[^`]*`");

    let raw = &s.text;
    let code_ranges: Vec<Range<usize>> = code_pattern.find_iter(raw).map(|m| m.range()).collect();
    let masked = mask_ranges(raw, &code_ranges);
    let link_spans: Vec<Range<usize>> = link_pattern
        .captures_iter(&masked)
        .filter_map(|c| c.get(1))
        .map(|g| g.range())
        .collect();

    ref_pattern
        .find_iter(&masked)
        .flat_map(|m| {
            let reference = m.as_str().to_string();
            let labelled = raw
                .get(m.end()..)
                .is_some_and(|rest| rest.trim_start().starts_with('('));
            let linked = link_spans
                .iter()
                .any(|span| span.start <= m.start() && m.end() <= span.end);
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

/// Builds `(?i)\b(?:a1|a2|...)\b` from a non-empty list of ready-made regex
/// alternatives, or nothing at all for an empty one. An empty alternation,
/// `(?:)`, matches a zero-width span at nearly every word boundary; that is
/// not the same thing as "nothing configured, so this never matches."
/// A caller escapes its own words first; a literal alternative is not safe
/// to pass here unescaped.
fn word_boundary_alternation(alternatives: &[String]) -> Option<Regex> {
    if alternatives.is_empty() {
        return None;
    }
    let pattern = format!(r"(?i)\b(?:{})\b", alternatives.join("|"));
    Some(Regex::new(&pattern).expect("word-boundary alternation pattern compiles"))
}

/// Words that only mean something inside one conversation. Built from the
/// resolved config's phrase and label lists, so it cannot be a static
/// [`FnRule`]; it is a small [`Rule`] impl instead, constructed once per lint.
struct ChatLocalRule {
    /// `None` when both configured lists are empty: the rule fires only
    /// from `step`, below, in that case.
    phrases: Option<Regex>,
    step: Regex,
}

impl ChatLocalRule {
    fn new(phrases: &[String], labels: &[String]) -> Self {
        let fixed = phrases.iter().map(|p| regex::escape(p));
        let labelled = labels
            .iter()
            .map(|l| format!(r"{}\s+\d+", regex::escape(l)));
        let alternatives: Vec<String> = fixed.chain(labelled).collect();
        ChatLocalRule {
            phrases: word_boundary_alternation(&alternatives),
            step: Regex::new(r"(?i)\b(step \d+)\b[,:]?\s*(\S?)").expect("step pattern compiles"),
        }
    }
}

impl Rule<WritingConfig> for ChatLocalRule {
    fn id(&self) -> &'static str {
        "chat-local-reference"
    }

    fn scope(&self) -> osf_lint_core::Scope {
        osf_lint_core::Scope::Sentence
    }

    fn check(&self, s: &TextUnit, _cfg: &WritingConfig) -> Vec<Finding> {
        let text = reduce_inline(&s.text);
        let bare_steps = self
            .step
            .captures_iter(&text)
            .filter(|c| {
                !c.get(2)
                    .and_then(|g| g.as_str().chars().next())
                    .is_some_and(|ch| ch.is_alphabetic() || ch == '(')
            })
            .filter_map(|c| c.get(1).map(|g| g.as_str().to_string()));
        let phrase_matches = self
            .phrases
            .as_ref()
            .map(|re| matches(s, re))
            .unwrap_or_default();
        phrase_matches
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
}

fn long_sentence(s: &TextUnit, cfg: &WritingConfig) -> Vec<Finding> {
    let words = s.words();
    let n = words.len();
    if s.in_table {
        return vec![];
    }
    let max = cfg.max_sentence_words;
    if n > max {
        return vec![finding(
            s,
            "long-sentence",
            Level::Error,
            format!("{n} words; split it, {max} is the limit"),
            &format!("{}...", first_words(&words, 8)),
        )];
    }
    match cfg.warn_sentence_words {
        Some(warn) if n > warn => vec![finding(
            s,
            "long-sentence",
            Level::Warning,
            format!("{n} words; split it, {warn} is the warning limit"),
            &format!("{}...", first_words(&words, 8)),
        )],
        _ => vec![],
    }
}

fn em_dash(s: &TextUnit, _cfg: &WritingConfig) -> Vec<Finding> {
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

fn arrow(s: &TextUnit, _cfg: &WritingConfig) -> Vec<Finding> {
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

fn semicolon(s: &TextUnit, _cfg: &WritingConfig) -> Vec<Finding> {
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

/// Words and phrases that add length and no meaning. Built from the
/// resolved config's list, so it is a small [`Rule`] impl, not a static
/// [`FnRule`], constructed once per lint.
struct FillerRule {
    /// `None` when the configured list is empty: the rule never matches.
    re: Option<Regex>,
}

impl FillerRule {
    fn new(words: &[String]) -> Self {
        let escaped: Vec<String> = words.iter().map(|w| regex::escape(w)).collect();
        FillerRule {
            re: word_boundary_alternation(&escaped),
        }
    }
}

impl Rule<WritingConfig> for FillerRule {
    fn id(&self) -> &'static str {
        "filler"
    }

    fn scope(&self) -> osf_lint_core::Scope {
        osf_lint_core::Scope::Sentence
    }

    fn check(&self, s: &TextUnit, _cfg: &WritingConfig) -> Vec<Finding> {
        let Some(re) = &self.re else {
            return vec![];
        };
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
}

fn numbers_in_prose(s: &TextUnit, cfg: &WritingConfig) -> Vec<Finding> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = re(&RE, r"\b\d[\d,.]*%?\b");
    let count = re.find_iter(&reduce_inline(&s.text)).count();
    let max = cfg.max_numerals;
    if s.in_table || count <= max {
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

/// A whole sentence in bold, more than six words. Not a long bold span
/// inside an otherwise plain sentence; only a sentence bolded start to end.
fn bold_sentence(s: &TextUnit, _cfg: &WritingConfig) -> Vec<Finding> {
    let trimmed = s.text.trim();
    let trimmed = trimmed
        .strip_suffix(|c: char| matches!(c, '.' | '!' | '?'))
        .unwrap_or(trimmed);
    let Some(inner) = trimmed
        .strip_prefix("**")
        .and_then(|t| t.strip_suffix("**"))
    else {
        return vec![];
    };
    if inner.is_empty() || inner.contains("**") || inner.split_whitespace().count() <= 6 {
        return vec![];
    }
    vec![finding(
        s,
        "bold-sentence",
        Level::Warning,
        "bold the first few words only".to_string(),
        &inner
            .split_whitespace()
            .take(6)
            .collect::<Vec<_>>()
            .join(" "),
    )]
}

fn parenthetical(s: &TextUnit, _cfg: &WritingConfig) -> Vec<Finding> {
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

/// A capitalised name whose first use is on the repository's must-explain
/// list, or merely looks like a name, with no description in that sentence
/// or the next.
///
/// This starts from positive evidence rather than a list of exceptions
/// subtracted from every capital letter in the text. A candidate earns a
/// report one of two ways:
///
/// - it is on `cfg.must_explain_names`, the repository's own curated list
///   of names worth explaining: a missing explanation is then certain, so
///   this is an error, [`osf_lint_core::Evidence::Deterministic`] (the
///   default a plain [`Finding`] already carries).
/// - it merely looks like a name: an internal capital such as `GitHub` or
///   `DuckDB`, a digit in one of its words, a domain-like suffix such as
///   `.dev`, or a multi-word capitalised run repeated more than once in
///   the document. None of these prove a name, only suggest one, so this
///   is a warning, and it carries [`osf_lint_core::Evidence::Statistical`].
///
/// A capitalised word with neither kind of evidence, such as an ordinary
/// word that only opens a sentence or a heading, is never reported: a bare
/// capital letter proves nothing on its own, in any position.
pub fn undefined_names(doc: &Doc, known: &KnownNames, cfg: &WritingConfig, out: &mut Vec<Finding>) {
    static DEFINER: OnceLock<Regex> = OnceLock::new();
    let definer = re(
        &DEFINER,
        r"(?i)\b(?:is|are|was|were)\s+(?:a|an|the|one|my|our|your|its)\b|\bmeans\b|\bstands for\b|, (?:a|an|the|one|its) |: |\(",
    );
    let sentences = &doc.sentences;
    let reduced: Vec<String> = sentences.iter().map(|s| reduce_inline(&s.text)).collect();
    // A run that opens with a known name, such as "GitHub Apps" opening with
    // the built-in "GitHub", is a specific case of the known thing; the rest
    // of the run does not need its own entry in the known-names list. A
    // generic word such as "The" does not qualify: it opens plenty of
    // ordinary runs by grammar alone, so `is_name_head` excludes it even
    // though it is itself known.
    let is_known = |name: &str| {
        known.contains(name)
            || name.split(' ').all(|w| known.contains(w))
            || name
                .split(' ')
                .next()
                .is_some_and(|first| known.contains(first) && super::names::is_name_head(first))
    };
    let all_candidates: Vec<(usize, bool, String)> = sentences
        .iter()
        .enumerate()
        .flat_map(|(i, s)| {
            candidate_names(s)
                .into_iter()
                .map(move |n| (i, s.in_table, n))
        })
        .collect();
    // A multi-word run the document uses more than once is unlikely to be a
    // one-off descriptive phrase; that repetition is itself evidence of a
    // name, alongside an internal capital, a digit, or a domain suffix.
    let mut run_counts: HashMap<String, usize> = HashMap::new();
    for (_, _, name) in &all_candidates {
        if name.contains(' ') {
            *run_counts.entry(name.clone()).or_insert(0) += 1;
        }
    }
    // The described-in-the-next-sentence check only counts when that
    // sentence actually mentions the name again; otherwise an unrelated
    // colon or parenthesis two sentences away from the real subject would
    // wrongly clear a genuine finding. The search anchors on the run's
    // first word rather than the whole joined name: a run's name is its
    // words with a single space between them, which never reappears
    // character for character once a possessive or other suffix breaks it
    // in the source, as "the Foundation's Work" does against a run of
    // "Foundation Work". The first word alone still finds a real
    // explanation next to it without demanding the whole run repeat
    // itself verbatim.
    let described = |i: usize, name: &str| {
        let anchor = name.split(' ').next().unwrap_or(name);
        let after_name = reduced
            .get(i)
            .and_then(|here| here.split_once(anchor).map(|(_, rest)| rest))
            .unwrap_or("");
        let next_after_name = reduced
            .get(i + 1)
            .and_then(|next| next.split_once(anchor).map(|(_, rest)| rest));
        definer.is_match(after_name) || next_after_name.is_some_and(|rest| definer.is_match(rest))
    };
    // A name used in a table and also in prose is judged from the prose:
    // its table appearance carries no defining sentence around it, so
    // dropping it here avoids reporting the same name from both places.
    let prose_names: HashSet<String> = all_candidates
        .iter()
        .filter(|(_, in_table, _)| !in_table)
        .map(|(_, _, name)| name.clone())
        .collect();
    let first_uses = all_candidates
        .into_iter()
        .filter(|(_, in_table, name)| !(*in_table && prose_names.contains(name)))
        .scan(HashSet::new(), |seen, (i, _in_table, name)| {
            Some(seen.insert(name.clone()).then_some((i, name)))
        })
        .flatten();
    out.extend(first_uses.filter_map(|(i, name)| {
        if is_known(&name) || described(i, &name) {
            return None;
        }
        let s = sentences.get(i)?;
        if cfg.must_explain_names.contains(&name) {
            return Some(finding(
                s,
                "undefined-name",
                Level::Error,
                "this name is on the project's must-explain list; add one plain sentence \
                 saying what it is"
                    .to_string(),
                &name,
            ));
        }
        looks_like_a_name(&name, &run_counts).then(|| {
            finding(
                s,
                "undefined-name-at-start",
                Level::Warning,
                "if this is a name, add one plain sentence saying what it is".to_string(),
                &name,
            )
            .with_evidence(osf_lint_core::Evidence::Statistical)
        })
    }));
}

/// Whether `name` carries any of the three kinds of weak, statistical
/// evidence that it names something specific, rather than being an
/// ordinary capitalised word: an internal capital such as `GitHub`'s or
/// `DuckDB`'s, a digit in one of its words, a domain-like suffix such as
/// `.dev`, or, only for a multi-word run, more than one occurrence of that
/// exact run elsewhere in the document.
fn looks_like_a_name(name: &str, run_counts: &HashMap<String, usize>) -> bool {
    let words: Vec<&str> = name.split(' ').collect();
    words.iter().any(|w| has_inner_capital(w))
        || words.iter().any(|w| w.chars().any(|c| c.is_ascii_digit()))
        || words.iter().any(|w| has_domain_suffix(w))
        || (words.len() > 1 && run_counts.get(name).copied().unwrap_or(0) > 1)
}

/// A dot followed by two or more letters, the shape of a domain's suffix,
/// such as ".dev" or ".io".
fn has_domain_suffix(w: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(&RE, r"(?i)\.[a-z]{2,}").is_match(w)
}

/// Capitalised words, joined when adjacent, taken from the reduced
/// sentence. Whether a run is reported never depends on position, unlike
/// the older design: a word that only looks like a name because it opens a
/// sentence is still returned as its own candidate, left for
/// [`looks_like_a_name`] and the must-explain list to judge on their own
/// evidence, which a mere capital letter never supplies by itself.
///
/// One position rule survives, for a narrower reason than reporting: an
/// ordinary sentence's own first word never joins the word after it into
/// one run. "Open Sublime Merge now." must not merge into "Open Sublime
/// Merge": "Open" is capitalised only because it opens the sentence, and
/// that tells a reader nothing about the word after it. A heading or a
/// list item is exempt, since both are normally a title or a labelled
/// term in full, such as `Prison Architect` or `JetBrains IDEs`, where the
/// first word is as much the name as the rest.
fn candidate_names(s: &TextUnit) -> Vec<String> {
    let words = s.words();
    let first_word = words
        .iter()
        .position(|w| w.chars().any(char::is_alphabetic))
        .unwrap_or(0);
    let titled = s.is_heading || s.in_list_item;
    let tokens = words.iter().enumerate().map(|(idx, raw)| {
        let w = normalize_word(raw);
        let closes_run =
            !raw.ends_with(|c: char| c.is_alphanumeric()) || (idx == first_word && !titled);
        (is_name_word(w).then(|| w.to_string()), closes_run)
    });
    let (mut names, run) = tokens.fold(
        (Vec::<String>::new(), Vec::<String>::new()),
        |(mut names, mut run), (word, closes_run)| {
            match word {
                Some(w) => run.push(w),
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
    // A single word ending in a lowercase "s" after an all-caps stem, such
    // as "APIs" or "PRs", is a plural of a known abbreviation. Its internal
    // capital comes from that grammar alone, not from naming something
    // specific, so it never reaches the evidence check. A multi-word
    // candidate is never touched here: testing one of its words in
    // isolation would tear a real name such as "JetBrains IDEs" in half.
    names.retain(|name| name.contains(' ') || !is_acronym_plural(name));
    names
}

/// Strip the punctuation a word picks up from its position in a sentence,
/// and a trailing possessive, so the bare word is left for every test.
fn normalize_word(raw: &str) -> &str {
    let w = raw.trim_matches(|c: char| !c.is_alphanumeric());
    w.strip_suffix("'s").unwrap_or(w)
}

/// An acronym's plural, such as `APIs` or `PRs`, is a grammatical form of a
/// known abbreviation, not a name that needs its own description.
fn is_acronym_plural(w: &str) -> bool {
    w.strip_suffix('s')
        .is_some_and(|stem| stem.len() >= 2 && stem.chars().all(char::is_uppercase))
}

fn flush_run(names: &mut Vec<String>, run: &mut Vec<String>) {
    if run.is_empty() {
        return;
    }
    names.push(run.join(" "));
    run.clear();
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
    first.is_uppercase() && !rest.is_empty() && has_lower && !is_contraction(w)
}

/// A short lowercase tail after an apostrophe, such as "'ll" in "We'll" or
/// "'t" in "Don't", marks a contraction. A name is never spelled this way,
/// so a contraction is not a name candidate even when it starts uppercase.
fn is_contraction(w: &str) -> bool {
    const TAILS: &[&str] = &["ll", "d", "s", "re", "ve", "m", "t"];
    w.rsplit_once('\'')
        .is_some_and(|(_, tail)| TAILS.contains(&tail))
}
