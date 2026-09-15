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
