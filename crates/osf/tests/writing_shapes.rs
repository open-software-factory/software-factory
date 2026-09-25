//! Integration tests for the rhetorical-shape rules added to the writing
//! lint: the ones that catch a sentence or a paragraph's shape rather than
//! a single banned word or phrase. Fixtures live under
//! `tests/fixtures/writing`, one positive and one negative file per rule,
//! each declaring its exact expected rule ids with an `osf-expect` marker.
//!
//! Every fixture is also checked, as a whole, by
//! `every_fixture_matches_its_declaration`: that test is the alarm for a
//! rule that stops firing, or one of these rules starting to fire on text
//! that should stay clean, the same contract the fixture marker states.

use osf::config::WritingConfig;
use osf::lints::{check_expectation, load_known_names, parse_expectation, Context, KnownNames};
use std::fs;
use std::path::{Path, PathBuf};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/writing")
}

fn fixture_path(name: &str) -> PathBuf {
    fixtures_dir().join(name)
}

fn known() -> KnownNames {
    load_known_names(&[], None).expect("built-in names load")
}

/// Every one of these fixtures is a short, self-contained document, so the
/// `document` context is the natural one to check it against: the same
/// context `osf lint writing` resolves to for a plain file argument.
fn rules_of(text: &str) -> Vec<&'static str> {
    let mut rules: Vec<&'static str> = osf::lints::writing::lint_writing(
        text,
        &known(),
        &WritingConfig::default(),
        Context::Document,
        false,
        false,
    )
    .into_iter()
    .map(|f| f.rule)
    .collect();
    rules.sort_unstable();
    rules.dedup();
    rules
}

fn read_fixture(name: &str) -> String {
    fs::read_to_string(fixture_path(name)).unwrap_or_else(|e| panic!("{name} reads: {e}"))
}

/// The positive fixture for `rule` must produce it; the negative one, a
/// text that comes close to the same shape, must not.
fn assert_rule_pair(rule: &str) {
    let positive = read_fixture(&format!("{rule}-positive.md"));
    let found = rules_of(&positive);
    assert!(
        found.contains(&rule),
        "{rule}-positive.md did not produce '{rule}': got {found:?}"
    );
    let negative = read_fixture(&format!("{rule}-negative.md"));
    let found = rules_of(&negative);
    assert!(
        !found.contains(&rule),
        "{rule}-negative.md wrongly produced '{rule}': got {found:?}"
    );
}

#[test]
fn contrast_tail() {
    assert_rule_pair("contrast-tail");
}

#[test]
fn contrast_not_just() {
    assert_rule_pair("contrast-not-just");
}

#[test]
fn universal_pronoun() {
    assert_rule_pair("universal-pronoun");
}

#[test]
fn contrast_pair() {
    assert_rule_pair("contrast-pair");
}

#[test]
fn negative_listing() {
    assert_rule_pair("negative-listing");
}

#[test]
fn aphorism() {
    assert_rule_pair("aphorism");
}

#[test]
fn throat_clearing() {
    assert_rule_pair("throat-clearing");
}

#[test]
fn faux_insight() {
    assert_rule_pair("faux-insight");
}

#[test]
fn puffery() {
    assert_rule_pair("puffery");
}

#[test]
fn weasel_attribution() {
    assert_rule_pair("weasel-attribution");
}

#[test]
fn rhetorical_setup() {
    assert_rule_pair("rhetorical-setup");
}

#[test]
fn colon_reveal() {
    assert_rule_pair("colon-reveal");
}

#[test]
fn short_kicker() {
    assert_rule_pair("short-kicker");
}

#[test]
fn ing_tail() {
    assert_rule_pair("ing-tail");
}

#[test]
fn recap_ending() {
    assert_rule_pair("recap-ending");
}

/// Runs every fixture under `tests/fixtures/writing` and asserts it
/// produces exactly the rule ids its own `osf-expect` marker names, no
/// more and no fewer. A rule that silently stops firing, or one that
/// starts firing on a fixture that declared it clean, fails here.
#[test]
fn every_fixture_matches_its_declaration() {
    let cfg = WritingConfig::default();
    let known = known();
    let mut checked = 0usize;
    for entry in fs::read_dir(fixtures_dir()).expect("fixtures/writing reads") {
        let entry = entry.expect("directory entry reads");
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let text =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} reads: {e}", path.display()));
        let expected = parse_expectation(&text)
            .unwrap_or_else(|| panic!("{} carries no osf-expect marker", path.display()));
        let findings =
            osf::lints::writing::lint_writing(&text, &known, &cfg, Context::Document, false, false);
        let mismatch = check_expectation(&expected, &findings, osf::lints::RETIRED_RULE_IDS);
        assert!(
            mismatch.is_empty(),
            "{}: missing {:?}, unexpected {:?}",
            path.display(),
            mismatch.missing,
            mismatch.unexpected
        );
        checked += 1;
    }
    assert_eq!(
        checked, 30,
        "expected 30 fixture files (15 rules x positive/negative)"
    );
}
