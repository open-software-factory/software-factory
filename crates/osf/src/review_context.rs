//! Assembles the context section of a reviewer's prompt for one lens: the
//! diff against the base, widened by the tier's depth, plus whatever else
//! the lens declares it needs. A declared input that cannot be found makes
//! the whole lens could-not-run, naming that input; nothing here ever
//! guesses at a missing work item or a missing decision record. Every path
//! named in the returned text is forward-slash and relative to the
//! repository. Before the text is handed back, `osf scan`'s own rules run
//! over it and redact any match, so a secret already in the repository
//! never reaches a reviewer's prompt; the result is then capped in size,
//! with a marker naming what was left out.

use crate::config::ScanConfig;
use crate::lenses::{ContextInput, Depth, Lens};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Where a lens's context comes from: the repository, the base it diffs
/// against, and the work item file the caller saved, if any.
#[derive(Debug, Clone, Copy)]
pub struct Sources<'a> {
    pub root: &'a Path,
    pub base: &'a str,
    pub work_item: Option<&'a Path>,
}

/// The largest context this module ever hands back, in bytes. Chosen so a
/// single lens's prompt stays well inside every shipped reviewer harness's
/// context window even at the widest depth, while still holding a whole
/// diff for an ordinary change; a lens that needs more must trigger at a
/// lower tier instead of silently sending an unbounded prompt.
const MAX_CONTEXT_BYTES: usize = 60_000;

/// Builds the context section of `lens`'s prompt at `depth`, from `sources`.
///
/// # Errors
/// Names the declared input that could not be found: a missing work item
/// file, a work item with no acceptance heading, a decision record the diff
/// links to that does not exist, a missing `docs/architecture`, or no
/// changed files against the base.
pub fn build(lens: &Lens, depth: Depth, sources: &Sources) -> Result<String, String> {
    let mut sections = Vec::with_capacity(lens.context.len());
    for input in &lens.context {
        let section = section_for(*input, depth, sources)
            .map_err(|reason| format!("{}: {reason}", input_name(*input)))?;
        sections.push(section);
    }
    let joined = sections.join("\n\n");
    let redacted =
        redact_secrets(sources.root, &joined).map_err(|reason| format!("context: {reason}"))?;
    Ok(cap(redacted))
}

/// The kebab-case name a declared input is known by, matching how a lens
/// file's own `context` list spells it.
fn input_name(input: ContextInput) -> &'static str {
    match input {
        ContextInput::Diff => "diff",
        ContextInput::WorkItem => "work-item",
        ContextInput::AcceptanceCriteria => "acceptance-criteria",
        ContextInput::DecisionRecords => "decision-records",
        ContextInput::ArchitectureDocs => "architecture-docs",
        ContextInput::EntryPoints => "entry-points",
    }
}

fn section_for(input: ContextInput, depth: Depth, sources: &Sources) -> Result<String, String> {
    match input {
        ContextInput::Diff => diff_section(sources, depth),
        ContextInput::WorkItem => work_item_section(sources),
        ContextInput::AcceptanceCriteria => acceptance_criteria_section(sources),
        ContextInput::DecisionRecords => decision_records_section(sources),
        ContextInput::ArchitectureDocs => architecture_docs_section(sources),
        ContextInput::EntryPoints => entry_points_section(sources),
    }
}

/// `path`, rendered with forward slashes whatever the platform's own separator is.
fn forward_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

// --- the diff, and its depth-widened neighbours ---------------------------

fn diff_section(sources: &Sources, depth: Depth) -> Result<String, String> {
    let changed =
        crate::git::changed_files(sources.root, sources.base).map_err(|e| e.to_string())?;
    if changed.is_empty() {
        return Err("no changed files against the base".to_string());
    }
    let range = format!("{}...HEAD", sources.base);
    let patch = crate::git::diff_patch(sources.root, &range).map_err(|e| e.to_string())?;
    let mut out = format!("## diff, base {}\n{patch}", sources.base);

    match depth {
        Depth::Diff => {}
        Depth::DiffAndCallers => {
            let symbols = changed_function_names(&patch);
            let callers = caller_files(sources.root, &symbols, &changed);
            if !callers.is_empty() {
                out.push_str("\n\n## files that name a changed symbol\n");
                out.push_str(&render_files(sources.root, &callers));
            }
        }
        Depth::Module => {
            let siblings = module_siblings(sources.root, &changed);
            if !siblings.is_empty() {
                out.push_str("\n\n## other files in the changed modules\n");
                out.push_str(&render_files(sources.root, &siblings));
            }
        }
    }
    Ok(out)
}

/// Every function name a diff's added or removed lines mention, in the
/// order first seen, each named once.
fn changed_function_names(patch: &str) -> Vec<String> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern =
        PATTERN.get_or_init(|| Regex::new(r"^[+-].*\bfn\s+(\w+)").expect("pattern compiles"));
    let mut names: Vec<String> = Vec::new();
    for captures in patch.lines().filter_map(|line| pattern.captures(line)) {
        if let Some(name) = captures.get(1) {
            let name = name.as_str().to_string();
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// Every tracked file naming one of `symbols`, other than a path already in `exclude`.
fn caller_files(root: &Path, symbols: &[String], exclude: &[String]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for symbol in symbols {
        for path in git_grep(root, symbol) {
            if !exclude.contains(&path) && !found.contains(&path) {
                found.push(path);
            }
        }
    }
    found
}

/// The tracked files `git grep` finds `pattern` in, as a whole word, or
/// empty when it finds none or cannot run: neither is an error here, since
/// a lens's extra context is additive, never required on its own.
fn git_grep(root: &Path, pattern: &str) -> Vec<String> {
    let mut command = std::process::Command::new("git");
    command
        .current_dir(root)
        .args(["grep", "-l", "-w", "-F", pattern]);
    crate::scrub_git_env_for_dir(&mut command, root);
    let Ok(output) = command.output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.replace('\\', "/"))
        .collect()
}

/// Every file in each of `changed`'s own directories, other than `changed` itself.
fn module_siblings(root: &Path, changed: &[String]) -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    for path in changed {
        let dir = Path::new(path)
            .parent()
            .map(forward_slash)
            .unwrap_or_default();
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    let mut siblings: Vec<String> = Vec::new();
    for dir in dirs {
        let full = if dir.is_empty() {
            root.to_path_buf()
        } else {
            root.join(&dir)
        };
        let Ok(entries) = std::fs::read_dir(&full) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            let rel = if dir.is_empty() {
                name
            } else {
                format!("{dir}/{name}")
            };
            if !changed.contains(&rel) && !siblings.contains(&rel) {
                siblings.push(rel);
            }
        }
    }
    siblings.sort();
    siblings
}

/// Renders each of `paths` as its own labelled block, its content read from `root`.
fn render_files(root: &Path, paths: &[String]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for path in paths {
        let full = crate::git::to_local_path(root, path);
        let content =
            std::fs::read_to_string(&full).unwrap_or_else(|_| "(could not be read)".to_string());
        let _ = write!(out, "### {path}\n{content}\n");
    }
    out
}

// --- the work item and its acceptance criteria -----------------------------

fn work_item_section(sources: &Sources) -> Result<String, String> {
    let body = read_work_item(sources)?;
    Ok(format!("## work item\n{body}"))
}

fn acceptance_criteria_section(sources: &Sources) -> Result<String, String> {
    let body = read_work_item(sources)?;
    match acceptance_section_text(&body) {
        Some(text) => Ok(format!("## acceptance criteria\n{text}")),
        None => {
            Err("the work item has no \"Done when\" or \"Acceptance criteria\" heading".to_string())
        }
    }
}

/// The work item's saved body, from the file the caller named.
fn read_work_item(sources: &Sources) -> Result<String, String> {
    let path = sources
        .work_item
        .ok_or_else(|| "no work item file was given".to_string())?;
    std::fs::read_to_string(path)
        .map_err(|e| format!("{}: cannot be read: {e}", forward_slash(path)))
}

/// How many leading `#` characters open `line` as a heading, when a space follows them.
fn heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    let is_heading = hashes > 0 && trimmed.as_bytes().get(hashes) == Some(&b' ');
    is_heading.then_some(hashes)
}

/// Whether `line` is a heading naming "Done when" or "Acceptance criteria", case-insensitively.
fn is_acceptance_heading(line: &str) -> bool {
    let Some(level) = heading_level(line) else {
        return false;
    };
    let trimmed = line.trim_start();
    let Some(rest) = trimmed.get(level..) else {
        return false;
    };
    let lower = rest.trim().to_lowercase();
    lower.contains("done when") || lower.contains("acceptance criteria")
}

/// The acceptance heading in `body` and everything under it, up to the next
/// heading at the same or a shallower level, or `None` when there is no such heading.
fn acceptance_section_text(body: &str) -> Option<String> {
    let lines: Vec<&str> = body.lines().collect();
    let start = lines.iter().position(|line| is_acceptance_heading(line))?;
    let level = heading_level(lines.get(start)?)?;
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| heading_level(line).is_some_and(|found| found <= level))
        .map_or(lines.len(), |(i, _)| i);
    lines.get(start..end).map(|section| section.join("\n"))
}

// --- decision records and architecture docs --------------------------------

fn decision_records_section(sources: &Sources) -> Result<String, String> {
    use std::fmt::Write as _;
    let changed =
        crate::git::changed_files(sources.root, sources.base).map_err(|e| e.to_string())?;
    let range = format!("{}...HEAD", sources.base);
    let patch = crate::git::diff_patch(sources.root, &range).map_err(|e| e.to_string())?;

    let mut refs: Vec<String> = changed
        .into_iter()
        .filter(|path| is_decision_record_path(path))
        .collect();
    for found in decision_record_links(&patch) {
        if !refs.contains(&found) {
            refs.push(found);
        }
    }

    if refs.is_empty() {
        return Ok("## decision records\n(none touched or linked by this change)".to_string());
    }

    let mut out = String::from("## decision records\n");
    for path in &refs {
        let full = crate::git::to_local_path(sources.root, path);
        let content = std::fs::read_to_string(&full)
            .map_err(|e| format!("{path} is linked but does not exist: {e}"))?;
        let _ = write!(out, "### {path}\n{content}\n");
    }
    Ok(out)
}

fn is_decision_record_path(path: &str) -> bool {
    path.starts_with("docs/architecture/decisions/")
        && Path::new(path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}

/// Every decision record path a diff's own text names, once each.
fn decision_record_links(patch: &str) -> Vec<String> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"docs/architecture/decisions/\d{4}-[A-Za-z0-9-]+\.md")
            .expect("pattern compiles")
    });
    let mut found = Vec::new();
    for hit in pattern.find_iter(patch) {
        let record_path = hit.as_str().to_string();
        if !found.contains(&record_path) {
            found.push(record_path);
        }
    }
    found
}

fn architecture_docs_section(sources: &Sources) -> Result<String, String> {
    use std::fmt::Write as _;
    let dir = sources.root.join("docs").join("architecture");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Err("docs/architecture is missing".to_string());
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        })
        .collect();
    files.sort();
    if files.is_empty() {
        return Err("docs/architecture has no .md files".to_string());
    }

    let mut out = String::from("## architecture docs\n");
    for file in &files {
        let rel = forward_slash(file.strip_prefix(sources.root).unwrap_or(file));
        let content =
            std::fs::read_to_string(file).map_err(|e| format!("{rel}: cannot be read: {e}"))?;
        let _ = write!(out, "### {rel}\n{content}\n");
    }
    Ok(out)
}

// --- entry points ------------------------------------------------------

fn entry_points_section(sources: &Sources) -> Result<String, String> {
    let changed =
        crate::git::changed_files(sources.root, sources.base).map_err(|e| e.to_string())?;
    let range = format!("{}...HEAD", sources.base);
    let patch = crate::git::diff_patch(sources.root, &range).map_err(|e| e.to_string())?;
    let symbols = changed_function_names(&patch);
    let callers = caller_files(sources.root, &symbols, &changed);
    if callers.is_empty() {
        return Ok("## entry points\n(no other file calls a changed function)".to_string());
    }
    Ok(format!(
        "## entry points\n{}",
        render_files(sources.root, &callers)
    ))
}

// --- redaction and the size cap --------------------------------------------

/// The `[scan]` table of `root`'s own `osf.toml`, or the compiled defaults
/// when the file, the table, or a field in it is missing or malformed: a
/// lens's context is always redacted, whether or not a repository has
/// configured anything extra for it to catch.
fn scan_config(root: &Path) -> ScanConfig {
    let Ok(text) = std::fs::read_to_string(root.join("osf.toml")) else {
        return ScanConfig::default();
    };
    let Ok(value) = toml::from_str::<toml::Value>(&text) else {
        return ScanConfig::default();
    };
    value
        .get("scan")
        .cloned()
        .and_then(|scan| scan.try_into().ok())
        .unwrap_or_default()
}

/// `text`, with every match of `osf scan`'s own rules replaced by a marker
/// naming the rule: the same rules that stop a secret, a session link or a
/// local path reaching a public repository stop it reaching a reviewer's
/// prompt first. A denylist match never even hands back its own excerpt, so
/// its whole line is replaced; every other rule replaces only the text it
/// matched.
///
/// # Errors
/// Returns an error if a configured denylist pattern or session link prefix
/// is not valid: a context this module cannot trust to be scanned must
/// never be sent unredacted instead.
fn redact_secrets(root: &Path, text: &str) -> Result<String, String> {
    let cfg = scan_config(root);
    let rules = crate::scan::Rules::build(root, &cfg)?;
    let findings = rules.scan_text(text, osf_lint_core::Context::Document);
    if findings.is_empty() {
        return Ok(text.to_string());
    }
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    for finding in &findings {
        let Some(line) = finding
            .line
            .checked_sub(1)
            .and_then(|index| lines.get_mut(index))
        else {
            continue;
        };
        if finding.excerpt.is_empty() {
            *line = format!("[redacted by {}]", finding.rule);
        } else {
            *line = line.replace(
                finding.excerpt.as_str(),
                &format!("[redacted by {}]", finding.rule),
            );
        }
    }
    lines.push(format!(
        "[{} line(s) redacted before this prompt was built]",
        findings.len()
    ));
    Ok(lines.join("\n"))
}

/// `text`, cut to [`MAX_CONTEXT_BYTES`] with a marker naming how much was left out.
fn cap(text: String) -> String {
    if text.len() <= MAX_CONTEXT_BYTES {
        return text;
    }
    let mut cut = MAX_CONTEXT_BYTES;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let Some(kept) = text.get(..cut) else {
        return text;
    };
    let omitted = text.len() - cut;
    format!(
        "{kept}\n\n[context truncated: {omitted} of {} bytes left out to stay under the {MAX_CONTEXT_BYTES}-byte cap]",
        text.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_level_needs_a_space_after_the_hashes() {
        assert_eq!(heading_level("## Done when"), Some(2));
        assert_eq!(heading_level("##Done when"), None);
        assert_eq!(heading_level("not a heading"), None);
    }

    #[test]
    fn is_acceptance_heading_matches_either_wording_case_insensitively() {
        assert!(is_acceptance_heading("## Done When"));
        assert!(is_acceptance_heading("### acceptance CRITERIA"));
        assert!(!is_acceptance_heading("## Summary"));
    }

    #[test]
    fn acceptance_section_stops_at_the_next_heading_of_the_same_level() {
        let body = "# Title\n\ntext\n\n## Done when\n- one\n- two\n\n## Notes\nmore\n";
        let section = acceptance_section_text(body).expect("heading found");
        assert!(section.contains("- one"));
        assert!(section.contains("- two"));
        assert!(!section.contains("more"));
    }

    #[test]
    fn acceptance_section_is_none_with_no_matching_heading() {
        let body = "# Title\n\nno acceptance heading here\n";
        assert!(acceptance_section_text(body).is_none());
    }

    #[test]
    fn changed_function_names_reads_added_and_removed_lines() {
        let patch = "+pub fn added_one() {}\n-fn removed_one() {}\n context line\n";
        let names = changed_function_names(patch);
        assert!(names.contains(&"added_one".to_string()));
        assert!(names.contains(&"removed_one".to_string()));
    }

    #[test]
    fn decision_record_links_finds_a_referenced_path_once() {
        let patch = "+see docs/architecture/decisions/0016-the-review-check.md and again docs/architecture/decisions/0016-the-review-check.md\n";
        let found = decision_record_links(patch);
        assert_eq!(
            found,
            vec!["docs/architecture/decisions/0016-the-review-check.md".to_string()]
        );
    }

    #[test]
    fn is_decision_record_path_requires_the_decisions_directory_and_extension() {
        assert!(is_decision_record_path(
            "docs/architecture/decisions/0001-thing.md"
        ));
        assert!(!is_decision_record_path("docs/architecture/overview.md"));
        assert!(!is_decision_record_path(
            "docs/architecture/decisions/0001-thing.txt"
        ));
    }

    #[test]
    fn cap_leaves_short_text_untouched() {
        let text = "short".to_string();
        assert_eq!(cap(text.clone()), text);
    }

    #[test]
    fn cap_truncates_long_text_and_says_how_much_was_left_out() {
        let text = "x".repeat(MAX_CONTEXT_BYTES + 500);
        let capped = cap(text);
        assert!(capped.len() < MAX_CONTEXT_BYTES + 500);
        assert!(capped.contains("truncated"));
        assert!(capped.contains("500"));
    }

    #[test]
    fn input_name_matches_the_kebab_case_a_lens_file_declares() {
        assert_eq!(input_name(ContextInput::WorkItem), "work-item");
        assert_eq!(
            input_name(ContextInput::AcceptanceCriteria),
            "acceptance-criteria"
        );
        assert_eq!(
            input_name(ContextInput::DecisionRecords),
            "decision-records"
        );
        assert_eq!(
            input_name(ContextInput::ArchitectureDocs),
            "architecture-docs"
        );
        assert_eq!(input_name(ContextInput::EntryPoints), "entry-points");
        assert_eq!(input_name(ContextInput::Diff), "diff");
    }
}
