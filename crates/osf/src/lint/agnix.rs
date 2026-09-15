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

use agnix_core::{DiagnosticLevel, LintConfig, ValidationOutcome};
use osf_lint_core::{intern, Evidence, Finding, Level, Remediation};
use std::path::Path;

/// The version of the engine compiled in. `version_matches_the_manifest`
/// keeps this equal to the pinned dependency.
pub const AGNIX_VERSION: &str = "0.53.0";

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

/// Run the engine over one file and return its diagnostics as findings.
///
/// A file the engine cannot read, does not recognise, or fails on produces a
/// finding of its own. Silence here would mean a malformed skill file passed
/// the check by being unreadable, which is the opposite of what a gate is
/// for.
pub fn lint_file(path: &Path, skill_dir: &Path) -> Vec<Finding> {
    let mut config = LintConfig::default();
    config.set_root_dir(skill_dir.to_path_buf());

    match agnix_core::validate_file(path, &config) {
        Ok(ValidationOutcome::Success(diagnostics)) => diagnostics.iter().map(to_finding).collect(),
        Ok(ValidationOutcome::Skipped) => vec![did_not_run(
            "the agnix engine does not recognise this file type, so none of its checks ran"
                .to_string(),
        )],
        Ok(ValidationOutcome::IoError(error)) => vec![did_not_run(format!(
            "the agnix engine could not read this file, so none of its checks ran: {error}"
        ))],
        Err(error) => vec![did_not_run(format!(
            "the agnix engine failed on this file, so none of its checks ran: {error}"
        ))],
        // The outcome type is open to new variants. A version of the engine
        // that adds one must fail loudly here rather than read as clean.
        Ok(other) => vec![did_not_run(format!(
            "the agnix engine returned an outcome this build does not understand, so its \
             result was not read: {other:?}"
        ))],
    }
}

fn did_not_run(message: String) -> Finding {
    Finding::new(DID_NOT_RUN, Level::Error, 1, message, String::new())
        .from_analyser(SOURCE, Evidence::Deterministic)
}

/// `osf` has two levels and the other engine has three. An informational
/// diagnostic is reported as a warning rather than dropped: a gate that
/// silently discards a level is a gate with a gap in it.
fn to_finding(diagnostic: &agnix_core::Diagnostic) -> Finding {
    let level = match diagnostic.level {
        DiagnosticLevel::Error => Level::Error,
        DiagnosticLevel::Warning | DiagnosticLevel::Info => Level::Warning,
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
    use super::{lint_file, AGNIX_VERSION};
    use std::path::Path;

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

    #[test]
    fn an_unreadable_path_reports_that_nothing_ran() {
        let missing = Path::new("this-path-does-not-exist").join("SKILL.md");
        let findings = lint_file(&missing, Path::new("this-path-does-not-exist"));
        assert_eq!(findings.len(), 1, "{findings:?}");
        let finding = findings.first().expect("one finding");
        assert_eq!(finding.rule, "agnix-did-not-run");
        assert!(
            finding.message.contains("none of its checks ran"),
            "{}",
            finding.message
        );
    }
}
