//! The lints this tool carries, one module each.
//!
//! [`writing`] checks prose written for people. [`skill`] checks a skill
//! folder. [`agnix`] runs the checks this project does not write itself.
//!
//! What sits here rather than in one of them is what more than one of them
//! needs: the shared name list, the rule metadata lookup across every lint,
//! and the two path rules that decide where a fixture declaration counts.

pub mod agnix;
pub mod skill;
pub mod writing;

pub use meta::{RuleMeta, RULE_META};
pub use osf_lint_core::{
    check_expectation, check_skill, parse_expectation, parse_skill_expectation, Context, Evidence,
    Finding, KnownNames, Level, Mismatch, Remediation,
};
pub use skill::SKILL_RULE_META;
use writing::{meta, names};

/// A rule's doc text and metadata, whether it is a writing rule or a skill rule.
#[must_use]
pub fn rule_meta(id: &str) -> Option<&'static RuleMeta> {
    meta::rule_meta(id).or_else(|| skill::rule_meta(id))
}

use std::path::Path;

/// # Errors
/// Returns an error if `path` is given and cannot be read.
pub fn load_known_names(extra: &[String], path: Option<&Path>) -> Result<KnownNames, String> {
    let built_in: Vec<&str> = names::BUILT_IN
        .iter()
        .copied()
        .chain(extra.iter().map(String::as_str))
        .collect();
    osf_lint_core::load_known_names(&built_in, path)
}

/// Whether `name` sits under a `tests/fixtures` directory. Hard-coded, not
/// a configured setting: an `osf-expect` marker only takes effect here, so
/// a repository cannot use it to launder a real finding in an ordinary
/// file. The tool does not honour the marker anywhere else.
#[must_use]
pub fn is_fixture_path(name: &str) -> bool {
    let normalised = name.replace('\\', "/");
    normalised
        .split('/')
        .collect::<Vec<_>>()
        .windows(2)
        .any(|pair| pair == ["tests", "fixtures"])
}

/// Whether `id` names a scan rule: a leaked name, a session link, a local
/// path. Hard-coded, not a configured setting: no `osf-expect` marker may
/// declare one of these expected, so nothing inside the repository can
/// silence a scan finding by naming it in a fixture.
#[must_use]
pub fn is_scan_rule(id: &str) -> bool {
    id.starts_with("scan-")
}
