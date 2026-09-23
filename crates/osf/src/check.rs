//! `osf check <name>`: one check `osf verify` already runs, callable on its
//! own over an explicit file list, so a moon task can run each check on
//! its own at a checkpoint instead of through the combined verify gate.

use crate::lints::{self, Context};
use crate::verify::Options;
use clap::ValueEnum;
use osf_lint_core::{Finding, Level};
use std::path::Path;

/// One check `osf verify` runs today, exposed on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum CheckName {
    Scan,
    ScanStaged,
    LintWriting,
    LintSkill,
    ScanCommits,
}

impl CheckName {
    /// The name this check answers to on the command line.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            CheckName::Scan => "scan",
            CheckName::ScanStaged => "scan-staged",
            CheckName::LintWriting => "lint-writing",
            CheckName::LintSkill => "lint-skill",
            CheckName::ScanCommits => "scan-commits",
        }
    }
}

/// Runs one named check over `files`. `scan-commits` ignores `files` and
/// reads `opts.base`, falling back to the default branch. `gate` ignores a
/// suppression marker for `lint-writing`, the same as `--gate` already does
/// for the checks `osf verify` runs.
///
/// # Errors
/// Returns an error when git could not run, a named file does not exist or
/// could not be read, or the default branch could not be resolved.
pub fn run_check(
    name: CheckName,
    opts: &Options,
    files: &[String],
    gate: bool,
) -> Result<Vec<(String, Finding)>, String> {
    match name {
        CheckName::Scan => {
            ensure_files_exist(opts.dir, files)?;
            scan_files(opts, files, false)
        }
        CheckName::ScanStaged => {
            ensure_staged_files_exist(opts.dir, files)?;
            scan_files(opts, files, true)
        }
        CheckName::LintWriting => {
            ensure_files_exist(opts.dir, files)?;
            lint_writing_files(opts, files, gate)
        }
        CheckName::LintSkill => {
            ensure_files_exist(opts.dir, files)?;
            lint_skill_files(opts, files)
        }
        CheckName::ScanCommits => scan_commits(opts),
    }
}

/// A file named to check that is not there: a could-not-run error naming
/// it, never a silent drop from the list.
fn ensure_files_exist(dir: &Path, files: &[String]) -> Result<(), String> {
    for path in files {
        if !dir.join(path).is_file() {
            return Err(format!("file not found: {path}"));
        }
    }
    Ok(())
}

/// `scan-staged` reads the index, so a file staged then deleted from an
/// unstaged working tree is still there to check: existence for this
/// check means present in the index, never the working tree.
fn ensure_staged_files_exist(dir: &Path, files: &[String]) -> Result<(), String> {
    for path in files {
        if crate::git::staged_content(dir, path).is_err() {
            return Err(format!("file not found: {path}"));
        }
    }
    Ok(())
}

/// The scan rules for this repository. A rule that could not run is
/// reported on standard error before any check runs, never left silent.
fn scan_rules(opts: &Options) -> Result<crate::scan::Rules, String> {
    let rules = crate::scan::Rules::build(opts.dir, &opts.config.scan)?;
    for note in rules.notes() {
        eprintln!("osf: {note}");
    }
    Ok(rules)
}

pub(crate) fn is_skill_file(path: &str) -> bool {
    Path::new(path).file_name().and_then(|n| n.to_str()) == Some("SKILL.md")
}

pub(crate) fn is_markdown(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|e| e == "md")
}

/// The nearest ancestor directory holding `SKILL.md`, for every named path
/// that has one, deduplicated.
pub(crate) fn skill_folders(dir: &Path, files: &[String]) -> Vec<std::path::PathBuf> {
    let mut found = std::collections::BTreeSet::new();
    for path in files {
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

fn scan_files(
    opts: &Options,
    files: &[String],
    staged: bool,
) -> Result<Vec<(String, Finding)>, String> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let (targets, _excluded) = opts.excluder.partition(files.to_vec());
    let rules = scan_rules(opts)?;
    let mut findings = Vec::new();
    for path in &targets {
        let bytes = if staged {
            crate::git::staged_content(opts.dir, path).map_err(|e| e.to_string())?
        } else {
            crate::git::content_at(opts.dir, "HEAD", path).map_err(|e| e.to_string())?
        };
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
    Ok(findings)
}

fn lint_writing_files(
    opts: &Options,
    files: &[String],
    ignore_suppress: bool,
) -> Result<Vec<(String, Finding)>, String> {
    let candidates: Vec<String> = files
        .iter()
        .filter(|p| is_markdown(p) && !is_skill_file(p))
        .cloned()
        .collect();
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let (markdown, _excluded) = opts.excluder.partition(candidates);
    let known = lints::load_known_names(&opts.config.writing.known_names, None)?;
    let mut findings = Vec::new();
    for path in &markdown {
        let bytes = crate::git::content_at(opts.dir, "HEAD", path).map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&bytes);
        let raw = lints::writing::lint_writing(
            &text,
            &known,
            &opts.config.writing,
            Context::Document,
            false,
            ignore_suppress,
        );
        let path_findings = match lints::parse_expectation(&text) {
            Some(expected) if lints::is_fixture_path(path) => {
                declared_fixture_findings(&expected, &raw)
            }
            _ => raw,
        };
        findings.extend(path_findings.into_iter().map(|f| (path.clone(), f)));
    }
    Ok(findings)
}

/// A writing fixture's own `osf-expect` declaration replaces its raw
/// findings here, the same way `osf lint writing` treats one.
fn declared_fixture_findings(
    expected: &std::collections::BTreeSet<String>,
    raw: &[Finding],
) -> Vec<Finding> {
    let forbidden: Vec<&String> = expected
        .iter()
        .filter(|id| lints::is_scan_rule(id))
        .collect();
    if !forbidden.is_empty() {
        return forbidden
            .into_iter()
            .map(|id| {
                Finding::new(
                    "expectation-forbidden-scan-rule",
                    Level::Error,
                    1,
                    format!("a scan finding cannot be declared expected: '{id}'"),
                    id.clone(),
                )
            })
            .collect();
    }
    let mismatch = lints::check_expectation(expected, raw);
    if mismatch.is_empty() {
        return Vec::new();
    }
    let missing = mismatch.missing.into_iter().map(|id| {
        Finding::new(
            "expectation-missing",
            Level::Error,
            1,
            format!("declared rule '{id}' did not fire; it may have stopped working"),
            id,
        )
    });
    let unexpected = mismatch.unexpected.into_iter().map(|id| {
        Finding::new(
            "expectation-unexpected",
            Level::Error,
            1,
            format!("rule '{id}' fired but this file did not declare it"),
            id,
        )
    });
    missing.chain(unexpected).collect()
}

fn lint_skill_files(opts: &Options, files: &[String]) -> Result<Vec<(String, Finding)>, String> {
    let candidates = skill_folders(opts.dir, files);
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let candidate_strings: Vec<String> = candidates
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();
    let (skill_dirs, _excluded) = opts.excluder.partition(candidate_strings);
    let known = lints::load_known_names(&opts.config.writing.known_names, None)?;
    let mut findings = Vec::new();
    for label in &skill_dirs {
        let full = opts.dir.join(label);
        let skill_findings = lints::skill::lint_skill_checked(
            &full,
            label,
            &opts.config.skill,
            &known,
            &opts.config.writing,
        )?;
        findings.extend(
            skill_findings
                .into_iter()
                .map(|sf| (format!("{label}/{}", sf.file), sf.finding)),
        );
    }
    Ok(findings)
}

fn scan_commits(opts: &Options) -> Result<Vec<(String, Finding)>, String> {
    let base = match &opts.base {
        Some(b) => b.clone(),
        None => crate::git::default_branch(opts.dir).map_err(|e| e.to_string())?,
    };
    let range = format!("{base}..HEAD");
    let hashes = crate::git::commit_hashes(opts.dir, &range).map_err(|e| e.to_string())?;
    if hashes.is_empty() {
        return Ok(Vec::new());
    }
    let rules = scan_rules(opts)?;
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
    Ok(findings)
}
