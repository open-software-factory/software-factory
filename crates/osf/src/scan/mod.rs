//! `osf scan`: text that must never reach a repository the public can
//! read. Each check is its own rule, with a class, a group and doc text,
//! resolved and explained the same way as every writing and skill rule.
//!
//! A scan rule reads raw text, not prose: it never parses Markdown, and it
//! runs the same way over a source file, a plan, or a commit message.
//!
//! Three things every rule here shares. No rule names one coding agent or
//! one operating system on its own: agents come from
//! [`crate::agents::AGENTS`]. No rule assumes anything about the repository:
//! [`crate::repository::resolve`] establishes who owns it first, and every
//! rule applies whether it is public or private. And a link is parsed as a
//! URL and a path as a path, by a library, before anything is compared; a
//! regular expression only finds candidates in free text.

mod meta;

pub use meta::{rule_ids, rule_meta};

use crate::agents::AGENTS;
use crate::config::ScanConfig;
use crate::exclude::Excluder;
use crate::repository::{self, Repository};
use osf_lint_core::{apply_suppressions, resolve, Context, Finding, Level};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use typed_path::{
    Utf8UnixComponent, Utf8UnixPath, Utf8WindowsComponent, Utf8WindowsPath, Utf8WindowsPrefix,
};
use url::Url;

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

/// Characters that end a link or a path pasted into prose.
const TOKEN_END: &str = r#"[^\s<>"'`()\[\]]"#;

/// Trailing punctuation that belongs to the sentence, not the token.
fn trim_token(token: &str) -> &str {
    token.trim_end_matches(['.', ',', ';', ':', '!', '?'])
}

// --- session links --------------------------------------------------------

/// One hosted-session link shape and whose it is.
struct SessionLink {
    agent: String,
    host: String,
    path: String,
}

fn session_links(cfg: &ScanConfig) -> Result<Vec<SessionLink>, String> {
    let mut out = Vec::new();
    for agent in AGENTS {
        if let Some((host, path)) = agent.hosted() {
            out.push(SessionLink {
                agent: agent.name.to_string(),
                host: host.to_ascii_lowercase(),
                path: path.to_string(),
            });
        }
    }
    for prefix in &cfg.session_links {
        let (host, path) = prefix.split_once('/').ok_or_else(|| {
            format!("session link prefix '{prefix}' needs a host, a slash, and a path")
        })?;
        if host.is_empty() || path.is_empty() {
            return Err(format!(
                "session link prefix '{prefix}' needs a host, a slash, and a path"
            ));
        }
        out.push(SessionLink {
            agent: format!("a configured agent ({prefix})"),
            host: host.to_ascii_lowercase(),
            path: format!("/{path}"),
        });
    }
    Ok(out)
}

/// Anything in free text that looks like a web address, with or without
/// its scheme. Parsing decides what it really is.
fn url_candidates(content: &str) -> impl Iterator<Item = &str> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let pattern = re(
        &RE,
        &format!(
            r"(?:[A-Za-z][A-Za-z0-9+.-]*://)?[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+(?::\d+)?(?:/{TOKEN_END}*)?"
        ),
    );
    pattern.find_iter(content).map(|m| trim_token(m.as_str()))
}

/// The host and path of `token`, read by a URL parser. A token with no
/// scheme is read as if it had one, since that is how people paste them.
fn parse_web_url(token: &str) -> Option<Url> {
    let with_scheme = if token.contains("://") {
        token.to_string()
    } else {
        format!("https://{token}")
    };
    let url = Url::parse(&with_scheme).ok()?;
    url.host_str()?;
    Some(url)
}

fn session_link_findings(links: &[SessionLink], clause: &str, text: &str, out: &mut Vec<Finding>) {
    for (line, content) in lines(text) {
        for token in url_candidates(content) {
            let Some(url) = parse_web_url(token) else {
                continue;
            };
            let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
            for link in links {
                if host == link.host && url.path().starts_with(&link.path) {
                    out.push(finding(
                        "scan-session-link",
                        line,
                        format!("a session link for {} {clause}", link.agent),
                        token,
                    ));
                }
            }
        }
    }
}

// --- agent state paths ----------------------------------------------------

/// One agent's state-path shape: any of its state directories followed by
/// one of its session paths.
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
                let alt = |items: &[&str]| {
                    items
                        .iter()
                        .map(|d| regex::escape(d).replace('/', r"[\\/]"))
                        .collect::<Vec<_>>()
                        .join("|")
                };
                let dirs = alt(agent.state_dirs);
                let paths = alt(agent.session_paths);
                let pattern = format!(r"(?:^|[\\/])(?:{dirs})[\\/](?:{paths})(?:[\\/]|$)");
                StatePath {
                    agent: agent.name,
                    pattern: Regex::new(&pattern).expect("state path pattern compiles"),
                }
            })
            .collect()
    })
}

fn agent_state_path_findings(clause: &str, text: &str, out: &mut Vec<Finding>) {
    for (line, content) in lines(text) {
        for token in path_candidates(content) {
            for shape in state_paths() {
                if shape.pattern.is_match(token) {
                    out.push(finding(
                        "scan-agent-state-path",
                        line,
                        format!("a path into {}'s own state {clause}", shape.agent),
                        token,
                    ));
                }
            }
        }
    }
}

// --- the trailer ----------------------------------------------------------

fn coauthor_trailer_findings(clause: &str, text: &str, out: &mut Vec<Finding>) {
    for (line, content) in lines(text) {
        if content.trim_start().starts_with("Co-Authored-By:") {
            out.push(finding(
                "scan-coauthor-trailer",
                line,
                format!("a co-author trailer {clause}"),
                content.trim(),
            ));
        }
    }
}

// --- local paths ----------------------------------------------------------

/// A path that names a real machine or account, and on which platform.
#[derive(Debug, PartialEq, Eq)]
enum UserPath {
    Windows,
    NetworkShare,
    WindowsSubsystemForLinux,
    Linux,
    LinuxRoot,
    MacOs,
}

impl UserPath {
    fn platform(&self) -> &'static str {
        match self {
            UserPath::Windows => "Windows",
            UserPath::NetworkShare => "a Windows network share",
            UserPath::WindowsSubsystemForLinux => "Windows Subsystem for Linux",
            UserPath::Linux => "Linux",
            UserPath::LinuxRoot => "Linux, as the root account",
            UserPath::MacOs => "macOS",
        }
    }
}

/// Reads `token` as a Windows path, then as a Unix path, and says whether
/// either names a user's home or a network share. The folder names that
/// mark a home are kept apart from any slash, so this source never holds
/// the shape it looks for.
fn classify_path(token: &str) -> Option<UserPath> {
    let users = "Users";
    let home = "home";
    let root = "root";
    let mount = "mnt";

    let win = Utf8WindowsPath::new(token);
    if win.is_absolute()
        || win
            .components()
            .next()
            .is_some_and(|c| matches!(c, Utf8WindowsComponent::Prefix(_)))
    {
        let mut parts = win.components();
        let prefix = match parts.next() {
            Some(Utf8WindowsComponent::Prefix(p)) => p.kind(),
            _ => return None,
        };
        let normals: Vec<&str> = parts
            .filter_map(|c| match c {
                Utf8WindowsComponent::Normal(s) => Some(s),
                _ => None,
            })
            .collect();
        // A share names a server, a share, and something under them. Two
        // backslashes and one word is an escape sequence, not a share.
        if let Utf8WindowsPrefix::UNC(server, share)
        | Utf8WindowsPrefix::VerbatimUNC(server, share) = prefix
        {
            let named = !server.is_empty() && !share.is_empty() && !normals.is_empty();
            return named.then_some(UserPath::NetworkShare);
        }
        let under_users = normals
            .first()
            .is_some_and(|first| first.eq_ignore_ascii_case(users));
        if under_users && normals.len() >= 2 {
            return Some(UserPath::Windows);
        }
        return None;
    }

    let unix = Utf8UnixPath::new(token);
    if !unix.is_absolute() {
        return None;
    }
    let normals: Vec<&str> = unix
        .components()
        .filter_map(|c| match c {
            Utf8UnixComponent::Normal(s) => Some(s),
            _ => None,
        })
        .collect();
    match normals.as_slice() {
        [first, _, ..] if *first == home => Some(UserPath::Linux),
        [first, _, ..] if *first == users => Some(UserPath::MacOs),
        [first, ..] if *first == root => Some(UserPath::LinuxRoot),
        [first, drive, second, _, ..]
            if *first == mount && drive.len() == 1 && *second == users =>
        {
            Some(UserPath::WindowsSubsystemForLinux)
        }
        _ => None,
    }
}

/// Anything in free text that could be a path: a run of non-space
/// characters holding a slash or a backslash.
fn path_candidates(content: &str) -> impl Iterator<Item = &str> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let pattern = re(&RE, &format!(r"{TOKEN_END}*[\\/]{TOKEN_END}*"));
    pattern.find_iter(content).map(|m| trim_token(m.as_str()))
}

/// A `file:` URL, read by the URL parser, as the path it names. A host in
/// the URL is a network share.
fn file_url_path(token: &str) -> Option<String> {
    let url = Url::parse(token).ok()?;
    if url.scheme() != "file" {
        return None;
    }
    let path = url.path();
    match url.host_str() {
        Some(host) if !host.is_empty() => Some(format!(r"\\{host}{}", path.replace('/', r"\"))),
        _ => {
            // `/C:/Users/...` is how a drive path appears inside a file URL.
            let stripped = path.strip_prefix('/').unwrap_or(path);
            if stripped.as_bytes().get(1) == Some(&b':') {
                Some(stripped.to_string())
            } else {
                Some(path.to_string())
            }
        }
    }
}

fn local_path_findings(clause: &str, text: &str, out: &mut Vec<Finding>) {
    for (line, content) in lines(text) {
        for token in path_candidates(content) {
            let classified = if token.starts_with("file:") {
                file_url_path(token).and_then(|p| classify_path(&p))
            } else {
                classify_path(token)
            };
            if let Some(kind) = classified {
                out.push(finding(
                    "scan-local-path",
                    line,
                    format!(
                        "a user path from {} names a machine or an account and {clause}",
                        kind.platform()
                    ),
                    token,
                ));
            }
        }
    }
}

// --- foreign references ---------------------------------------------------

fn foreign_reference_findings(owner: &str, clause: &str, text: &str, out: &mut Vec<Finding>) {
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
                    "'{}' is not this project's owner '{owner}', and a reference to it {clause}",
                    found.as_str()
                ),
                whole.as_str(),
            ));
        }
    }
}

// --- secrets ---------------------------------------------------------------

/// Well-known cloud, forge and provider token shapes, and an assignment of
/// a literal to an upper-snake-case name ending in `KEY`, `TOKEN`, `SECRET`
/// or `PASSWORD`. The name shape is deliberately narrow: it excludes an
/// ordinary lower-case identifier such as `cache_key` or `sort_key`, and a
/// whole word that merely ends in one of these strings, such as `MONKEY`.
fn secret_shape_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(
        &RE,
        concat!(
            r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b",
            r"|\bgh[pousr]_[A-Za-z0-9]{36,}\b",
            r"|\bgithub_pat_[A-Za-z0-9_]{20,}\b",
            r"|\bglpat-[A-Za-z0-9_\-]{20,}\b",
            r"|\bsk-ant-[A-Za-z0-9_\-]{20,}\b",
            r"|\bsk-[A-Za-z0-9]{20,}\b",
            r"|\bxox[baprs]-[A-Za-z0-9\-]{10,}\b",
            r#"|\b(?:[A-Z][A-Z0-9]*_)*(?:KEY|TOKEN|SECRET|PASSWORD)\b\s*[:=]\s*(?:'[^'\s]{8,}'|"[^"\s]{8,}")"#,
        ),
    )
}

/// A PEM private-key block, its `BEGIN`/`END` lines and everything between
/// them, found over the whole text rather than one line at a time.
fn pem_key_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        regex::RegexBuilder::new(
            r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----",
        )
        .dot_matches_new_line(true)
        .build()
        .expect("PEM key pattern compiles")
    })
}

/// Never records the matched text: the same reason `scan-denied-name`
/// does not. A line this fires on is not something a reader needs to see
/// repeated back to them to know it must go.
fn secret_line_findings(clause: &str, text: &str, out: &mut Vec<Finding>) {
    let pattern = secret_shape_pattern();
    for (line, content) in lines(text) {
        if pattern.is_match(content) {
            out.push(finding(
                "scan-secret",
                line,
                format!("text shaped like a real secret {clause}"),
                "",
            ));
        }
    }
}

/// One finding per line of a PEM private-key block, so a caller that
/// redacts by line blanks the whole key, not only its `BEGIN` line.
fn pem_key_findings(clause: &str, text: &str, out: &mut Vec<Finding>) {
    for hit in pem_key_pattern().find_iter(text) {
        let start_line = text
            .bytes()
            .take(hit.start())
            .filter(|&b| b == b'\n')
            .count()
            + 1;
        let span_lines = hit.as_str().matches('\n').count() + 1;
        for offset in 0..span_lines {
            out.push(finding(
                "scan-secret",
                start_line + offset,
                format!("a private-key block {clause}"),
                "",
            ));
        }
    }
}

// --- the denylist ---------------------------------------------------------

/// A compiled denylist: patterns that must never appear in the repository,
/// and never appear in a finding either. `None` when the configured list
/// is empty, so the rule never fires.
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
                    "a denylisted name must never reach a repository, private or public"
                        .to_string(),
                    "",
                ));
            }
        }
    }
}

// --- the rules as a set ---------------------------------------------------

/// The clause every provenance message ends with. It does not say public:
/// none of this belongs in a private repository either.
const CLAUSE: &str = "must never reach a repository, private or public";

/// Every scan rule, built once for one repository.
pub struct Rules {
    repository: Repository,
    links: Vec<SessionLink>,
    denylist: Denylist,
    notes: Vec<String>,
}

impl Rules {
    /// Rules for the repository at `dir`. The owner comes from `cfg` where
    /// it states one, else from the git remote. Nothing in `cfg` is required. A fact that could not be
    /// read is reported through [`Rules::notes`].
    ///
    /// # Errors
    /// Returns an error if a configured denylist pattern or session link prefix is not valid.
    pub fn build(dir: &Path, cfg: &ScanConfig) -> Result<Self, String> {
        let resolved = repository::resolve(dir, cfg);
        Ok(Rules {
            repository: resolved.repository,
            links: session_links(cfg)?,
            denylist: Denylist::compile(&cfg.denylist)?,
            notes: resolved.notes,
        })
    }

    /// The repository these rules were built for.
    #[must_use]
    pub fn repository(&self) -> &Repository {
        &self.repository
    }

    /// The owner every `owner/repo#N` reference is compared against, when
    /// one was found.
    #[must_use]
    pub fn owner(&self) -> Option<&str> {
        self.repository.owner.as_deref()
    }

    /// What could not be established, and what that means for the run.
    /// Empty when everything was known.
    #[must_use]
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// Every finding in `text`, resolved and explained under `context`.
    #[must_use]
    pub fn scan_text(&self, text: &str, context: Context) -> Vec<Finding> {
        let clause = CLAUSE;
        let mut out = Vec::new();
        session_link_findings(&self.links, clause, text, &mut out);
        agent_state_path_findings(clause, text, &mut out);
        coauthor_trailer_findings(clause, text, &mut out);
        local_path_findings(clause, text, &mut out);
        if let Some(owner) = &self.repository.owner {
            foreign_reference_findings(owner, clause, text, &mut out);
        }
        secret_line_findings(clause, text, &mut out);
        pem_key_findings(clause, text, &mut out);
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
        let findings = apply_suppressions(&text, findings, &crate::lints::all_rule_ids());
        files.push((label, findings));
    }
    Ok(ScanOutcome {
        files,
        excluded: dropped,
    })
}

/// Scans every commit message in `range`, named by its commit hash. A
/// commit message has no file to carry a suppression marker, so a finding
/// here is never suppressible.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drive_path_is_read_as_windows_with_either_slash() {
        let back = format!("{}{}{}{}", "C:", r"\", "Users", r"\pat\x");
        let fwd = format!("{}{}{}{}", "C:", "/", "Users", "/pat/x");
        assert_eq!(classify_path(&back), Some(UserPath::Windows));
        assert_eq!(classify_path(&fwd), Some(UserPath::Windows));
    }

    #[test]
    fn a_program_folder_is_not_a_user_path() {
        let token = format!("{}{}{}", "C:", r"\", r"Program Files\x");
        assert_eq!(classify_path(&token), None);
    }

    #[test]
    fn relative_and_portable_forms_are_not_user_paths() {
        for token in [
            "crates/osf/src",
            "~/.osf/config.toml",
            "$HOME/.osf",
            "%USERPROFILE%\\x",
            "https://example.com/Users/guide/",
        ] {
            assert_eq!(classify_path(token), None, "{token}");
        }
    }

    #[test]
    fn a_file_url_yields_the_path_inside_it() {
        let win = format!("{}{}", "file:///", "C:/x/y");
        assert_eq!(file_url_path(&win).as_deref(), Some("C:/x/y"));
        let unix = format!("{}{}", "file://", "/x/y");
        assert_eq!(file_url_path(&unix).as_deref(), Some("/x/y"));
        let share = format!("{}{}", "file://", "server/share/y");
        assert!(file_url_path(&share).is_some_and(|p| p.starts_with(r"\\server")));
    }

    #[test]
    fn a_url_without_a_scheme_still_parses_to_its_host() {
        let url = parse_web_url("example.com/a/b").expect("parses");
        assert_eq!(url.host_str(), Some("example.com"));
        assert_eq!(url.path(), "/a/b");
    }
}
