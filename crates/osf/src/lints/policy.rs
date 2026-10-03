//! Audited enforcement defaults, separate from raw detector evaluation.
//! Disabled detectors and fixture contracts remain available; proposed
//! detectors are tracked in the backlog, not represented as checks here.

use osf_lint_core::LevelSetting;
use std::collections::BTreeMap;

pub const DISABLED_WRITING: &[&str] = &[
    "undefined-name",
    "undefined-name-at-start",
    "arrow",
    "filler",
    "numbers-in-prose",
    "bold-sentence",
    "contrast-tail",
    "aphorism",
    "rhetorical-setup",
    "universal-pronoun",
    "colon-reveal",
    "short-kicker",
];

pub const DISABLED_SKILL: &[&str] = &[
    "skill-description-no-trigger",
    "skill-first-person",
    "skill-first-section-is-overview",
    "skill-descriptive-over-imperative",
    "skill-no-done-condition",
];

#[must_use]
pub fn off_levels(ids: &[&str]) -> BTreeMap<String, LevelSetting> {
    ids.iter()
        .map(|id| ((*id).to_string(), LevelSetting::Off))
        .collect()
}

#[must_use]
pub fn disabled_by_default(id: &str) -> bool {
    DISABLED_WRITING.contains(&id) || DISABLED_SKILL.contains(&id)
}

/// Skill bodies produce writing findings as well as skill findings.
/// Preserve the existing skill override precedence for an explicit entry,
/// but include writing policy when no skill-level override was supplied.
#[must_use]
pub fn skill_levels(
    writing: &BTreeMap<String, LevelSetting>,
    skill: &BTreeMap<String, LevelSetting>,
) -> BTreeMap<String, LevelSetting> {
    let mut levels = writing.clone();
    levels.extend(skill.iter().map(|(id, level)| (id.clone(), *level)));
    levels
}

/// Reports actual resolved Off settings, not only compiled defaults.
/// This is enforcement coverage; raw fixture evaluation is separate.
#[must_use]
pub fn coverage(levels: &BTreeMap<String, LevelSetting>) -> String {
    let ids: Vec<&str> = levels
        .iter()
        .filter_map(|(id, level)| (*level == LevelSetting::Off).then_some(id.as_str()))
        .collect();
    let disabled = if ids.is_empty() {
        "none".to_string()
    } else {
        ids.join(", ")
    };
    format!("disabled from enforcement: {disabled}; raw fixture evaluation is separate")
}
