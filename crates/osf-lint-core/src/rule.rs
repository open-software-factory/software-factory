//! A rule set is data a caller supplies. The engine only orchestrates it,
//! so a check can be a cheap regular expression today and a slow local
//! model later, without any change to this crate.

use crate::segment::TextUnit;
use crate::Finding;

/// How much text a check needs to see at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Sentence,
    Paragraph,
    Document,
}

/// How expensive a check is. The engine can run the fast tier alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Fast,
    Slow,
}

/// A check over one unit of text: a sentence, a paragraph, or a document.
/// `C` is whatever resolved configuration the caller's rules need to read;
/// a caller with no such need leaves it at the default, `()`.
pub trait Rule<C = ()> {
    fn id(&self) -> &'static str;
    fn scope(&self) -> Scope;
    fn tier(&self) -> Tier {
        Tier::Fast
    }
    fn check(&self, unit: &TextUnit, config: &C) -> Vec<Finding>;
}

/// A plain function, promoted to a [`Rule`]. Keeps a cheap check a one-liner.
pub struct FnRule<C = ()> {
    id: &'static str,
    scope: Scope,
    tier: Tier,
    check: fn(&TextUnit, &C) -> Vec<Finding>,
}

impl<C> FnRule<C> {
    #[must_use]
    pub const fn new(
        id: &'static str,
        scope: Scope,
        check: fn(&TextUnit, &C) -> Vec<Finding>,
    ) -> Self {
        FnRule {
            id,
            scope,
            tier: Tier::Fast,
            check,
        }
    }

    #[must_use]
    pub const fn sentence(id: &'static str, check: fn(&TextUnit, &C) -> Vec<Finding>) -> Self {
        Self::new(id, Scope::Sentence, check)
    }

    #[must_use]
    pub const fn paragraph(id: &'static str, check: fn(&TextUnit, &C) -> Vec<Finding>) -> Self {
        Self::new(id, Scope::Paragraph, check)
    }

    #[must_use]
    pub const fn document(id: &'static str, check: fn(&TextUnit, &C) -> Vec<Finding>) -> Self {
        Self::new(id, Scope::Document, check)
    }
}

impl<C> Rule<C> for FnRule<C> {
    fn id(&self) -> &'static str {
        self.id
    }

    fn scope(&self) -> Scope {
        self.scope
    }

    fn tier(&self) -> Tier {
        self.tier
    }

    fn check(&self, unit: &TextUnit, config: &C) -> Vec<Finding> {
        (self.check)(unit, config)
    }
}

/// A slow, possibly external check that sees every unit of its scope at
/// once, so it can batch many sentences into one call instead of many.
/// A caller that wants a local model implements this, not the core crate.
pub trait Analyser {
    fn id(&self) -> &'static str;
    fn scope(&self) -> Scope;
    fn analyse(&self, units: &[TextUnit]) -> Vec<Finding>;
}
