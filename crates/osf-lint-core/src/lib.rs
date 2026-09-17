//! Reusable engine for Open Software Factory linters: findings, known
//! names, text segmentation, rule and analyser traits, and output
//! rendering. A caller supplies its own rules; this crate carries none.

mod config;
mod engine;
mod expectation;
mod finding;
mod human;
mod intern;
mod meta;
mod names;
mod rule;
mod sarif;
pub mod segment;
mod suppress;

pub use config::{
    apply_level_overrides, lookup, read_config_file, resolve_path, to_value, ConfigError, Layer,
    Layered, LevelSetting,
};
pub use engine::{run_analysers, run_rules, sort_findings};
pub use expectation::{
    check as check_expectation, check_skill, parse_expectation, parse_skill_expectation, Mismatch,
};
pub use finding::{Evidence, Finding, Level, Remediation};
pub use human::render_human;
pub use intern::intern;
pub use meta::{resolve, Class, Context, Exception, Group};
pub use names::{load_known_names, KnownNames};
pub use rule::{Analyser, FnRule, Rule, Scope, Tier};
pub use sarif::{to_sarif, ToolInfo};
pub use serde_sarif::sarif::Sarif;
pub use suppress::apply_suppressions;
