//! Precision and recall for the writing lint against the unplaceable-reference fixture, per kind and in total.

use osf::config::WritingConfig;
use osf::lints::{load_known_names, Context, KnownNames};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Rule ids this measure treats as flagged.
const MEASURED_RULE_IDS: &[&str] = &["unplaceable-reference"];

/// Kind reported on its own row, excluded from every total.
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
    target: String,
    text: String,
}

/// Parses a fixture's own `osf-unplaceable` HTML-comment header.
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
    let mut target = None;
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
            "target" => target = Some(value.trim().to_string()),
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
    let target = target.unwrap_or_else(|| panic!("{}: header has no target", path.display()));
    assert!(
        text.contains(&target),
        "{}: target {target:?} is not literal text in the paragraph",
        path.display()
    );
    reason.unwrap_or_else(|| panic!("{}: header has no reason", path.display()));

    FixtureCase {
        file: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        label,
        kind,
        target,
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

/// Whether a finding's excerpt and a fixture's target text refer to the same span, by containment either way.
fn overlaps(excerpt: &str, target: &str) -> bool {
    let excerpt = excerpt.trim().to_lowercase();
    let target = target.trim().to_lowercase();
    !excerpt.is_empty()
        && !target.is_empty()
        && (target.contains(&excerpt) || excerpt.contains(&target))
}

#[derive(Default, Clone, Copy)]
struct Stats {
    true_positive: u32,
    false_positive: u32,
    false_negative: u32,
    true_negative: u32,
    /// A measured finding that lands away from the fixture's own target.
    stray: u32,
}

impl Stats {
    fn record(&mut self, label: Label, hit_on_target: bool) {
        match (label, hit_on_target) {
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

    fn add(&mut self, other: Stats) {
        self.true_positive += other.true_positive;
        self.false_positive += other.false_positive;
        self.false_negative += other.false_negative;
        self.true_negative += other.true_negative;
        self.stray += other.stray;
    }
}

fn percent(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_string(), |v| format!("{:.1}%", v * 100.0))
}

fn print_row(label: &str, stats: &Stats) {
    println!(
        "| {label} | {} | {} | {} | {} | {} | {} | {} |",
        stats.true_positive,
        stats.false_positive,
        stats.false_negative,
        stats.true_negative,
        stats.stray,
        percent(stats.precision()),
        percent(stats.recall())
    );
}

/// Every kind's stats added together, the excluded kind left out.
fn total_excluding_model(by_kind: &BTreeMap<String, Stats>) -> Stats {
    let mut total = Stats::default();
    for (kind, stats) in by_kind {
        if kind != EXCLUDED_KIND {
            total.add(*stats);
        }
    }
    total
}

fn print_table(by_kind: &BTreeMap<String, Stats>) {
    println!("| Kind | TP | FP | FN | TN | Stray | Precision | Recall |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
    for (kind, stats) in by_kind {
        if kind == EXCLUDED_KIND {
            continue;
        }
        print_row(kind, stats);
    }
    print_row("**Total**", &total_excluding_model(by_kind));
    if let Some(model) = by_kind.get(EXCLUDED_KIND) {
        print_row("model (excluded from totals)", model);
    }
}

/// Prints the precision-and-recall table `MEASURED_RULE_IDS` produces, and requires perfect recall and precision.
#[test]
fn writing_lint_precision_and_recall_on_the_unplaceable_fixture() {
    let cases = load_all_fixtures();
    assert_eq!(
        cases.len(),
        36,
        "expected 36 fixture files under tests/fixtures/writing/unplaceable"
    );

    let known = known();
    let cfg = WritingConfig::default();
    let mut by_kind: BTreeMap<String, Stats> = BTreeMap::new();

    for case in &cases {
        let findings = osf::lints::writing::lint_writing(
            &case.text,
            &known,
            &cfg,
            Context::Document,
            false,
            false,
        );
        let measured: Vec<_> = findings
            .iter()
            .filter(|f| MEASURED_RULE_IDS.contains(&f.rule))
            .collect();
        let hit_on_target = measured.iter().any(|f| overlaps(&f.excerpt, &case.target));
        let stray_count = measured
            .iter()
            .filter(|f| !overlaps(&f.excerpt, &case.target))
            .count();

        let stats = by_kind.entry(case.kind.clone()).or_default();
        stats.record(case.label, hit_on_target);
        stats.stray += u32::try_from(stray_count).unwrap_or(u32::MAX);
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

    let total = total_excluding_model(&by_kind);
    assert_eq!(total.precision(), Some(1.0), "total precision must be 100%");
    assert_eq!(total.recall(), Some(1.0), "total recall must be 100%");
    assert_eq!(
        total.stray, 0,
        "no measured finding may land away from its own fixture's target"
    );
}
