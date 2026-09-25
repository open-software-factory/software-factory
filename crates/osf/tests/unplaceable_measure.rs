//! Precision and recall for the writing lint against the fixture that
//! stands in for `unplaceable-reference`: one paragraph per file under
//! `tests/fixtures/writing/unplaceable/`, each declaring whether it should
//! resolve (`placeable`) or stay a finding (`unplaceable`), its kind, and
//! a one-line reason.
//!
//! This file measures whichever rule ids `MEASURED_RULE_IDS` names, so the
//! same test runs against the six rules replaced today and, later, against
//! the one rule that replaces them: only that constant changes.

use osf::config::WritingConfig;
use osf::lints::{load_known_names, Context, KnownNames};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The rule ids this measure treats as a "flagged" finding. Swap this to
/// `&["unplaceable-reference"]` once that rule replaces the six below; the
/// rest of this file does not change.
const MEASURED_RULE_IDS: &[&str] = &[
    "bare-reference",
    "reference-without-label",
    "reference-without-link",
    "chat-local-reference",
    "undefined-name",
    "undefined-name-at-start",
];

/// The kind reported on its own row and left out of every total: a
/// deterministic layer is not expected to place or flag it.
const EXCLUDED_KIND: &str = "model";

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/writing/unplaceable")
}

fn known() -> KnownNames {
    load_known_names(&[], None).expect("built-in names load")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Label {
    Placeable,
    Unplaceable,
}

struct FixtureCase {
    file: String,
    label: Label,
    kind: String,
    text: String,
}

/// Reads the `label`, `kind` and `reason` a fixture declares in its own
/// `osf-unplaceable` HTML-comment header, the same comment-block shape the
/// writing lint's own `osf-expect` marker uses. A header field this parser
/// cannot make sense of is a fixture-authoring mistake, not a soft failure.
fn parse_fixture(path: &Path) -> FixtureCase {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{} reads: {e}", path.display()));
    let marker = "<!-- osf-unplaceable";
    let start = text
        .find(marker)
        .unwrap_or_else(|| panic!("{}: no osf-unplaceable header", path.display()));
    let after = &text[start + marker.len()..];
    let end = after
        .find("-->")
        .unwrap_or_else(|| panic!("{}: unterminated osf-unplaceable header", path.display()));
    let block = &after[..end];

    let mut label = None;
    let mut kind = None;
    let mut reason = None;
    for line in block.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line
            .split_once(':')
            .unwrap_or_else(|| panic!("{}: header line {line:?} has no ':'", path.display()));
        match key.trim() {
            "label" => label = Some(value.trim().to_string()),
            "kind" => kind = Some(value.trim().to_string()),
            "reason" => reason = Some(value.trim().to_string()),
            other => panic!("{}: unknown header key {other:?}", path.display()),
        }
    }
    let label = match label.as_deref() {
        Some("placeable") => Label::Placeable,
        Some("unplaceable") => Label::Unplaceable,
        other => panic!(
            "{}: label must be placeable or unplaceable, got {other:?}",
            path.display()
        ),
    };
    let kind = kind.unwrap_or_else(|| panic!("{}: header has no kind", path.display()));
    reason.unwrap_or_else(|| panic!("{}: header has no reason", path.display()));

    FixtureCase {
        file: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        label,
        kind,
        text,
    }
}

fn load_all_fixtures() -> Vec<FixtureCase> {
    let dir = fixtures_dir();
    let mut cases: Vec<FixtureCase> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{} reads: {e}", dir.display()))
        .map(|entry| entry.expect("directory entry reads").path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("md"))
        .map(|path| parse_fixture(&path))
        .collect();
    cases.sort_by(|a, b| a.file.cmp(&b.file));
    cases
}

#[derive(Default, Clone, Copy)]
struct Stats {
    true_positive: u32,
    false_positive: u32,
    false_negative: u32,
    true_negative: u32,
}

impl Stats {
    fn record(&mut self, label: Label, flagged: bool) {
        match (label, flagged) {
            (Label::Unplaceable, true) => self.true_positive += 1,
            (Label::Unplaceable, false) => self.false_negative += 1,
            (Label::Placeable, true) => self.false_positive += 1,
            (Label::Placeable, false) => self.true_negative += 1,
        }
    }

    fn precision(&self) -> Option<f64> {
        let denom = self.true_positive + self.false_positive;
        (denom > 0).then(|| f64::from(self.true_positive) / f64::from(denom))
    }

    fn recall(&self) -> Option<f64> {
        let denom = self.true_positive + self.false_negative;
        (denom > 0).then(|| f64::from(self.true_positive) / f64::from(denom))
    }
}

fn percent(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_string(), |v| format!("{:.1}%", v * 100.0))
}

fn print_table(by_kind: &BTreeMap<String, Stats>) {
    println!("| Kind | TP | FP | FN | TN | Precision | Recall |");
    println!("| --- | --- | --- | --- | --- | --- | --- |");
    let mut total = Stats::default();
    for (kind, stats) in by_kind {
        if kind == EXCLUDED_KIND {
            continue;
        }
        println!(
            "| {kind} | {} | {} | {} | {} | {} | {} |",
            stats.true_positive,
            stats.false_positive,
            stats.false_negative,
            stats.true_negative,
            percent(stats.precision()),
            percent(stats.recall())
        );
        total.true_positive += stats.true_positive;
        total.false_positive += stats.false_positive;
        total.false_negative += stats.false_negative;
        total.true_negative += stats.true_negative;
    }
    println!(
        "| **Total** | {} | {} | {} | {} | {} | {} |",
        total.true_positive,
        total.false_positive,
        total.false_negative,
        total.true_negative,
        percent(total.precision()),
        percent(total.recall())
    );
    if let Some(model) = by_kind.get(EXCLUDED_KIND) {
        println!(
            "| model (excluded from totals) | {} | {} | {} | {} | {} | {} |",
            model.true_positive,
            model.false_positive,
            model.false_negative,
            model.true_negative,
            percent(model.precision()),
            percent(model.recall())
        );
    }
}

/// Runs every unplaceable fixture through the writing lint in message
/// context, the same context a reply or a hook check runs in, and prints
/// the precision-and-recall table `MEASURED_RULE_IDS` produces today.
///
/// This only asserts that every fixture file parses and that its declared
/// kind and label are well formed; it asserts nothing about the printed
/// numbers. Thresholds land once `unplaceable-reference` replaces the rule
/// ids above, and this file's only change at that point is the constant.
#[test]
fn writing_lint_precision_and_recall_on_the_unplaceable_fixture() {
    let cases = load_all_fixtures();
    assert_eq!(
        cases.len(),
        33,
        "expected 33 fixture files under tests/fixtures/writing/unplaceable"
    );

    let known = known();
    let cfg = WritingConfig::default();
    let mut by_kind: BTreeMap<String, Stats> = BTreeMap::new();

    for case in &cases {
        let findings = osf::lints::writing::lint_writing(
            &case.text,
            &known,
            &cfg,
            Context::Transcript,
            false,
            false,
        );
        let flagged = findings.iter().any(|f| MEASURED_RULE_IDS.contains(&f.rule));
        by_kind
            .entry(case.kind.clone())
            .or_default()
            .record(case.label, flagged);
    }

    print_table(&by_kind);

    let expected_kinds = ["number", "phrase", "time", "name", "model"];
    for kind in expected_kinds {
        assert!(
            by_kind.contains_key(kind),
            "no fixture declared kind {kind:?}"
        );
    }
    let kinds: Vec<&String> = by_kind.keys().collect();
    assert_eq!(
        by_kind.len(),
        expected_kinds.len(),
        "an unexpected kind was declared: {kinds:?}"
    );
}
