//! A fixture's declared expectation: the exact rule ids one file must
//! produce, read from an `<!-- osf-expect ... -->` marker in the raw
//! source. A rule that silently stops firing, or a fixture that starts
//! firing a rule it never declared, is a failure, not a quiet pass.
//!
//! A skill folder uses its own marker name, `osf-expect-skill`, read by
//! [`parse_skill_expectation`]. The two names are deliberately different
//! and deliberately never overlap in what they match: a checker must read
//! only its own markers, never the other checker's, so a skill declaration
//! sitting inside a `SKILL.md` file is invisible to the writing lint's own
//! `osf-expect` marker, and the other way round.
//!
//! This module only parses a marker and compares it against findings. It
//! does not decide which files are allowed to declare one; a caller with
//! its own path convention makes that call.

use crate::finding::Finding;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// Matches an `osf-expect` marker and, in the first group, an optional
/// suffix such as `-skill` that names a different checker's marker
/// entirely. The `regex` crate this project uses has no look-around, so the
/// one plain-vs-skill decision that would otherwise want a negative
/// lookahead is made afterwards, in Rust, by reading that group: absent for
/// [`parse_expectation`]'s own marker, `-skill` for
/// [`parse_skill_expectation`]'s.
fn marker_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?s)<!--\s*osf-expect(-[a-z]+)?\b(.*?)-->")
            .expect("expectation marker pattern compiles")
    })
}

/// The rule ids one `osf-expect` marker names, one per line, or `None` if
/// `source` carries no such marker. A marker carrying a suffix, such as
/// `osf-expect-skill`, belongs to a different checker and is never read
/// here; that one belongs to [`parse_skill_expectation`] alone.
#[must_use]
pub fn parse_expectation(source: &str) -> Option<BTreeSet<String>> {
    let captures = marker_regex().captures(source)?;
    if captures.get(1).is_some() {
        return None;
    }
    let body = captures.get(2)?.as_str();
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

/// The per-file rule ids one `osf-expect-skill` marker names in a folder
/// that holds more than one lintable file, such as a skill folder's
/// `SKILL.md` plus its `scripts/`. This marker has its own name, distinct
/// from [`parse_expectation`]'s `osf-expect`, so the writing lint never
/// reads a skill declaration as its own and a skill folder never reads a
/// plain `osf-expect` marker as a skill declaration: each checker sees only
/// the marker it owns. Line syntax is otherwise the same: a line may carry
/// a file name ahead of the rule id. `SKILL.md` is implied when a line
/// names only a rule id, and a line with a file name and a rule id,
/// separated by whitespace, names that file instead, such as
/// `scripts/install.sh skill-script-unpinned`. Neither a file name nor a
/// rule id may contain whitespace, so the first run of whitespace on a line
/// always separates the two.
#[must_use]
pub fn parse_skill_expectation(source: &str) -> Option<BTreeMap<String, BTreeSet<String>>> {
    let captures = marker_regex().captures(source)?;
    if captures.get(1).map(|m| m.as_str()) != Some("-skill") {
        return None;
    }
    let body = captures.get(2)?.as_str();
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
        let text = "<!-- osf-expect-skill\nskill-first-person\n-->\n";
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
        let text = "<!-- osf-expect-skill\nundefined-name-at-start\nscripts/install.sh skill-script-unpinned\n-->\n";
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
        let text = "<!-- osf-expect-skill\nagnix:AS-001\n-->\n";
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
        let text = "<!-- osf-expect-skill\n-->\n";
        let expected = parse_skill_expectation(text).expect("marker parses");
        assert!(expected.is_empty());
    }

    /// The two marker names must never overlap: a checker reads only its
    /// own marker and treats the other's as if it were not there at all.
    #[test]
    fn a_writing_marker_and_a_skill_marker_are_invisible_to_each_other() {
        let writing_only = "<!-- osf-expect\narrow\n-->\n";
        assert!(parse_skill_expectation(writing_only).is_none());

        let skill_only = "<!-- osf-expect-skill\nskill-first-person\n-->\n";
        assert!(parse_expectation(skill_only).is_none());
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
