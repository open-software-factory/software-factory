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
use std::collections::BTreeSet;
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
}
