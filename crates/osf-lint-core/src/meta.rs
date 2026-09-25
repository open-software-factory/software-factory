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
/// group, the context the text lives in, any per-rule exception, and the
/// finding's own evidence.
///
/// Statistical evidence is a guess, not a fact, so it always resolves to a
/// warning the caller only advises on, in every context, whatever the
/// rule's class, group or exception says. A policy that blocks a reply
/// must never block it on a guess.
///
/// A transcript is already sent by the time anything reads it, so no
/// finding there ever asks for a rewrite. See below.
///
/// Everywhere else, a rule of class [`Class::Security`] or
/// [`Class::Correctness`] is always an error asking for a rewrite: a real
/// risk or a broken artifact is not advisory. Otherwise the group and the
/// context set the level and the remediation, and `exception` may still
/// override the level.
#[must_use]
pub fn resolve(
    class: Class,
    group: Group,
    context: Context,
    exception: Option<Exception>,
    evidence: crate::Evidence,
) -> (crate::Level, crate::Remediation) {
    use crate::{Level, Remediation};

    if matches!(evidence, crate::Evidence::Statistical) {
        return (Level::Warning, Remediation::Advise);
    }

    // A transcript cannot be changed. Whatever checks it runs after the
    // text has been sent, so asking for the text again does not replace
    // it: it adds a second copy underneath the first, and the reader now
    // has the problem twice. The only correction a sent message can take
    // is an addition.
    //
    // So nothing here resolves to a rewrite. A finding that stops a
    // reader understanding the text asks for one short follow-up. Every
    // other finding waits, and reaches the next turn as advice, where it
    // costs nothing and improves the text that has not been written yet.
    if matches!(context, Context::Transcript) {
        if matches!(class, Class::Security | Class::Correctness) {
            return (Level::Error, Remediation::Clarify);
        }
        let (level, remediation) = match group {
            Group::Comprehension => (Level::Error, Remediation::Clarify),
            Group::Style => (Level::Warning, Remediation::Advise),
        };
        return match exception {
            Some(Exception::FixedLevel(l)) => (l, remediation),
            None => (level, remediation),
        };
    }

    if matches!(class, Class::Security | Class::Correctness) {
        return (Level::Error, Remediation::Rewrite);
    }
    // Outside a transcript the text has not been delivered yet, so every
    // finding asks for a rewrite and only the level differs. A style
    // point in a document is a warning; everything else is an error.
    let remediation = Remediation::Rewrite;
    let level = if matches!((group, context), (Group::Style, Context::Document)) {
        Level::Warning
    } else {
        Level::Error
    };
    match exception {
        Some(Exception::FixedLevel(l)) => (l, remediation),
        None => (level, remediation),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Evidence, Level, Remediation};

    const D: Evidence = Evidence::Deterministic;

    /// The one that matters. A transcript is already sent, so asking for
    /// it again adds a second copy and corrects nothing. No combination
    /// of class, group and exception may produce a rewrite there.
    #[test]
    fn nothing_in_a_transcript_ever_asks_for_a_rewrite() {
        let classes = [
            Class::Spec,
            Class::Evidence,
            Class::Correctness,
            Class::Security,
            Class::House,
        ];
        let groups = [Group::Comprehension, Group::Style];
        let exceptions = [
            None,
            Some(Exception::FixedLevel(Level::Error)),
            Some(Exception::FixedLevel(Level::Warning)),
        ];
        for class in classes {
            for group in groups {
                for exception in exceptions {
                    let (_, remediation) = resolve(class, group, Context::Transcript, exception, D);
                    assert_ne!(
                        remediation,
                        Remediation::Rewrite,
                        "{class:?} {group:?} {exception:?} asked a sent message to be rewritten"
                    );
                }
            }
        }
    }

    /// A rule that stops a reader understanding the text still pushes
    /// back in a transcript. It asks for one addition, not a rewrite.
    #[test]
    fn a_broken_reference_in_a_transcript_still_asks_for_a_correction() {
        assert_eq!(
            resolve(
                Class::Correctness,
                Group::Comprehension,
                Context::Transcript,
                None,
                D
            ),
            (Level::Error, Remediation::Clarify)
        );
    }

    /// The same rule in a commit message is different, because nothing is
    /// committed yet and the text can still be replaced.
    #[test]
    fn the_same_rule_in_a_commit_message_still_asks_for_a_rewrite() {
        assert_eq!(
            resolve(
                Class::Correctness,
                Group::Comprehension,
                Context::Commit,
                None,
                D
            ),
            (Level::Error, Remediation::Rewrite)
        );
    }

    #[test]
    fn a_comprehension_rule_in_a_transcript_blocks_for_a_named_correction_only() {
        assert_eq!(
            resolve(
                Class::House,
                Group::Comprehension,
                Context::Transcript,
                None,
                D
            ),
            (Level::Error, Remediation::Clarify)
        );
    }

    #[test]
    fn a_comprehension_rule_elsewhere_asks_for_a_rewrite() {
        for context in [Context::Commit, Context::Document, Context::Skill] {
            assert_eq!(
                resolve(Class::House, Group::Comprehension, context, None, D),
                (Level::Error, Remediation::Rewrite)
            );
        }
    }

    #[test]
    fn a_style_rule_in_a_transcript_does_not_block() {
        assert_eq!(
            resolve(Class::House, Group::Style, Context::Transcript, None, D),
            (Level::Warning, Remediation::Advise)
        );
    }

    #[test]
    fn a_style_rule_in_a_document_warns_but_still_asks_for_a_rewrite() {
        assert_eq!(
            resolve(Class::House, Group::Style, Context::Document, None, D),
            (Level::Warning, Remediation::Rewrite)
        );
    }

    #[test]
    fn a_style_rule_in_a_commit_or_skill_file_is_an_error() {
        for context in [Context::Commit, Context::Skill] {
            assert_eq!(
                resolve(Class::House, Group::Style, context, None, D),
                (Level::Error, Remediation::Rewrite)
            );
        }
    }

    /// A real risk or a broken artifact is an error everywhere. What it
    /// asks for depends on whether the text can still change: a rewrite
    /// where it can, an addition where it cannot.
    #[test]
    fn a_security_or_correctness_rule_is_an_error_in_every_context() {
        for class in [Class::Security, Class::Correctness] {
            for context in [Context::Commit, Context::Document, Context::Skill] {
                assert_eq!(
                    resolve(class, Group::Style, context, None, D),
                    (Level::Error, Remediation::Rewrite)
                );
            }
            assert_eq!(
                resolve(class, Group::Style, Context::Transcript, None, D),
                (Level::Error, Remediation::Clarify)
            );
        }
    }

    #[test]
    fn a_fixed_level_exception_overrides_the_level_but_not_the_remediation() {
        let exception = Some(Exception::FixedLevel(Level::Warning));
        assert_eq!(
            resolve(Class::House, Group::Style, Context::Commit, exception, D),
            (Level::Warning, Remediation::Rewrite)
        );
    }

    /// Statistical evidence never blocks, in any context, class or group.
    #[test]
    fn statistical_evidence_is_always_advisory() {
        let classes = [
            Class::Spec,
            Class::Evidence,
            Class::Correctness,
            Class::Security,
            Class::House,
        ];
        let groups = [Group::Comprehension, Group::Style];
        let contexts = [
            Context::Transcript,
            Context::Commit,
            Context::Document,
            Context::Skill,
        ];
        let exceptions = [None, Some(Exception::FixedLevel(Level::Error))];
        for class in classes {
            for group in groups {
                for context in contexts {
                    for exception in exceptions {
                        assert_eq!(
                            resolve(class, group, context, exception, Evidence::Statistical),
                            (Level::Warning, Remediation::Advise),
                            "{class:?} {group:?} {context:?} {exception:?}"
                        );
                    }
                }
            }
        }
    }
}
