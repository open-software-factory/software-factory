//! A fixture's declared expectation: the exact rule ids one file must
//! produce, read from an `<!-- osf-expect ... -->` marker in the raw
//! source. A rule that silently stops firing, or a fixture that starts
//! firing a rule it never declared, is a failure, not a quiet pass.
//!
//! This module only parses the marker and compares it against findings.
//! It does not decide which files are allowed to declare one; a caller
//! with its own path convention makes that call.

use crate::finding::Finding;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

fn expect_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?s)<!--\s*osf-expect\b(.*?)-->").expect("expectation marker pattern compiles")
    })
}

/// The rule ids one `osf-expect` marker names, one per line, or `None` if
/// `source` carries no such marker.
#[must_use]
pub fn parse_expectation(source: &str) -> Option<BTreeSet<String>> {
    let captures = expect_regex().captures(source)?;
    let body = captures.get(1)?.as_str();
    Some(
        body.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// What a declared file's actual findings did not match: rule ids it
/// promised but did not produce, and rule ids it produced but did not
/// promise. Both are sorted, since they come from a `BTreeSet`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Mismatch {
    pub missing: Vec<String>,
    pub unexpected: Vec<String>,
}

impl Mismatch {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.missing.is_empty() && self.unexpected.is_empty()
    }
}

/// Compares `expected` against the rule ids actually present in
/// `findings`, ignoring suppression: the declaration is about whether a
/// rule fires, not about whether its finding stays visible.
#[must_use]
pub fn check(expected: &BTreeSet<String>, findings: &[Finding]) -> Mismatch {
    let actual: BTreeSet<&str> = findings.iter().map(|f| f.rule).collect();
    let missing = expected
        .iter()
        .filter(|id| !actual.contains(id.as_str()))
        .cloned()
        .collect();
    let unexpected = actual
        .iter()
        .filter(|id| !expected.contains(**id))
        .map(|id| (*id).to_string())
        .collect();
    Mismatch {
        missing,
        unexpected,
    }
}

/// The per-file rule ids one `osf-expect` marker names in a folder that
/// holds more than one lintable file, such as a skill folder's `SKILL.md`
/// plus its `scripts/`. Reads the same marker syntax [`parse_expectation`]
/// reads, but a line may carry a file name ahead of the rule id: `SKILL.md`
/// is implied when a line names only a rule id, and a line with a file name
/// and a rule id, separated by whitespace, names that file instead, such as
/// `scripts/install.sh skill-script-unpinned`. Neither a file name nor a
/// rule id may contain whitespace, so the first run of whitespace on a line
/// always separates the two.
#[must_use]
pub fn parse_skill_expectation(source: &str) -> Option<BTreeMap<String, BTreeSet<String>>> {
    let captures = expect_regex().captures(source)?;
    let body = captures.get(1)?.as_str();
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for line in body.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let (file, rule) = match line.split_once(char::is_whitespace) {
            Some((file, rule)) => (file.trim(), rule.trim()),
            None => ("SKILL.md", line),
        };
        out.entry(file.to_string())
            .or_default()
            .insert(rule.to_string());
    }
    Some(out)
}

/// Compares a folder's declared, per-file rule ids against `(file, rule)`
/// pairs actually produced. A missing or unexpected entry renders as
/// `"<file>: <rule>"`, so the file and the rule id both show in the
/// mismatch, the same as [`check`] does for rule ids alone.
#[must_use]
pub fn check_skill<'a>(
    expected: &BTreeMap<String, BTreeSet<String>>,
    actual: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Mismatch {
    let expected_pairs: BTreeSet<(&str, &str)> = expected
        .iter()
        .flat_map(|(file, rules)| rules.iter().map(move |rule| (file.as_str(), rule.as_str())))
        .collect();
    let actual_pairs: BTreeSet<(&str, &str)> = actual.into_iter().collect();
    let missing = expected_pairs
        .difference(&actual_pairs)
        .map(|(file, rule)| format!("{file}: {rule}"))
        .collect();
    let unexpected = actual_pairs
        .difference(&expected_pairs)
        .map(|(file, rule)| format!("{file}: {rule}"))
        .collect();
    Mismatch {
        missing,
        unexpected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::Level;

    fn finding(rule: &'static str) -> Finding {
        Finding::new(rule, Level::Warning, 1, "m".to_string(), "x".to_string())
    }

    #[test]
    fn a_file_with_no_marker_parses_to_none() {
        assert!(parse_expectation("Plain text.\n").is_none());
    }

    #[test]
    fn a_marker_lists_one_id_per_line() {
        let text = "Text.\n<!-- osf-expect\narrow\nsemicolon\n-->\n";
        let expected = parse_expectation(text).expect("marker parses");
        assert_eq!(
            expected,
            BTreeSet::from(["arrow".to_string(), "semicolon".to_string()])
        );
    }

    #[test]
    fn a_matching_declaration_has_no_mismatch() {
        let expected = BTreeSet::from(["arrow".to_string()]);
        let findings = vec![finding("arrow")];
        assert!(check(&expected, &findings).is_empty());
    }

    #[test]
    fn a_missing_declared_rule_is_reported() {
        let expected = BTreeSet::from(["arrow".to_string(), "semicolon".to_string()]);
        let findings = vec![finding("arrow")];
        let mismatch = check(&expected, &findings);
        assert_eq!(mismatch.missing, vec!["semicolon".to_string()]);
        assert!(mismatch.unexpected.is_empty());
    }

    #[test]
    fn an_undeclared_rule_is_reported() {
        let expected = BTreeSet::from(["arrow".to_string()]);
        let findings = vec![finding("arrow"), finding("semicolon")];
        let mismatch = check(&expected, &findings);
        assert!(mismatch.missing.is_empty());
        assert_eq!(mismatch.unexpected, vec!["semicolon".to_string()]);
    }

    #[test]
    fn a_bare_line_in_a_skill_marker_names_skill_md() {
        let text = "<!-- osf-expect\nskill-first-person\n-->\n";
        let expected = parse_skill_expectation(text).expect("marker parses");
        assert_eq!(
            expected,
            BTreeMap::from([(
                "SKILL.md".to_string(),
                BTreeSet::from(["skill-first-person".to_string()])
            )])
        );
    }

    #[test]
    fn a_qualified_line_names_its_own_file() {
        let text =
            "<!-- osf-expect\nundefined-name-at-start\nscripts/install.sh skill-script-unpinned\n-->\n";
        let expected = parse_skill_expectation(text).expect("marker parses");
        assert_eq!(
            expected,
            BTreeMap::from([
                (
                    "SKILL.md".to_string(),
                    BTreeSet::from(["undefined-name-at-start".to_string()])
                ),
                (
                    "scripts/install.sh".to_string(),
                    BTreeSet::from(["skill-script-unpinned".to_string()])
                ),
            ])
        );
    }

    /// A rule id with its own colon, such as a second engine's `agnix:AS-001`,
    /// must not be split into a bogus file name: the marker splits on
    /// whitespace, never on a colon.
    #[test]
    fn a_rule_id_carrying_a_colon_is_not_split() {
        let text = "<!-- osf-expect\nagnix:AS-001\n-->\n";
        let expected = parse_skill_expectation(text).expect("marker parses");
        assert_eq!(
            expected,
            BTreeMap::from([(
                "SKILL.md".to_string(),
                BTreeSet::from(["agnix:AS-001".to_string()])
            )])
        );
    }

    #[test]
    fn an_empty_skill_marker_declares_no_findings_at_all() {
        let text = "<!-- osf-expect\n-->\n";
        let expected = parse_skill_expectation(text).expect("marker parses");
        assert!(expected.is_empty());
    }

    fn skill_expect(pairs: &[(&str, &str)]) -> BTreeMap<String, BTreeSet<String>> {
        let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (file, rule) in pairs {
            out.entry((*file).to_string())
                .or_default()
                .insert((*rule).to_string());
        }
        out
    }

    #[test]
    fn a_matching_skill_declaration_has_no_mismatch() {
        let expected = skill_expect(&[("SKILL.md", "skill-first-person")]);
        let actual = [("SKILL.md", "skill-first-person")];
        assert!(check_skill(&expected, actual).is_empty());
    }

    #[test]
    fn a_missing_declared_skill_rule_names_its_file() {
        let expected = skill_expect(&[
            ("SKILL.md", "skill-first-person"),
            ("scripts/install.sh", "skill-script-unpinned"),
        ]);
        let actual = [("SKILL.md", "skill-first-person")];
        let mismatch = check_skill(&expected, actual);
        assert_eq!(
            mismatch.missing,
            vec!["scripts/install.sh: skill-script-unpinned".to_string()]
        );
        assert!(mismatch.unexpected.is_empty());
    }

    #[test]
    fn an_undeclared_skill_rule_names_its_file() {
        let expected = skill_expect(&[("SKILL.md", "skill-first-person")]);
        let actual = [
            ("SKILL.md", "skill-first-person"),
            ("scripts/install.sh", "skill-script-unpinned"),
        ];
        let mismatch = check_skill(&expected, actual);
        assert!(mismatch.missing.is_empty());
        assert_eq!(
            mismatch.unexpected,
            vec!["scripts/install.sh: skill-script-unpinned".to_string()]
        );
    }
}
