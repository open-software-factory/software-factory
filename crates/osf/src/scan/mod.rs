//! `osf scan`: text that must never reach a public repository. Each check
//! is its own rule, with a class, a group and doc text, resolved and
//! explained the same way as every writing and skill rule.
//!
//! A scan rule reads raw text, not prose: it never parses Markdown, and it
//! runs the same way over a source file, a plan, or a commit message.
//!
//! No rule here names one coding agent or one operating system on its own.
//! Agents come from [`crate::agents::AGENTS`], and path shapes from a table
//! that covers every platform this project runs on. A rule that cannot run
//! says so through [`Rules::notes`] rather than staying quiet.

mod meta;

pub use meta::rule_meta;

use crate::agents::AGENTS;
use crate::config::ScanConfig;
use crate::exclude::Excluder;
use osf_lint_core::{resolve, Context, Finding, Level};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Every file `scan` looked at: the findings for each one kept, and how
/// many the exclude list dropped before they were even read.
#[derive(Debug)]
pub struct ScanOutcome {
    pub files: Vec<(String, Vec<Finding>)>,
    pub excluded: usize,
}

fn lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines().enumerate().map(|(i, l)| (i + 1, l))
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("scan rule pattern compiles"))
}

fn finding(rule: &'static str, line: usize, message: String, excerpt: &str) -> Finding {
    Finding::new(rule, Level::Error, line, message, excerpt.to_string())
}

/// The characters a link or an id may run on to, once its prefix matched.
const LINK_TAIL: &str = r#"[^\s)>\]"'`]*"#;

/// One hosted-session link shape and whose it is.
struct SessionLink {
    agent: String,
    pattern: Regex,
}

fn session_link_pattern(prefix: &str) -> Result<Regex, String> {
    Regex::new(&format!(
        r"(?:https?://)?{}{LINK_TAIL}",
        regex::escape(prefix)
    ))
    .map_err(|_| format!("session link prefix '{prefix}' does not compile"))
}

fn session_links(cfg: &ScanConfig) -> Result<Vec<SessionLink>, String> {
    let mut out = Vec::new();
    for agent in AGENTS {
        if let Some(prefix) = agent.session_link_prefix() {
            out.push(SessionLink {
                agent: agent.name.to_string(),
                pattern: session_link_pattern(&prefix)?,
            });
        }
    }
    for prefix in &cfg.session_links {
        out.push(SessionLink {
            agent: format!("a configured agent ({prefix})"),
            pattern: session_link_pattern(prefix)?,
        });
    }
    Ok(out)
}

fn session_link_findings(links: &[SessionLink], text: &str, out: &mut Vec<Finding>) {
    for (line, content) in lines(text) {
        for link in links {
            for m in link.pattern.find_iter(content) {
                out.push(finding(
                    "scan-session-link",
                    line,
                    format!(
                        "a session link for {} must never reach a public repository",
                        link.agent
                    ),
                    m.as_str(),
                ));
            }
        }
    }
}

/// Segments under an agent's state directory that hold conversations.
const STATE_SEGMENTS: &[&str] = &["sessions", "projects", "transcripts", "history", "logs"];

/// One agent's state-path shape: any of its state directories followed by
/// a conversation segment.
struct StatePath {
    agent: &'static str,
    pattern: Regex,
}

fn state_paths() -> &'static [StatePath] {
    static CELL: OnceLock<Vec<StatePath>> = OnceLock::new();
    CELL.get_or_init(|| {
        AGENTS
            .iter()
            .map(|agent| {
                let dirs = agent
                    .state_dirs
                    .iter()
                    .map(|d| regex::escape(d).replace('/', r"[\\/]"))
                    .collect::<Vec<_>>()
                    .join("|");
                let segments = STATE_SEGMENTS.join("|");
                let pattern = format!(r"(?:^|[\\/~\s])(?:{dirs})[\\/](?:{segments})[\\/]\S*");
                StatePath {
                    agent: agent.name,
                    pattern: Regex::new(&pattern).expect("state path pattern compiles"),
                }
            })
            .collect()
    })
}

fn agent_state_path_findings(text: &str, out: &mut Vec<Finding>) {
    for (line, content) in lines(text) {
        for shape in state_paths() {
            for m in shape.pattern.find_iter(content) {
                out.push(finding(
                    "scan-agent-state-path",
                    line,
                    format!(
                        "a path into {}'s own state must never reach a public repository",
                        shape.agent
                    ),
                    m.as_str().trim(),
                ));
            }
        }
    }
}

fn coauthor_trailer_findings(text: &str, out: &mut Vec<Finding>) {
    for (line, content) in lines(text) {
        if content.trim_start().starts_with("Co-Authored-By:") {
            out.push(finding(
                "scan-coauthor-trailer",
                line,
                "a co-author trailer must never reach a public repository".to_string(),
                content.trim(),
            ));
        }
    }
}

/// The platforms whose user-path shapes the local-path rule knows, in the
/// order the combined pattern tries them. Earlier entries win where two
/// overlap, so a drive path is reported once, as Windows, not again as the
/// home-directory shape it also contains.
const LOCAL_PATH_PLATFORMS: &[&str] = &[
    "Windows",
    "a Windows network share",
    "Windows Subsystem for Linux",
    "Linux",
    "Linux, as the root account",
    "macOS",
];

/// Built at run time from parts. This project scans its own source, and a
/// shape written out in full here would match itself.
fn local_path_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let acct = r"[^/\\\s]+";
        let sep = r"[\\/]";
        let users = "Users";
        // A share needs a server, a share name, and a path under them. Two
        // bare segments would also match an escape sequence in source code.
        let name = r"[A-Za-z0-9._$-]+";
        let shapes = [
            format!(r"[A-Za-z]:{sep}{users}{sep}{acct}{sep}"),
            format!(r"\\\\{name}\\{name}\\"),
            format!("/mnt/[A-Za-z]/{users}/{acct}/"),
            format!("/{}/{acct}/", "home"),
            format!("/{}/", "root"),
            format!("/{users}/{acct}/"),
        ];
        let alternation = shapes
            .iter()
            .enumerate()
            .map(|(i, s)| format!("(?P<p{i}>{s})"))
            .collect::<Vec<_>>()
            .join("|");
        Regex::new(&alternation).expect("local path pattern compiles")
    })
}

fn local_path_findings(text: &str, out: &mut Vec<Finding>) {
    let pattern = local_path_regex();
    for (line, content) in lines(text) {
        for cap in pattern.captures_iter(content) {
            let Some(whole) = cap.get(0) else { continue };
            let platform = LOCAL_PATH_PLATFORMS
                .iter()
                .enumerate()
                .find(|(i, _)| cap.name(&format!("p{i}")).is_some())
                .map_or("an unknown platform", |(_, name)| name);
            out.push(finding(
                "scan-local-path",
                line,
                format!(
                    "a user path from {platform} names a machine or an account and must never reach a public repository"
                ),
                whole.as_str(),
            ));
        }
    }
}

fn foreign_reference_findings(owner: &str, text: &str, out: &mut Vec<Finding>) {
    static RE: OnceLock<Regex> = OnceLock::new();
    let pattern = re(&RE, r"\b([A-Za-z0-9][\w.-]*)/[A-Za-z0-9][\w.-]*#\d+\b");
    for (line, content) in lines(text) {
        for cap in pattern.captures_iter(content) {
            let (Some(found), Some(whole)) = (cap.get(1), cap.get(0)) else {
                continue;
            };
            if found.as_str().eq_ignore_ascii_case(owner) {
                continue;
            }
            out.push(finding(
                "scan-foreign-reference",
                line,
                format!(
                    "'{}' is not this project's owner '{owner}'; check the reference is meant",
                    found.as_str()
                ),
                whole.as_str(),
            ));
        }
    }
}

/// A compiled denylist: patterns that must never appear in a public
/// repository, and never appear in a finding either. `None` when the
/// configured list is empty, so the rule never fires.
struct Denylist(Option<Regex>);

impl Denylist {
    fn compile(patterns: &[String]) -> Result<Self, String> {
        if patterns.is_empty() {
            return Ok(Denylist(None));
        }
        let joined = patterns
            .iter()
            .map(|p| format!("(?:{p})"))
            .collect::<Vec<_>>()
            .join("|");
        Regex::new(&format!("(?i){joined}"))
            .map(Some)
            .map(Denylist)
            .map_err(|_| "the configured denylist pattern is not valid regex".to_string())
    }

    /// Never records the matched text, or the pattern: only the line it
    /// matched on. Printing either would leak the very thing this rule
    /// protects.
    fn find(&self, text: &str, out: &mut Vec<Finding>) {
        let Some(pattern) = &self.0 else { return };
        for (line, content) in lines(text) {
            if pattern.is_match(content) {
                out.push(finding(
                    "scan-denied-name",
                    line,
                    "a denylisted name must never reach a public repository".to_string(),
                    "",
                ));
            }
        }
    }
}

/// Every scan rule, built once from the resolved configuration.
pub struct Rules {
    owner: Option<String>,
    links: Vec<SessionLink>,
    denylist: Denylist,
    notes: Vec<String>,
}

impl Rules {
    /// Rules with the project owner taken from `cfg` alone. With no owner
    /// configured, the foreign-reference rule does not run, and a note
    /// says so. Prefer [`Rules::build_for`] where a repository is at hand.
    ///
    /// # Errors
    /// Returns an error if a configured denylist pattern or session link prefix is not valid.
    pub fn build(cfg: &ScanConfig) -> Result<Self, String> {
        let mut rules = Self::assemble(cfg)?;
        if cfg.project_owner.is_empty() {
            rules.notes.push(
                "scan-foreign-reference did not run: no project owner is configured and no repository was given to read one from".to_string(),
            );
        } else {
            rules.owner = Some(cfg.project_owner.clone());
        }
        Ok(rules)
    }

    /// Rules with the project owner taken from `cfg`, or, when that is
    /// empty, from the `origin` remote of the repository at `dir`. When
    /// neither yields an owner, the foreign-reference rule does not run and
    /// a note says why.
    ///
    /// # Errors
    /// Returns an error if a configured denylist pattern or session link prefix is not valid.
    pub fn build_for(dir: &Path, cfg: &ScanConfig) -> Result<Self, String> {
        let mut rules = Self::assemble(cfg)?;
        if !cfg.project_owner.is_empty() {
            rules.owner = Some(cfg.project_owner.clone());
            return Ok(rules);
        }
        match crate::git::remote_owner(dir) {
            Ok(owner) => rules.owner = Some(owner),
            Err(e) => rules.notes.push(format!(
                "scan-foreign-reference did not run: no project owner is configured and the git remote gave none ({e})"
            )),
        }
        Ok(rules)
    }

    fn assemble(cfg: &ScanConfig) -> Result<Self, String> {
        Ok(Rules {
            owner: None,
            links: session_links(cfg)?,
            denylist: Denylist::compile(&cfg.denylist)?,
            notes: Vec::new(),
        })
    }

    /// The owner every `owner/repo#N` reference is compared against, when
    /// one was found.
    #[must_use]
    pub fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }

    /// What did not run, and why. Empty when every rule ran.
    #[must_use]
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// Every finding in `text`, resolved and explained under `context`.
    #[must_use]
    pub fn scan_text(&self, text: &str, context: Context) -> Vec<Finding> {
        let mut out = Vec::new();
        session_link_findings(&self.links, text, &mut out);
        agent_state_path_findings(text, &mut out);
        coauthor_trailer_findings(text, &mut out);
        local_path_findings(text, &mut out);
        if let Some(owner) = &self.owner {
            foreign_reference_findings(owner, text, &mut out);
        }
        self.denylist.find(text, &mut out);
        resolve_and_explain(&mut out, context);
        osf_lint_core::sort_findings(&mut out);
        out
    }
}

/// Sets each finding's level and remediation from its rule's class and
/// group, resolved against `context`, and points the message at
/// `osf explain <rule-id>`.
fn resolve_and_explain(findings: &mut [Finding], context: Context) {
    for f in findings.iter_mut() {
        let Some(meta) = rule_meta(f.rule) else {
            continue;
        };
        let (level, remediation) = resolve(meta.class, meta.group, context, meta.exception);
        f.level = level;
        f.remediation = remediation;
        f.message = format!("{} (see `osf explain {}`)", f.message, f.rule);
    }
}

/// A file is binary, not text, when a NUL byte turns up early in it: the
/// same heuristic git itself uses.
#[must_use]
pub fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|&b| b == 0)
}

fn collect_files(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let meta =
        std::fs::metadata(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if !meta.is_dir() {
        out.push(path.to_path_buf());
        return Ok(());
    }
    let entries =
        std::fs::read_dir(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let child = entry.path();
        if child.file_name().is_some_and(|n| n == ".git") {
            continue;
        }
        collect_files(&child, out)?;
    }
    Ok(())
}

/// Files to scan: a display label, and the real path to read from disk.
/// With no paths, every file git tracks in `dir`, labelled by its
/// git-relative name; otherwise the given paths, walking any directory
/// among them, each labelled as given.
fn resolve_scan_targets(dir: &Path, paths: &[PathBuf]) -> Result<Vec<(String, PathBuf)>, String> {
    if paths.is_empty() {
        let tracked = crate::git::tracked_files(dir, None).map_err(|e| e.to_string())?;
        return Ok(tracked
            .into_iter()
            .map(|p| {
                let full = crate::git::to_local_path(dir, &p);
                (p, full)
            })
            .collect());
    }
    let mut files = Vec::new();
    for path in paths {
        collect_files(path, &mut files)?;
    }
    Ok(files
        .into_iter()
        .map(|f| (f.to_string_lossy().replace('\\', "/"), f))
        .collect())
}

/// Scans every target file, named relative to `dir` when it came from git,
/// or as given on the command line otherwise. A binary file is skipped. A
/// file matching `excluder` is never even read.
///
/// # Errors
/// Returns an error if git cannot run, or a named path cannot be read.
pub fn scan_paths(
    dir: &Path,
    paths: &[PathBuf],
    rules: &Rules,
    excluder: &Excluder,
) -> Result<ScanOutcome, String> {
    let targets = resolve_scan_targets(dir, paths)?;
    let mut files = Vec::new();
    let mut dropped = 0usize;
    for (label, full) in targets {
        if excluder.is_excluded(&label) {
            dropped += 1;
            continue;
        }
        let bytes =
            std::fs::read(&full).map_err(|e| format!("cannot read {}: {e}", full.display()))?;
        if is_binary(&bytes) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let findings = rules.scan_text(&text, Context::Document);
        files.push((label, findings));
    }
    Ok(ScanOutcome {
        files,
        excluded: dropped,
    })
}

/// Scans every commit message in `range`, named by its commit hash.
///
/// # Errors
/// Returns an error if git cannot run in `dir`, such as when `range` does not resolve.
pub fn scan_commits(
    dir: &Path,
    range: &str,
    rules: &Rules,
) -> Result<Vec<(String, Vec<Finding>)>, String> {
    let hashes = crate::git::commit_hashes(dir, range).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for hash in hashes {
        let message = crate::git::commit_message(dir, &hash).map_err(|e| e.to_string())?;
        let findings = rules.scan_text(&message, Context::Commit);
        out.push((hash, findings));
    }
    Ok(out)
}
