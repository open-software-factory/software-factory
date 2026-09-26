//! The review lens catalogue: the shipped lenses, the organisation's, and the repository's `.osf/review-lenses/`, one lens per area a reviewer judges.

use crate::risk;
use std::fs;
use std::path::{Path, PathBuf};

/// One area a reviewer judges on its own.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lens {
    pub name: String,
    pub summary: String,
    pub criteria: Vec<Criterion>,
    pub severity_guide: SeverityGuide,
    pub weight: f64,
    pub runs: Runs,
    #[serde(default)]
    pub trigger: Trigger,
    #[serde(default)]
    pub context: Vec<ContextInput>,
}

/// One question a lens's reviewer scores, from 0 to 1.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    pub id: String,
    pub question: String,
}

/// What separates a blocker, a major and a minor finding for a lens.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeverityGuide {
    pub blocker: String,
    pub major: String,
    pub minor: String,
}

/// When a lens runs: on every change, only when its trigger fires, or its trigger plus every change at the high tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Runs {
    Always,
    Triggered,
    AlwaysAtHighTier,
}

/// The paths and signals that run a triggered lens.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trigger {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub signals: Vec<String>,
}

/// One input a lens declares it needs, assembled before its reviewer runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContextInput {
    Diff,
    WorkItem,
    AcceptanceCriteria,
    DecisionRecords,
    ArchitectureDocs,
    EntryPoints,
}

/// Every loaded review lens, and which source each one came from.
#[derive(Debug)]
pub struct Catalogue {
    pub lenses: Vec<Lens>,
    pub sources: Vec<(String, String)>,
}

/// The shipped lenses, compiled in, each paired with its file name for error messages.
const SHIPPED: &[(&str, &str)] = &[
    (
        "correctness.toml",
        include_str!("../defaults/review-lenses/correctness.toml"),
    ),
    (
        "spec-and-acceptance.toml",
        include_str!("../defaults/review-lenses/spec-and-acceptance.toml"),
    ),
    (
        "test-quality.toml",
        include_str!("../defaults/review-lenses/test-quality.toml"),
    ),
    (
        "security.toml",
        include_str!("../defaults/review-lenses/security.toml"),
    ),
    (
        "privacy-and-data-protection.toml",
        include_str!("../defaults/review-lenses/privacy-and-data-protection.toml"),
    ),
    (
        "data-migration-and-compatibility.toml",
        include_str!("../defaults/review-lenses/data-migration-and-compatibility.toml"),
    ),
    (
        "architecture-adherence.toml",
        include_str!("../defaults/review-lenses/architecture-adherence.toml"),
    ),
    (
        "duplication-and-reuse.toml",
        include_str!("../defaults/review-lenses/duplication-and-reuse.toml"),
    ),
    (
        "performance.toml",
        include_str!("../defaults/review-lenses/performance.toml"),
    ),
    (
        "reliability-and-failure-modes.toml",
        include_str!("../defaults/review-lenses/reliability-and-failure-modes.toml"),
    ),
    (
        "concurrency.toml",
        include_str!("../defaults/review-lenses/concurrency.toml"),
    ),
    (
        "interface-compatibility.toml",
        include_str!("../defaults/review-lenses/interface-compatibility.toml"),
    ),
    (
        "dependency-licence-supply-chain.toml",
        include_str!("../defaults/review-lenses/dependency-licence-supply-chain.toml"),
    ),
    (
        "observability-and-rollback.toml",
        include_str!("../defaults/review-lenses/observability-and-rollback.toml"),
    ),
    (
        "cost.toml",
        include_str!("../defaults/review-lenses/cost.toml"),
    ),
    (
        "accessibility.toml",
        include_str!("../defaults/review-lenses/accessibility.toml"),
    ),
    (
        "internationalisation.toml",
        include_str!("../defaults/review-lenses/internationalisation.toml"),
    ),
    (
        "user-visible-change.toml",
        include_str!("../defaults/review-lenses/user-visible-change.toml"),
    ),
    (
        "design-documents.toml",
        include_str!("../defaults/review-lenses/design-documents.toml"),
    ),
    (
        "repository-patterns.toml",
        include_str!("../defaults/review-lenses/repository-patterns.toml"),
    ),
];

/// Loads the shipped lenses, then the organisation's folder when given, then the repository's `.osf/review-lenses/*.toml`, a later file replacing an earlier lens of the same name.
///
/// # Errors
/// Names the file and the reason when a lens file fails to parse or does not match the lens schema.
pub fn load(root: &Path, org_dir: Option<&Path>) -> Result<Catalogue, String> {
    let mut lenses: Vec<Lens> = Vec::new();
    let mut sources: Vec<(String, String)> = Vec::new();

    for (file_name, text) in SHIPPED {
        let lens: Lens = toml::from_str(text).map_err(|e| format!("{file_name}: {e}"))?;
        upsert(&mut lenses, &mut sources, lens, "shipped".to_string());
    }

    if let Some(dir) = org_dir {
        for (path, lens) in read_toml_dir(dir)? {
            let relative = path.strip_prefix(dir).unwrap_or(&path);
            upsert(&mut lenses, &mut sources, lens, forward_slash(relative));
        }
    }

    let repo_dir = root.join(".osf/review-lenses");
    for (path, lens) in read_toml_dir(&repo_dir)? {
        let relative = path.strip_prefix(root).unwrap_or(&path);
        upsert(&mut lenses, &mut sources, lens, forward_slash(relative));
    }

    Ok(Catalogue { lenses, sources })
}

/// Every `.toml` file directly under `dir`, in name order, each parsed as a `Lens`.
fn read_toml_dir(dir: &Path) -> Result<Vec<(PathBuf, Lens)>, String> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        paths.push(entry.path());
    }
    paths.retain(|p| p.extension().and_then(|ext| ext.to_str()) == Some("toml"));
    paths.sort();

    let mut loaded = Vec::with_capacity(paths.len());
    for path in paths {
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let lens: Lens = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        loaded.push((path, lens));
    }
    Ok(loaded)
}

/// Replaces the lens named `lens.name` in place, keeping catalogue order, or appends it as new.
fn upsert(lenses: &mut Vec<Lens>, sources: &mut Vec<(String, String)>, lens: Lens, source: String) {
    let name = lens.name.clone();
    match lenses.iter_mut().find(|existing| existing.name == name) {
        Some(existing) => *existing = lens,
        None => lenses.push(lens),
    }
    match sources.iter_mut().find(|(n, _)| *n == name) {
        Some(existing) => existing.1 = source,
        None => sources.push((name, source)),
    }
}

/// `path`, rendered with forward slashes whatever the platform's own separator is.
fn forward_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// One lens selected for a change, and the rule that selected it.
pub struct Selected<'a> {
    pub lens: &'a Lens,
    pub reason: String,
}

/// How far a lens's context reaches beyond the diff, set by the change's tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    Diff,
    DiffAndCallers,
    Module,
}

/// The context depth a tier earns: a low-tier change reviews the diff
/// alone, a normal one adds the diff's callers, a high one adds every file
/// in each changed file's own module.
#[must_use]
pub fn depth(tier: risk::Tier) -> Depth {
    match tier {
        risk::Tier::Low => Depth::Diff,
        risk::Tier::Normal => Depth::DiffAndCallers,
        risk::Tier::High => Depth::Module,
    }
}

/// The lenses a change selects, in catalogue order, each paired with the
/// rule that selected it.
///
/// A lens is selected when it runs on every change; when it runs on every
/// change at the high tier and the tier is high; when a changed path
/// matches one of its trigger globs; or when a signal the change earned
/// matches one of its trigger signals.
#[must_use]
pub fn select<'a>(
    catalogue: &'a Catalogue,
    changed: &[String],
    signals: &[String],
    tier: risk::Tier,
) -> Vec<Selected<'a>> {
    catalogue
        .lenses
        .iter()
        .filter_map(|lens| {
            selection_reason(lens, changed, signals, tier).map(|reason| Selected { lens, reason })
        })
        .collect()
}

/// Why `lens` is selected for this change, or `None` when nothing about it selects the lens.
fn selection_reason(
    lens: &Lens,
    changed: &[String],
    signals: &[String],
    tier: risk::Tier,
) -> Option<String> {
    if lens.runs == Runs::Always {
        return Some("runs on every change".to_string());
    }
    if lens.runs == Runs::AlwaysAtHighTier && tier == risk::Tier::High {
        return Some("runs on every change at the high tier".to_string());
    }
    if let Some(pattern) = matching_path(&lens.trigger.paths, changed) {
        return Some(format!("path trigger: {pattern}"));
    }
    if let Some(signal) = matching_signal(&lens.trigger.signals, signals) {
        return Some(format!("signal trigger: {signal}"));
    }
    None
}

/// The first trigger glob among `patterns` that matches a path in `changed`, if any.
fn matching_path<'a>(patterns: &'a [String], changed: &[String]) -> Option<&'a str> {
    patterns.iter().find_map(|pattern| {
        let matcher = globset::Glob::new(pattern).ok()?.compile_matcher();
        changed
            .iter()
            .any(|path| matcher.is_match(path.replace('\\', "/")))
            .then_some(pattern.as_str())
    })
}

/// The first trigger signal among `trigger_signals` that the change also earned, if any.
fn matching_signal<'a>(trigger_signals: &'a [String], earned: &[String]) -> Option<&'a str> {
    trigger_signals
        .iter()
        .find(|signal| earned.contains(*signal))
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    fn temp_root(name: &str) -> TempDir {
        let dir = TempDir::new(&format!("osf-lenses-{name}"));
        std::fs::create_dir_all(dir.join(".osf/review-lenses")).expect("dirs");
        dir
    }

    #[test]
    fn the_shipped_catalogue_has_twenty_lenses_and_six_must_run() {
        let root = temp_root("shipped");
        let c = load(&root, None).expect("loads");
        assert_eq!(c.lenses.len(), 20);
        assert_eq!(
            c.lenses.iter().filter(|l| l.runs == Runs::Always).count(),
            6
        );
    }

    #[test]
    fn an_adopter_lens_with_the_same_name_replaces_the_shipped_one() {
        let root = temp_root("replace");
        let shipped = include_str!("../defaults/review-lenses/security.toml");
        let replaced = shipped.replace("weight = 1.0", "weight = 3.0");
        std::fs::write(root.join(".osf/review-lenses/security.toml"), replaced).expect("write");
        let c = load(&root, None).expect("loads");
        let sec = c
            .lenses
            .iter()
            .find(|l| l.name == "security")
            .expect("security");
        assert!((sec.weight - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_misspelt_field_stops_the_load_and_names_the_file_and_field() {
        let root = temp_root("typo");
        std::fs::write(
            root.join(".osf/review-lenses/money.toml"),
            "name = \"money\"\nsumary = \"x\"\n",
        )
        .expect("write");
        let err = load(&root, None).expect_err("must fail");
        assert!(err.contains("money.toml"), "{err}");
        assert!(err.contains("sumary"), "{err}");
    }

    #[test]
    fn the_examples_are_shipped_but_not_loaded() {
        let root = temp_root("examples");
        let c = load(&root, None).expect("loads");
        assert!(c.lenses.iter().all(|l| l.name != "money"));
    }

    #[test]
    fn every_triggered_lens_has_at_least_one_trigger_path_or_signal() {
        let root = temp_root("trigger-coverage");
        let c = load(&root, None).expect("loads");
        for lens in &c.lenses {
            if matches!(lens.runs, Runs::Triggered | Runs::AlwaysAtHighTier) {
                assert!(
                    !lens.trigger.paths.is_empty() || !lens.trigger.signals.is_empty(),
                    "{} has no trigger path or signal",
                    lens.name
                );
            }
        }
    }

    #[test]
    fn accessibility_and_user_visible_change_do_not_share_a_signal() {
        let root = temp_root("no-shared-signal");
        let c = load(&root, None).expect("loads");
        let accessibility = c
            .lenses
            .iter()
            .find(|l| l.name == "accessibility")
            .expect("accessibility");
        let user_visible = c
            .lenses
            .iter()
            .find(|l| l.name == "user-visible-change")
            .expect("user-visible-change");
        for signal in &accessibility.trigger.signals {
            assert!(
                !user_visible.trigger.signals.contains(signal),
                "both lenses trigger on {signal}"
            );
        }
    }

    #[test]
    fn sources_are_forward_slash_paths_relative_to_their_own_root() {
        let root = temp_root("sources");
        std::fs::write(
            root.join(".osf/review-lenses/money.toml"),
            include_str!("../defaults/review-lenses/examples/money.toml"),
        )
        .expect("write");
        let org = TempDir::new("osf-lenses-sources-org");
        std::fs::write(
            org.join("health-data.toml"),
            include_str!("../defaults/review-lenses/examples/health-data.toml"),
        )
        .expect("write");
        let c = load(&root, Some(&org)).expect("loads");

        let shipped_source = c
            .sources
            .iter()
            .find(|(name, _)| name == "correctness")
            .map(|(_, source)| source.as_str());
        assert_eq!(shipped_source, Some("shipped"));

        let repo_source = c
            .sources
            .iter()
            .find(|(name, _)| name == "money")
            .map(|(_, source)| source.as_str());
        assert_eq!(repo_source, Some(".osf/review-lenses/money.toml"));

        let org_source = c
            .sources
            .iter()
            .find(|(name, _)| name == "health-data")
            .map(|(_, source)| source.as_str());
        assert_eq!(org_source, Some("health-data.toml"));
    }

    #[test]
    fn an_untriggered_change_still_runs_the_six_must_run_lenses() {
        let root = temp_root("select-none");
        let c = load(&root, None).expect("loads");
        let s = select(&c, &["assets/logo.png".to_string()], &[], risk::Tier::Low);
        let names: Vec<&str> = s.iter().map(|x| x.lens.name.as_str()).collect();
        assert_eq!(names.len(), 6, "{names:?}");
        assert!(names.contains(&"privacy-and-data-protection"));
    }

    #[test]
    fn a_low_tier_change_to_a_migration_runs_the_data_migration_lens_by_trigger() {
        let root = temp_root("select-migration");
        let c = load(&root, None).expect("loads");
        let s = select(
            &c,
            &["migrations/0003_add_column.sql".to_string()],
            &["stored-data".to_string()],
            risk::Tier::Low,
        );
        assert!(s
            .iter()
            .any(|x| x.lens.name == "data-migration-and-compatibility"));
    }

    #[test]
    fn the_high_tier_adds_architecture_and_duplication() {
        let root = temp_root("select-high");
        let c = load(&root, None).expect("loads");
        let s = select(&c, &["README.md".to_string()], &[], risk::Tier::High);
        assert!(s.iter().any(|x| x.lens.name == "architecture-adherence"));
        assert!(s.iter().any(|x| x.lens.name == "duplication-and-reuse"));
    }

    #[test]
    fn architecture_and_duplication_do_not_run_at_low_tier_without_a_trigger() {
        let root = temp_root("select-low-no-trigger");
        let c = load(&root, None).expect("loads");
        let s = select(&c, &["README.md".to_string()], &[], risk::Tier::Low);
        assert!(!s.iter().any(|x| x.lens.name == "architecture-adherence"));
        assert!(!s.iter().any(|x| x.lens.name == "duplication-and-reuse"));
    }

    #[test]
    fn architecture_and_duplication_run_at_low_tier_when_triggered() {
        let root = temp_root("select-low-triggered");
        let c = load(&root, None).expect("loads");
        let s = select(
            &c,
            &["crates/osf/src/lib.rs".to_string()],
            &["repeated-logic".to_string()],
            risk::Tier::Low,
        );
        assert!(
            s.iter().any(|x| x.lens.name == "architecture-adherence"),
            "lib.rs should path-trigger architecture-adherence"
        );
        assert!(
            s.iter().any(|x| x.lens.name == "duplication-and-reuse"),
            "the repeated-logic signal should trigger duplication-and-reuse"
        );
    }

    #[test]
    fn depth_follows_the_tier() {
        assert_eq!(depth(risk::Tier::Low), Depth::Diff);
        assert_eq!(depth(risk::Tier::Normal), Depth::DiffAndCallers);
        assert_eq!(depth(risk::Tier::High), Depth::Module);
    }

    #[test]
    fn an_interface_source_file_selects_internationalisation() {
        let root = temp_root("select-i18n");
        let c = load(&root, None).expect("loads");
        let s = select(
            &c,
            &["lib/screens/checkout.dart".to_string()],
            &[],
            risk::Tier::Low,
        );
        assert!(s.iter().any(|x| x.lens.name == "internationalisation"));
    }

    #[test]
    fn accessibility_and_user_visible_change_select_independently() {
        let root = temp_root("select-independent");
        let c = load(&root, None).expect("loads");

        let html_only = select(&c, &["app/widget.html".to_string()], &[], risk::Tier::Low);
        assert!(html_only.iter().any(|x| x.lens.name == "accessibility"));
        assert!(!html_only
            .iter()
            .any(|x| x.lens.name == "user-visible-change"));

        let ui_dir_only = select(&c, &["app/ui/logo.png".to_string()], &[], risk::Tier::Low);
        assert!(ui_dir_only
            .iter()
            .any(|x| x.lens.name == "user-visible-change"));
        assert!(!ui_dir_only.iter().any(|x| x.lens.name == "accessibility"));
    }
}
