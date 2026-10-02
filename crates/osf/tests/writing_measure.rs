//! Measure tests for the two rules that guard text which has to last:
//! `count-word` and `relative-time`. Each rule has a positive and a negative
//! fixture under `tests/fixtures/writing-measure`, one sentence per line,
//! copied from this repository's own documents wherever they hold one. The
//! test counts true and false findings over every line and requires a
//! precision and a recall of exactly one.
//!
//! A line in a positive file reads `sentence ||| excerpts ||| source`, with
//! the excerpts the rule must report, separated by `;`. A line in a negative
//! file reads `sentence ||| source`. A line that starts with `//` is a note.

use osf::config::WritingConfig;
use osf::lints::writing::lint_writing;
use osf::lints::{load_known_names, Context, Level};
use std::fs;
use std::path::{Path, PathBuf};

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/writing-measure")
}

struct Example {
    sentence: String,
    expected: Vec<String>,
    source: String,
}

fn read(name: &str, positive: bool) -> Vec<Example> {
    let text = fs::read_to_string(dir().join(name)).unwrap_or_else(|e| panic!("{name} reads: {e}"));
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with("//"))
        .map(|line| {
            let parts: Vec<&str> = line.split(" ||| ").collect();
            match (positive, parts.as_slice()) {
                (true, [sentence, expected, source]) => Example {
                    sentence: (*sentence).to_string(),
                    expected: expected.split(';').map(str::to_string).collect(),
                    source: (*source).to_string(),
                },
                (false, [sentence, source]) => Example {
                    sentence: (*sentence).to_string(),
                    expected: Vec::new(),
                    source: (*source).to_string(),
                },
                _ => panic!("{name}: bad line {line:?}"),
            }
        })
        .collect()
}

fn excerpts(rule: &str, sentence: &str, context: Context) -> Vec<(String, Level)> {
    let known = load_known_names(&[], None).expect("built-in names load");
    lint_writing(
        &format!("{sentence}\n"),
        &known,
        &WritingConfig::default(),
        context,
        false,
        false,
    )
    .into_iter()
    .filter(|f| f.rule == rule)
    .map(|f| (f.excerpt, f.level))
    .collect()
}

#[derive(Default, Debug)]
struct Counts {
    true_positive: usize,
    false_positive: usize,
    false_negative: usize,
}

impl Counts {
    /// Precision in thousandths: 1000 means every finding was wanted.
    fn precision(&self) -> usize {
        let found = self.true_positive + self.false_positive;
        if found == 0 {
            return 1000;
        }
        self.true_positive * 1000 / found
    }

    /// Recall in thousandths: 1000 means every wanted finding was made.
    fn recall(&self) -> usize {
        let wanted = self.true_positive + self.false_negative;
        if wanted == 0 {
            return 1000;
        }
        self.true_positive * 1000 / wanted
    }
}

fn measure(rule: &str, level: Level) {
    let positives = read(&format!("{rule}.positive.txt"), true);
    let negatives = read(&format!("{rule}.negative.txt"), false);
    assert!(
        positives.len() >= 20,
        "{rule}: {} positive examples",
        positives.len()
    );
    assert!(
        negatives.len() >= 20,
        "{rule}: {} negative examples",
        negatives.len()
    );
    let mut counts = Counts::default();
    let mut misses = Vec::new();
    for example in positives.iter().chain(negatives.iter()) {
        let found = excerpts(rule, &example.sentence, Context::Document);
        for (excerpt, found_level) in &found {
            assert_eq!(*found_level, level, "{rule}: {}", example.sentence);
            if example.expected.contains(excerpt) {
                counts.true_positive += 1;
            } else {
                counts.false_positive += 1;
                misses.push(format!(
                    "unexpected [{excerpt}] in {:?} ({})",
                    example.sentence, example.source
                ));
            }
        }
        for want in &example.expected {
            if !found.iter().any(|(excerpt, _)| excerpt == want) {
                counts.false_negative += 1;
                misses.push(format!(
                    "missed [{want}] in {:?} ({})",
                    example.sentence, example.source
                ));
            }
        }
    }
    eprintln!(
        "{rule}: {} positive and {} negative examples, precision {} of 1000, recall {} of 1000, {counts:?}",
        positives.len(),
        negatives.len(),
        counts.precision(),
        counts.recall()
    );
    assert!(misses.is_empty(), "{rule}:\n{}", misses.join("\n"));
    assert_eq!(counts.precision(), 1000);
    assert_eq!(counts.recall(), 1000);
}

#[test]
fn count_word_has_full_precision_and_recall_on_its_fixtures() {
    measure("count-word", Level::Error);
}

#[test]
fn relative_time_has_full_precision_and_recall_on_its_fixtures() {
    measure("relative-time", Level::Warning);
}

/// Neither rule runs on a chat transcript, which is not kept.
#[test]
fn neither_rule_runs_in_a_transcript() {
    for rule in ["count-word", "relative-time"] {
        let all: Vec<Example> = read(&format!("{rule}.positive.txt"), true);
        for example in &all {
            let found = excerpts(rule, &example.sentence, Context::Transcript);
            assert!(found.is_empty(), "{rule} fired in a transcript: {found:?}");
        }
    }
}

/// Both rules run, at a fixed level, in every kind of durable text.
#[test]
fn both_rules_hold_their_level_in_every_durable_context() {
    let cases = [
        ("count-word", "The pipeline has three stages.", Level::Error),
        (
            "relative-time",
            "The cache is currently disabled.",
            Level::Warning,
        ),
    ];
    for (rule, sentence, level) in cases {
        for context in [Context::Document, Context::Commit, Context::Skill] {
            let found = excerpts(rule, sentence, context);
            let levels: Vec<Level> = found.iter().map(|(_, l)| *l).collect();
            assert_eq!(levels, vec![level], "{rule} in {context:?}");
        }
    }
}

/// A fenced code block is never prose, so a count or a time word in it is left alone.
#[test]
fn fenced_code_is_left_alone() {
    let text = "```\nThe pipeline has three stages and runs today.\n```\n";
    let known = load_known_names(&[], None).expect("built-in names load");
    let found = lint_writing(
        text,
        &known,
        &WritingConfig::default(),
        Context::Document,
        false,
        false,
    );
    assert!(
        found
            .iter()
            .all(|f| f.rule != "count-word" && f.rule != "relative-time"),
        "{found:?}"
    );
}

/// The finding carries the owner's remediation text.
#[test]
fn the_remediation_text_is_the_one_the_owner_asked_for() {
    let count = excerpts_with_message("count-word", "The pipeline has three stages.");
    assert!(
        count.starts_with(
            "Name the things, or say each, every or these, so the text stays true when the list changes."
        ),
        "{count}"
    );
    let time = excerpts_with_message("relative-time", "The cache is currently disabled.");
    assert!(
        time.starts_with("State the date, or state the fact without a time word."),
        "{time}"
    );
}

fn excerpts_with_message(rule: &str, sentence: &str) -> String {
    let known = load_known_names(&[], None).expect("built-in names load");
    lint_writing(
        &format!("{sentence}\n"),
        &known,
        &WritingConfig::default(),
        Context::Document,
        false,
        false,
    )
    .into_iter()
    .find(|f| f.rule == rule)
    .map_or_else(
        || panic!("{rule} did not fire on {sentence:?}"),
        |f| f.message,
    )
}
