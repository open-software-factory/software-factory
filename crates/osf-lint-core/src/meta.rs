//! Rule metadata a caller attaches to its own rules: why the rule exists,
//! and whether a reader can work out the meaning of the flagged text
//! without asking. Neither field sets a level; [`crate::resolve`] does that.

use std::fmt;

/// Why we believe a rule. Documentation and honesty, not a level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// An external written standard requires it. The citation names the clause.
    Spec,
    /// A published measurement supports it. The citation names the study.
    Evidence,
    /// The artifact is objectively broken; no opinion is involved.
    Correctness,
    /// A real risk in a file an agent reads or executes.
    Security,
    /// Our own taste. The citation says so and claims nothing more.
    House,
}

impl fmt::Display for Class {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Class::Spec => "spec",
            Class::Evidence => "evidence",
            Class::Correctness => "correctness",
            Class::Security => "security",
            Class::House => "house",
        })
    }
}

/// Whether a reader can work out the meaning of the flagged text without asking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// No. The text is unusable as written.
    Comprehension,
    /// Yes. The text is understandable but worse.
    Style,
}

impl fmt::Display for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Group::Comprehension => "comprehension",
            Group::Style => "style",
        })
    }
}

/// Where the text lives. The caller supplies it; it is not guessed from
/// the text itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    /// A reply in an agent's own conversation. Disposable.
    Transcript,
    /// A commit message, a pull request, or an issue. Costs a history rewrite to fix later.
    Commit,
    /// A document such as a plan or a report. Still fixable in review.
    Document,
    /// A skill file, read by a machine. Held to the strictest standard.
    Skill,
}

/// A per-rule override of the level the context matrix would otherwise
/// choose. Remediation still comes from the matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exception {
    FixedLevel(crate::Level),
}

/// Resolves a finding's level and remediation from its rule's class and
/// group, the context the text lives in, and any per-rule exception.
///
/// A rule of class [`Class::Security`] or [`Class::Correctness`] is always
/// an error asking for a rewrite, in every context: a real risk or a
/// broken artifact is not advisory anywhere. Otherwise the group and the
/// context set the level and the remediation, and `exception` may still
/// override the level.
#[must_use]
pub fn resolve(
    class: Class,
    group: Group,
    context: Context,
    exception: Option<Exception>,
) -> (crate::Level, crate::Remediation) {
    use crate::{Level, Remediation};
    if matches!(class, Class::Security | Class::Correctness) {
        return (Level::Error, Remediation::Rewrite);
    }
    let (level, remediation) = match (group, context) {
        (Group::Comprehension, Context::Transcript) => (Level::Error, Remediation::Clarify),
        (Group::Comprehension, Context::Commit | Context::Document | Context::Skill) => {
            (Level::Error, Remediation::Rewrite)
        }
        (Group::Style, Context::Transcript) => (Level::Warning, Remediation::Advise),
        (Group::Style, Context::Document) => (Level::Warning, Remediation::Rewrite),
        (Group::Style, Context::Commit | Context::Skill) => (Level::Error, Remediation::Rewrite),
    };
    match exception {
        Some(Exception::FixedLevel(l)) => (l, remediation),
        None => (level, remediation),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Level, Remediation};

    #[test]
    fn a_comprehension_rule_in_a_transcript_blocks_for_a_named_correction_only() {
        assert_eq!(
            resolve(
                Class::House,
                Group::Comprehension,
                Context::Transcript,
                None
            ),
            (Level::Error, Remediation::Clarify)
        );
    }

    #[test]
    fn a_comprehension_rule_elsewhere_asks_for_a_rewrite() {
        for context in [Context::Commit, Context::Document, Context::Skill] {
            assert_eq!(
                resolve(Class::House, Group::Comprehension, context, None),
                (Level::Error, Remediation::Rewrite)
            );
        }
    }

    #[test]
    fn a_style_rule_in_a_transcript_does_not_block() {
        assert_eq!(
            resolve(Class::House, Group::Style, Context::Transcript, None),
            (Level::Warning, Remediation::Advise)
        );
    }

    #[test]
    fn a_style_rule_in_a_document_warns_but_still_asks_for_a_rewrite() {
        assert_eq!(
            resolve(Class::House, Group::Style, Context::Document, None),
            (Level::Warning, Remediation::Rewrite)
        );
    }

    #[test]
    fn a_style_rule_in_a_commit_or_skill_file_is_an_error() {
        for context in [Context::Commit, Context::Skill] {
            assert_eq!(
                resolve(Class::House, Group::Style, context, None),
                (Level::Error, Remediation::Rewrite)
            );
        }
    }

    #[test]
    fn a_security_or_correctness_rule_is_always_an_error_asking_for_a_rewrite() {
        for class in [Class::Security, Class::Correctness] {
            for context in [
                Context::Transcript,
                Context::Commit,
                Context::Document,
                Context::Skill,
            ] {
                assert_eq!(
                    resolve(class, Group::Style, context, None),
                    (Level::Error, Remediation::Rewrite)
                );
            }
        }
    }

    #[test]
    fn a_fixed_level_exception_overrides_the_level_but_not_the_remediation() {
        let exception = Some(Exception::FixedLevel(Level::Warning));
        assert_eq!(
            resolve(Class::House, Group::Style, Context::Commit, exception),
            (Level::Warning, Remediation::Rewrite)
        );
    }
}
