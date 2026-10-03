//! The checks this project does not write itself.
//!
//! `agnix` is a linter for the files coding agents read: skill files, agent
//! instruction files, hook and server configuration. It already checks the
//! skill name format, description and compatibility lengths, required and
//! unknown frontmatter keys, body length, link resolution, absolute paths
//! and reference depth. This project links its engine as a library rather
//! than writing those checks again, and rather than calling a separate
//! program that would have to be installed, found on a path and trusted.
//!
//! Two choices here are deliberate and should not be relaxed.
//!
//! The engine is built from its own compiled defaults, never from a
//! configuration file. `agnix` reads a file named `.agnix.toml` when asked
//! to. This module never asks. A setting that loosens a check must not come
//! from the thing being checked, and a skill folder inside a repository
//! being linted could otherwise carry one.
//!
//! Each file is validated on its own, rather than by handing the engine a
//! directory to walk. The directory walk honours ignore files, which is the
//! same hole by another route: a line in `.gitignore` would quietly remove a
//! skill from the check.

use agnix_core::{DiagnosticLevel, FileType, LintConfig, ValidatorRegistry};
use osf_lint_core::{intern, Evidence, Finding, Level, Remediation};
use std::path::Path;

/// The version of the engine compiled in. `version_matches_the_manifest`
/// keeps this equal to the pinned dependency.
pub const AGNIX_VERSION: &str = "0.54.0";

/// Named on every run so a clean result is never read as "nothing ran".
#[must_use]
pub fn checked_note() -> String {
    format!(
        "the skill name format, description and compatibility lengths, frontmatter keys, body \
         length, link resolution, absolute paths and reference depth are checked by the agnix \
         engine, version {AGNIX_VERSION}, compiled in. Run `agnix explain <rule>` for a rule \
         reported as agnix:<rule>."
    )
}

/// The source recorded on every finding that came from the other engine, so
/// a reader of the output can tell which linter to ask about it.
const SOURCE: &str = "agnix";

/// Rule id for the cases where the engine reported nothing because it could
/// not run, rather than because the file is clean.
const DID_NOT_RUN: &str = "agnix-did-not-run";

/// Run the engine over `content`, already resolved by the caller, and
/// return its diagnostics as findings. `path` only shapes file-type
/// detection and the root-relative labels the engine reports; the engine
/// never reads it from disk. The caller resolves `content` itself —
/// checkpoint-aware for `osf verify` (decision 0003: pre-push and every
/// other checkpoint but the hook read committed content, never whatever
/// happens to sit in the working tree), disk content for the standalone
/// `osf lint skill` command.
///
/// A file type the engine does not recognise produces a finding of its
/// own. Silence here would mean a malformed skill file passed the check by
/// going unrecognised, which is the opposite of what a gate is for.
pub fn lint_text(path: &Path, content: &str, skill_dir: &Path) -> Vec<Finding> {
    let mut config = LintConfig::default();
    config.set_root_dir(skill_dir.to_path_buf());
    if agnix_core::resolve_file_type(path, &config) == FileType::Unknown {
        return vec![did_not_run(
            "the agnix engine does not recognise this file type, so none of its checks ran"
                .to_string(),
        )];
    }
    let registry = ValidatorRegistry::with_defaults();
    agnix_core::validate_content(path, content, &config, &registry)
        .iter()
        .map(to_finding)
        .collect()
}

fn did_not_run(message: String) -> Finding {
    Finding::new(DID_NOT_RUN, Level::Error, 1, message, String::new())
        .from_analyser(SOURCE, Evidence::Deterministic)
}

/// The other engine's three levels map one to one. An informational
/// diagnostic stays informational: reported, never dropped, and never
/// promoted, so a note from the engine cannot block a push under
/// `--strict` the way a warning does.
fn to_finding(diagnostic: &agnix_core::Diagnostic) -> Finding {
    let level = match diagnostic.level {
        DiagnosticLevel::Error => Level::Error,
        DiagnosticLevel::Warning => Level::Warning,
        DiagnosticLevel::Info => Level::Info,
    };
    let message = match &diagnostic.suggestion {
        Some(suggestion) => format!("{}; {suggestion}", diagnostic.message),
        None => diagnostic.message.clone(),
    };
    let mut finding = Finding::new(
        intern(&format!("{SOURCE}:{}", diagnostic.rule)),
        level,
        diagnostic.line.max(1),
        message,
        String::new(),
    )
    .from_analyser(SOURCE, Evidence::Deterministic);
    // Every one of these names a single thing to correct, so correcting
    // that thing is enough; none of them asks for the file to be written
    // again from the start.
    finding.remediation = Remediation::Clarify;
    finding
}

#[cfg(test)]
mod tests {
    use super::{lint_text, to_finding, AGNIX_VERSION};
    use osf_lint_core::Level;
    use std::path::{Path, PathBuf};

    /// The engine's three levels arrive as three levels. An informational
    /// note must not come out as a warning, because a warning turns into
    /// an error under `--strict` and would block a push over a remark.
    #[test]
    fn each_engine_level_maps_to_its_own_level() {
        let file = PathBuf::from("SKILL.md");
        let info = agnix_core::Diagnostic::info(file.clone(), 3, 1, "AS-001", "a note");
        let warning = agnix_core::Diagnostic::warning(file.clone(), 3, 1, "AS-001", "a caution");
        let error = agnix_core::Diagnostic::error(file, 3, 1, "AS-001", "a fault");
        assert_eq!(to_finding(&info).level, Level::Info);
        assert_eq!(to_finding(&warning).level, Level::Warning);
        assert_eq!(to_finding(&error).level, Level::Error);
    }

    /// The version in the note must be the version that runs. A stale note
    /// would tell a reader to check a rule against the wrong rule set.
    #[test]
    fn version_matches_the_manifest() {
        let manifest = include_str!("../../Cargo.toml");
        let pinned = format!("agnix-core = \"={AGNIX_VERSION}\"");
        assert!(
            manifest.contains(&pinned),
            "Cargo.toml does not pin {pinned}"
        );
    }

    /// A file type the engine does not recognise reports that nothing ran,
    /// rather than a plain empty finding list a caller could read as clean.
    #[test]
    fn an_unrecognised_file_type_reports_that_nothing_ran() {
        let path = Path::new("notes.an-unknown-extension");
        let findings = lint_text(path, "hello", Path::new("."));
        assert_eq!(findings.len(), 1, "{findings:?}");
        let finding = findings.first().expect("one finding");
        assert_eq!(finding.rule, "agnix-did-not-run");
        assert!(
            finding.message.contains("none of its checks ran"),
            "{}",
            finding.message
        );
    }

    /// `lint_text` never touches disk: content the caller passes is what
    /// gets linted, not whatever a path of the same name holds (or does
    /// not hold) on disk. This is the property `check.rs`'s checkpoint-aware
    /// `lint_skill_files` depends on.
    #[test]
    fn lint_text_never_reads_the_path_from_disk() {
        let missing = Path::new("this-path-does-not-exist").join("SKILL.md");
        let findings = lint_text(&missing, "---\nname: a\n---\nbody\n", Path::new("."));
        assert!(
            findings.iter().all(|f| f.rule != "agnix-did-not-run"),
            "{findings:?}"
        );
    }
}
