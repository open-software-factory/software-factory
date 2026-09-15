//! `osf verify`: one entry point every gate calls, so a pre-commit hook, a
//! pre-push hook, and continuous integration can never drift apart.

use crate::config::Config;
use crate::lint::{self, Context};
use osf_lint_core::{Finding, Level};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    PreCommit,
    PrePush,
    Ci,
}

/// What `osf verify` needs to run: where the repository lives, the base to
/// diff against, an optional commit message file, and the resolved config.
pub struct Options<'a> {
    pub dir: &'a Path,
    pub base: Option<String>,
    pub message_file: Option<&'a Path>,
    pub config: &'a Config,
}

struct CheckOutcome {
    name: &'static str,
    ran: bool,
    findings: Vec<(String, Finding)>,
}

impl CheckOutcome {
    fn skipped(name: &'static str) -> Self {
        CheckOutcome {
            name,
            ran: false,
            findings: Vec::new(),
        }
    }

    fn ran(name: &'static str, findings: Vec<(String, Finding)>) -> Self {
        CheckOutcome {
            name,
            ran: true,
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
                    "{}: {} error(s), {} warning(s)",
                    check.name,
                    check.errors(),
                    check.warnings()
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
                "total: {} error(s), {} warning(s)",
                self.total_errors(),
                self.total_warnings()
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
        let rules = crate::scan::Rules::build(opts.config.scan.clone())?;
        let mut findings = Vec::new();
        for path in &staged {
            let bytes = crate::git::staged_content(opts.dir, path).map_err(|e| e.to_string())?;
            if crate::scan::is_binary(&bytes) {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            findings.extend(
                rules
                    .scan_text(&text, Context::Document)
                    .into_iter()
                    .map(|f| (path.clone(), f)),
            );
        }
        CheckOutcome::ran("scan", findings)
    };

    let message_outcome = match opts.message_file {
        None => CheckOutcome::skipped("lint writing (commit message)"),
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            let known = lint::load_known_names(&opts.config.writing.known_names, None)?;
            let findings = lint::lint_writing(
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
                findings.into_iter().map(|f| (label.clone(), f)).collect(),
            )
        }
    };

    Ok(Report {
        checks: vec![scan_outcome, message_outcome],
    })
}

fn is_skill_file(path: &str) -> bool {
    Path::new(path).file_name().and_then(|n| n.to_str()) == Some("SKILL.md")
}

fn is_markdown(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|e| e == "md")
}

/// The nearest ancestor directory holding `SKILL.md`, for every changed
/// path that has one, deduplicated.
fn skill_folders(dir: &Path, changed: &[String]) -> Vec<std::path::PathBuf> {
    let mut found = std::collections::BTreeSet::new();
    for path in changed {
        let mut current = Path::new(path).parent().map(Path::to_path_buf);
        while let Some(candidate) = current {
            if candidate.as_os_str().is_empty() {
                break;
            }
            if dir.join(&candidate).join("SKILL.md").is_file() {
                found.insert(candidate);
                break;
            }
            current = candidate.parent().map(Path::to_path_buf);
        }
    }
    found.into_iter().collect()
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
        let rules = crate::scan::Rules::build(opts.config.scan.clone())?;
        let mut findings = Vec::new();
        for path in &changed {
            let bytes =
                crate::git::content_at(opts.dir, "HEAD", path).map_err(|e| e.to_string())?;
            if crate::scan::is_binary(&bytes) {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            findings.extend(
                rules
                    .scan_text(&text, Context::Document)
                    .into_iter()
                    .map(|f| (path.clone(), f)),
            );
        }
        CheckOutcome::ran("scan", findings)
    };

    let markdown: Vec<&String> = changed
        .iter()
        .filter(|p| is_markdown(p) && !is_skill_file(p))
        .collect();
    let writing_outcome = if markdown.is_empty() {
        CheckOutcome::skipped("lint writing")
    } else {
        let known = lint::load_known_names(&opts.config.writing.known_names, None)?;
        let mut findings = Vec::new();
        for path in &markdown {
            let bytes =
                crate::git::content_at(opts.dir, "HEAD", path).map_err(|e| e.to_string())?;
            let text = String::from_utf8_lossy(&bytes);
            findings.extend(
                lint::lint_writing(
                    &text,
                    &known,
                    &opts.config.writing,
                    Context::Document,
                    false,
                    ignore_suppress,
                )
                .into_iter()
                .map(|f| ((*path).clone(), f)),
            );
        }
        CheckOutcome::ran("lint writing", findings)
    };

    let skill_dirs = skill_folders(opts.dir, &changed);
    let skill_outcome = if skill_dirs.is_empty() {
        CheckOutcome::skipped("lint skill")
    } else {
        let known = lint::load_known_names(&opts.config.writing.known_names, None)?;
        let mut findings = Vec::new();
        for skill_dir in &skill_dirs {
            let full = opts.dir.join(skill_dir);
            let label = skill_dir.to_string_lossy().replace('\\', "/");
            let skill_findings =
                lint::skill::lint_skill(&full, &opts.config.skill, &known, &opts.config.writing)?;
            findings.extend(
                skill_findings
                    .into_iter()
                    .map(|sf| (format!("{label}/{}", sf.file), sf.finding)),
            );
        }
        CheckOutcome::ran("lint skill", findings)
    };

    let range = format!("{base}..HEAD");
    let hashes = crate::git::commit_hashes(opts.dir, &range).map_err(|e| e.to_string())?;
    let commit_outcome = if hashes.is_empty() {
        CheckOutcome::skipped("scan (commits)")
    } else {
        let rules = crate::scan::Rules::build(opts.config.scan.clone())?;
        let mut findings = Vec::new();
        for hash in &hashes {
            let message = crate::git::commit_message(opts.dir, hash).map_err(|e| e.to_string())?;
            findings.extend(
                rules
                    .scan_text(&message, Context::Commit)
                    .into_iter()
                    .map(|f| (hash.clone(), f)),
            );
        }
        CheckOutcome::ran("scan (commits)", findings)
    };

    Ok(Report {
        checks: vec![scan_outcome, writing_outcome, skill_outcome, commit_outcome],
    })
}
