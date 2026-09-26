//! The review lens catalogue: the shipped lenses, the organisation's, and the repository's `.osf/review-lenses/`, one lens per area a reviewer judges.

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
            upsert(&mut lenses, &mut sources, lens, forward_slash(&path));
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
}
