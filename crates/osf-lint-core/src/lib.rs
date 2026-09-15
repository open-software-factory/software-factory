//! Reusable engine for Open Software Factory linters: findings, known
//! names, text segmentation, rule and analyser traits, and output
//! rendering. A caller supplies its own rules; this crate carries none.

mod engine;
mod finding;
mod names;
mod rule;
pub mod segment;

pub use engine::{run_analysers, run_rules, sort_findings};
pub use finding::{Evidence, Finding, Level};
pub use names::{load_known_names, KnownNames};
pub use rule::{Analyser, FnRule, Rule, Scope, Tier};
