//! `osf verify`: one entry point every gate calls, so a pre-commit hook, a
//! pre-push hook, and continuous integration can never drift apart.

use crate::check::{self, is_markdown, is_skill_file, skill_folders, CheckName};
use crate::config::Config;
use crate::exclude::Excluder;
use crate::lints::{self, Context};
use osf_lint_core::{Finding, Level};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    PreCommit,
    PrePush,
    Ci,
}

/// What `osf verify` needs to run: where the repository lives, the base to
/// diff against, an optional commit message file, the resolved config, and
/// the exclude list already built into a matcher.
pub struct Options<'a> {
    pub dir: &'a Path,
    pub base: Option<String>,
    pub message_file: Option<&'a Path>,
    pub config: &'a Config,
    pub excluder: &'a Excluder,
}

struct CheckOutcome {
    name: &'static str,
    ran: bool,
    excluded: usize,
    findings: Vec<(String, Finding)>,
}

impl CheckOutcome {
    fn skipped(name: &'static str) -> Self {
        CheckOutcome {
            name,
            ran: false,
            excluded: 0,
            findings: Vec::new(),
        }
    }

    fn ran(name: &'static str, excluded: usize, findings: Vec<(String, Finding)>) -> Self {
        CheckOutcome {
            name,
            ran: true,
            excluded,
            findings,
        }
    }

    fn errors(&self) -> usize {
        self.findings
            .iter()
            .filter(|(_, f)| f.level == Level::Error && f.suppressed.is_none())
            .count()
    }

    fn warnings(&self) -> usize {
        self.findings
            .iter()
            .filter(|(_, f)| f.level == Level::Warning && f.suppressed.is_none())
            .count()
    }

    fn infos(&self) -> usize {
        self.findings
            .iter()
            .filter(|(_, f)| f.level == Level::Info && f.suppressed.is_none())
            .count()
    }
}

/// How many of `candidates` the exclude list drops, without the caller
/// needing the kept list too. Collapses the repeated
/// `excluder.partition(x.clone()).1` this module used to write at every
/// call site.
fn excluded_count(excluder: &Excluder, candidates: &[String]) -> usize {
    excluder.partition(candidates.to_vec()).1
}

/// Every check `osf verify` ran for one stage, and what each one found.
pub struct Report {
    checks: Vec<CheckOutcome>,
}

impl Report {
    #[must_use]
    pub fn total_errors(&self) -> usize {
        self.checks.iter().map(CheckOutcome::errors).sum()
    }

    #[must_use]
    pub fn total_warnings(&self) -> usize {
        self.checks.iter().map(CheckOutcome::warnings).sum()
    }

    /// Informational findings never decide the exit code, but they are
    /// counted and shown, never dropped.
    #[must_use]
    pub fn total_infos(&self) -> usize {
        self.checks.iter().map(CheckOutcome::infos).sum()
    }

    /// How many candidate files or folders the exclude list dropped,
    /// across every check. Never silent: a gate that stops checking
    /// something must say so, the same reasoning as a suppressed finding.
    #[must_use]
    pub fn total_excluded(&self) -> usize {
        self.checks.iter().map(|c| c.excluded).sum()
    }

    /// One line per check, then a total. A check that had nothing to run
    /// over says so plainly, never counted the same as a clean run.
    #[must_use]
    pub fn render_summary(&self, stage_label: &str) -> String {
        use std::fmt::Write as _;
        let mut out = format!("osf verify (stage: {stage_label})\n");
        for check in &self.checks {
            if check.ran {
                writeln!(
                    out,
                    "{}: {} error(s), {} warning(s), {} info, {} excluded",
                    check.name,
                    check.errors(),
                    check.warnings(),
                    check.infos(),
                    check.excluded
                )
                .expect("writing to a string never fails");
            } else {
                writeln!(out, "{}: nothing to check", check.name)
                    .expect("writing to a string never fails");
            }
        }
        if self.checks.iter().all(|c| !c.ran) {
            writeln!(out, "total: nothing to check").expect("writing to a string never fails");
        } else {
            writeln!(
                out,
                "total: {} error(s), {} warning(s), {} info, {} excluded",
                self.total_errors(),
                self.total_warnings(),
                self.total_infos(),
                self.total_excluded()
            )
            .expect("writing to a string never fails");
        }
        out
    }

    /// Every finding, labelled by the check that found it and the file or
    /// commit it came from.
    pub fn findings(&self) -> impl Iterator<Item = (&'static str, &str, &Finding)> {
        self.checks.iter().flat_map(|c| {
            c.findings
                .iter()
                .map(move |(name, f)| (c.name, name.as_str(), f))
        })
    }
}

/// Runs every check `stage` calls for. The caller reads
/// [`Report::total_errors`] to choose an exit code: 0 with none, 1 with at
/// least one.
///
/// # Errors
/// Returns an error when git could not run, a file could not be read, or
/// the default branch could not be found. That is the tool failing to run
/// at all, never the same as running and finding nothing.
pub fn run(stage: Stage, opts: &Options) -> Result<Report, String> {
    match stage {
        Stage::PreCommit => pre_commit(opts),
        Stage::PrePush => pre_push(opts, false),
        Stage::Ci => pre_push(opts, true),
    }
}

fn pre_commit(opts: &Options) -> Result<Report, String> {
    let staged = crate::git::staged_files(opts.dir).map_err(|e| e.to_string())?;
    let scan_outcome = if staged.is_empty() {
        CheckOutcome::skipped("scan")
    } else {
        let excluded = excluded_count(opts.excluder, &staged);
        let findings = check::run_check(CheckName::ScanStaged, opts, &staged, false)?;
        CheckOutcome::ran("scan", excluded, findings)
    };

    let message_outcome = match opts.message_file {
        None => CheckOutcome::skipped("lint writing (commit message)"),
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            let known = lints::load_known_names(&opts.config.writing.known_names, None)?;
            let findings = lints::writing::lint_writing(
                &text,
                &known,
                &opts.config.writing,
                Context::Commit,
                false,
                false,
            );
            let label = path.display().to_string();
            CheckOutcome::ran(
                "lint writing (commit message)",
                0,
                findings.into_iter().map(|f| (label.clone(), f)).collect(),
            )
        }
    };

    Ok(Report {
        checks: vec![scan_outcome, message_outcome],
    })
}

fn pre_push(opts: &Options, ignore_suppress: bool) -> Result<Report, String> {
    let base = match &opts.base {
        Some(b) => b.clone(),
        None => crate::git::default_branch(opts.dir).map_err(|e| e.to_string())?,
    };
    let changed = crate::git::changed_files(opts.dir, &base).map_err(|e| e.to_string())?;

    let scan_outcome = if changed.is_empty() {
        CheckOutcome::skipped("scan")
    } else {
        let excluded = excluded_count(opts.excluder, &changed);
        let findings = check::run_check(CheckName::Scan, opts, &changed, false)?;
        CheckOutcome::ran("scan", excluded, findings)
    };

    let writing_candidates: Vec<String> = changed
        .iter()
        .filter(|p| is_markdown(p) && !is_skill_file(p))
        .cloned()
        .collect();
    let writing_outcome = if writing_candidates.is_empty() {
        CheckOutcome::skipped("lint writing")
    } else {
        let excluded = excluded_count(opts.excluder, &writing_candidates);
        let findings = check::run_check(
            CheckName::LintWriting,
            opts,
            &writing_candidates,
            ignore_suppress,
        )?;
        CheckOutcome::ran("lint writing", excluded, findings)
    };

    let skill_candidates = skill_folders(opts.dir, &changed);
    let skill_outcome = if skill_candidates.is_empty() {
        CheckOutcome::skipped("lint skill")
    } else {
        let candidate_strings: Vec<String> = skill_candidates
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        let excluded = excluded_count(opts.excluder, &candidate_strings);
        let findings = check::run_check(CheckName::LintSkill, opts, &changed, false)?;
        CheckOutcome::ran("lint skill", excluded, findings)
    };

    let range = format!("{base}..HEAD");
    let hashes = crate::git::commit_hashes(opts.dir, &range).map_err(|e| e.to_string())?;
    let commit_outcome = if hashes.is_empty() {
        CheckOutcome::skipped("scan (commits)")
    } else {
        let findings = check::run_check(CheckName::ScanCommits, opts, &[], false)?;
        CheckOutcome::ran("scan (commits)", 0, findings)
    };

    Ok(Report {
        checks: vec![scan_outcome, writing_outcome, skill_outcome, commit_outcome],
    })
}
