//! Assembles the metadata section of a reviewer's prompt: the pull request's
//! number, title and body, the base and head commits, the changed files with
//! their line counts, the work item, and the paths of the linked decision
//! records. It holds no diff and no file content: the reviewer runs inside a
//! read-only checkout of the change at the head commit, and reads the diff
//! and the commit log from a file in a read-only folder ([`ChangeFolder`]).
//!
//! A lens that declares `work-item` or `acceptance-criteria` makes the whole
//! lens could-not-run, naming that input, when the work item is missing;
//! nothing here guesses. Every other declared input is read from the
//! checkout by the reviewer. Every path in the text is forward-slash and
//! relative to the repository. Before the text is handed back, `osf scan`'s
//! own rules run over it and redact any match, so a secret in text osf
//! inserts, such as the pull request body or the work item, never reaches a
//! reviewer's prompt.

use crate::config::ScanConfig;
use crate::lenses::{ContextInput, Lens};
use regex::Regex;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The pull request under review, as the code host's event reports it.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
}

/// The pull request a saved work item must belong to: its repository and head commit.
#[derive(Debug, Clone, Copy)]
pub struct WorkItemBinding<'a> {
    pub repository: &'a str,
    pub head: &'a str,
}

/// Where a lens's metadata comes from: the repository, the base it diffs
/// against, the work item file the caller saved, if any, the pull request,
/// if any, and where the trusted `[scan]` redaction settings are read from.
#[derive(Debug, Clone, Copy)]
pub struct Sources<'a> {
    pub root: &'a Path,
    /// Where `redact_secrets` reads the `[scan]` table of `osf.toml` from.
    /// A pull request must not be able to turn off `scan-secret`, or any
    /// other scan rule, and have that loosen what a reviewer's prompt gets
    /// to see. Defaults to `root` when the caller has no separate trusted
    /// tree.
    pub config_root: &'a Path,
    pub base: &'a str,
    pub work_item: Option<&'a Path>,
    pub pull_request: Option<&'a PullRequest>,
    /// The pull request a saved work item must match, when one is known.
    pub work_item_binding: Option<WorkItemBinding<'a>>,
}

/// Builds the metadata section of `lens`'s prompt from `sources`.
///
/// # Errors
/// Names the declared input that could not be found: a missing work item
/// file, a work item with no acceptance heading, or no changed files
/// against the base. Also names the reason when git cannot be read or the
/// redaction rules cannot be built.
pub fn build(lens: &Lens, sources: &Sources) -> Result<String, String> {
    let needs_work_item = lens.context.contains(&ContextInput::WorkItem)
        || lens.context.contains(&ContextInput::AcceptanceCriteria);
    let work_item = if needs_work_item {
        Some(read_work_item(sources).map_err(|reason| format!("work-item: {reason}"))?)
    } else if sources.work_item.is_some() {
        // A lens that does not need the work item still shows it when there is one.
        read_work_item(sources).ok()
    } else {
        None
    };
    if lens.context.contains(&ContextInput::AcceptanceCriteria) {
        let body = work_item.as_deref().unwrap_or_default();
        if acceptance_section_text(body).is_none() {
            return Err(if acceptance_heading_present(body) {
                "acceptance-criteria: the acceptance criteria section is empty".to_string()
            } else {
                "acceptance-criteria: the work item has no \"Done when\" or \"Acceptance criteria\" heading"
                    .to_string()
            });
        }
    }

    let mut text = String::new();
    if let Some(pr) = sources.pull_request {
        let _ = write!(
            text,
            "## pull request\nnumber: {}\ntitle: {}\nbody:\n{}\n\n",
            pr.number,
            pr.title,
            pr.body.as_deref().unwrap_or("(none)")
        );
    }
    text.push_str(&commits_section(sources)?);
    text.push_str("\n\n");
    text.push_str(&changed_files_section(sources)?);
    if let Some(body) = &work_item {
        let _ = write!(text, "\n\n## work item\n{body}");
    }
    text.push_str("\n\n");
    text.push_str(&decision_records_section(sources)?);

    let (redacted, redactions) = redact_secrets(sources.root, sources.config_root, &text)
        .map_err(|reason| format!("context: {reason}"))?;
    Ok(append_redaction_note(redacted, redactions))
}

/// A folder, read-only while it lives, that holds one file: the commit log and
/// the full diff of the reviewed range. A reviewer reads it with file tools,
/// so it needs no command that runs git. The folder is removed on drop.
#[derive(Debug)]
pub struct ChangeFolder {
    dir: PathBuf,
}

/// The name of the file inside a [`ChangeFolder`].
pub const CHANGE_FILE_NAME: &str = "change.diff";

impl ChangeFolder {
    /// Writes the commit log and `git diff --no-color <base>...<head>` into a
    /// new folder under the OS temp directory, then makes the file and the
    /// folder read-only. Secret redaction covers the text, as it does the prompt.
    ///
    /// # Errors
    /// Names the reason when git cannot be read, the redaction rules cannot
    /// be built, or the folder or file cannot be written.
    pub fn create(sources: &Sources) -> Result<Self, String> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let log =
            crate::git::log_since(sources.root, sources.base).map_err(|e| format!("log: {e}"))?;
        let diff = crate::git::diff_full_since(sources.root, sources.base)
            .map_err(|e| format!("diff: {e}"))?;
        let text = format!("## commit log\n{log}\n\n## diff\n{diff}");
        let (text, _) = redact_secrets(sources.root, sources.config_root, &text)
            .map_err(|reason| format!("context: {reason}"))?;
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir() // osf: temp-dir allowed, one read-only folder per reviewer run
            .join(format!(
                "osf-review-change-{}-{unique}-{n}",
                std::process::id()
            ));
        std::fs::create_dir(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let folder = Self { dir };
        let file = folder.file();
        std::fs::write(&file, text).map_err(|e| format!("cannot write {}: {e}", file.display()))?;
        set_read_only(&file, true)?;
        set_read_only(&folder.dir, true)?;
        Ok(folder)
    }

    /// The folder's path.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The path of the diff and log file.
    #[must_use]
    pub fn file(&self) -> PathBuf {
        self.dir.join(CHANGE_FILE_NAME)
    }
}

impl Drop for ChangeFolder {
    fn drop(&mut self) {
        let _ = set_read_only(&self.dir, false);
        let _ = set_read_only(&self.file(), false);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Turns `path`'s write permission off or on.
fn set_read_only(path: &Path, read_only: bool) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = match (path.is_dir(), read_only) {
            (true, true) => 0o555,
            (true, false) => 0o700,
            (false, true) => 0o444,
            (false, false) => 0o600,
        };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .map_err(|e| format!("cannot protect {}: {e}", path.display()))
    }
    #[cfg(not(unix))]
    {
        let mut permissions = std::fs::metadata(path)
            .map_err(|e| format!("cannot protect {}: {e}", path.display()))?
            .permissions();
        permissions.set_readonly(read_only);
        std::fs::set_permissions(path, permissions)
            .map_err(|e| format!("cannot protect {}: {e}", path.display()))
    }
}

/// `path`, rendered with forward slashes whatever the platform's own separator is.
fn forward_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn commits_section(sources: &Sources) -> Result<String, String> {
    let base = crate::git::resolve_rev(sources.root, sources.base).map_err(|e| e.to_string())?;
    let head = crate::git::head_sha(sources.root).map_err(|e| e.to_string())?;
    Ok(format!("## commits\nbase: {base}\nhead: {head}"))
}

fn changed_files_section(sources: &Sources) -> Result<String, String> {
    let files =
        crate::git::numstat_since(sources.root, sources.base).map_err(|e| format!("diff: {e}"))?;
    if files.is_empty() {
        return Err("diff: no changed files against the base".to_string());
    }
    let mut out = String::from("## changed files (added, removed)\n");
    let lines: Vec<String> = files
        .iter()
        .map(|f| {
            format!(
                "{} (+{} -{})",
                f.path.replace('\\', "/"),
                f.added,
                f.removed
            )
        })
        .collect();
    out.push_str(&lines.join("\n"));
    Ok(out)
}

// --- the work item and its acceptance criteria -----------------------------

/// The work item's saved text, from the file the caller named. A `.json`
/// file is the form `osf review work-item` saves, which holds either the
/// issue's text or the reason the pull request has none. Any other file is
/// the work item's text itself.
fn read_work_item(sources: &Sources) -> Result<String, String> {
    let path = sources.work_item.ok_or_else(|| {
        "no work item file was given: pass --work-item, or link an issue on the pull request's Issue line"
            .to_string()
    })?;
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("{}: cannot be read: {e}", forward_slash(path)))?;
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        if let Some(binding) = sources.work_item_binding {
            let body = sources.pull_request.and_then(|pr| pr.body.as_deref());
            crate::work_item::check_binding(&text, binding.repository, binding.head, body)?;
        }
        return crate::work_item::read_saved(&text);
    }
    Ok(text)
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

/// `body` with code, comments and quotes blanked, for heading detection.
fn visible_lines(body: &str) -> Vec<String> {
    crate::work_item::prose(body)
        .lines()
        .map(str::to_string)
        .collect()
}

/// Whether `body` holds an acceptance heading outside a code block.
fn acceptance_heading_present(body: &str) -> bool {
    visible_lines(body)
        .iter()
        .any(|line| is_acceptance_heading(line))
}

/// The acceptance heading in `body` and everything under it, up to the next
/// heading at the same or a shallower level. `None` when there is no such
/// heading, or when the section holds no text.
fn acceptance_section_text(body: &str) -> Option<String> {
    let lines = visible_lines(body);
    let start = lines.iter().position(|line| is_acceptance_heading(line))?;
    let level = heading_level(lines.get(start)?)?;
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| heading_level(line).is_some_and(|found| found <= level))
        .map_or(lines.len(), |(i, _)| i);
    let section = lines.get(start..end)?;
    let has_text = section.iter().skip(1).any(|line| !line.trim().is_empty());
    has_text.then(|| section.join("\n"))
}

// --- decision records ------------------------------------------------------

/// The paths of the decision records the change touches or links, each
/// marked when this checkout has no such file.
fn decision_records_section(sources: &Sources) -> Result<String, String> {
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
    let lines: Vec<String> = refs
        .iter()
        .map(|path| {
            if crate::git::to_local_path(sources.root, path).is_file() {
                path.clone()
            } else {
                format!("{path} (not found in this checkout)")
            }
        })
        .collect();
    Ok(format!("## decision records\n{}", lines.join("\n")))
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

// --- redaction -----------------------------------------------------------

/// The `[scan]` table of `config_root`'s own `osf.toml`, or the compiled
/// defaults when the file, the table, or a field in it is missing or
/// malformed: a lens's context is always redacted, whether or not a
/// repository has configured anything extra for it to catch.
fn scan_config(config_root: &Path) -> ScanConfig {
    let Ok(text) = std::fs::read_to_string(config_root.join("osf.toml")) else {
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
/// naming the rule, and how many lines were replaced. A built-in rule,
/// `scan-secret`, covers well-known token shapes on its own, with no
/// configuration needed; the same rules that stop a secret, a session
/// link or a local path reaching a public repository stop it reaching a
/// reviewer's prompt first. A match with no excerpt of its own (a denied
/// name, a secret shape, or a private-key line) has its whole line
/// replaced; every other rule replaces only the text it matched.
///
/// Crate-visible so every other place a reviewer's own text leaves osf (a
/// verified finding's quote and body, a printed line, a later posted
/// review) redacts it the same way a prompt is redacted here, through this
/// one function.
///
/// The `[scan]` settings themselves come from `config_root`, never `root`:
/// a pull request under review must not be able to turn a rule off in its
/// own `osf.toml` and have that loosen what a reviewer's prompt gets to
/// see. `root` still names the repository the rules run against, for a
/// rule such as `scan-foreign-reference` that reads the repository's own
/// git remote when `config_root` names no owner.
///
/// # Errors
/// Returns an error if a configured denylist pattern or session link prefix
/// is not valid: a context this module cannot trust to be scanned must
/// never be sent unredacted instead.
pub(crate) fn redact_secrets(
    root: &Path,
    config_root: &Path,
    text: &str,
) -> Result<(String, usize), String> {
    let cfg = scan_config(config_root);
    let rules = crate::scan::Rules::build(root, &cfg)?;
    let findings = rules.scan_text(text, osf_lint_core::Context::Document);
    if findings.is_empty() {
        return Ok((text.to_string(), 0));
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
    Ok((lines.join("\n"), findings.len()))
}

/// Appends a one-line note naming how many lines were redacted, or leaves
/// `text` untouched when nothing was.
fn append_redaction_note(mut text: String, count: usize) -> String {
    if count > 0 {
        let _ = write!(
            text,
            "\n\n[{count} line(s) redacted before this prompt was built]"
        );
    }
    text
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
    fn acceptance_section_is_none_when_the_body_ends_with_the_heading() {
        assert!(acceptance_section_text("# Title\n\n## Done when").is_none());
    }

    #[test]
    fn acceptance_section_is_none_when_only_blank_lines_follow_the_heading() {
        assert!(acceptance_section_text("## Done when\n\n\n   \n").is_none());
    }

    #[test]
    fn acceptance_section_is_none_when_only_an_html_comment_follows_the_heading() {
        let body = "## Done when\n<!-- nothing here yet -->\n";
        assert!(acceptance_section_text(body).is_none());
    }

    #[test]
    fn acceptance_section_is_none_when_the_next_heading_follows_at_once() {
        let body = "## Done when\n## Notes\nsome notes\n";
        assert!(acceptance_section_text(body).is_none());
    }

    #[test]
    fn a_heading_inside_a_code_fence_is_not_an_acceptance_heading() {
        let body = "```\n## Done when\n- not a section\n```\n";
        assert!(acceptance_section_text(body).is_none());
        assert!(!acceptance_heading_present(body));
    }

    #[test]
    fn acceptance_section_with_one_list_item_still_works() {
        let section = acceptance_section_text("## Done when\n- one item\n").expect("section");
        assert!(section.contains("- one item"), "{section}");
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
    fn append_redaction_note_is_a_no_op_at_zero() {
        let text = "unchanged".to_string();
        assert_eq!(append_redaction_note(text.clone(), 0), text);
    }

    #[test]
    fn append_redaction_note_names_the_count() {
        let text = append_redaction_note("body".to_string(), 3);
        assert!(text.contains("3 line"), "{text}");
    }
}
