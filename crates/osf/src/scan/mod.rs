//! `osf scan`: text that must never reach a public repository. Each check
//! is its own rule, with a class, a group and doc text, resolved and
//! explained the same way as every writing and skill rule.
//!
//! A scan rule reads raw text, not prose: it never parses Markdown, and it
//! runs the same way over a source file, a plan, or a commit message.

mod meta;

pub use meta::rule_meta;

use crate::config::ScanConfig;
use osf_lint_core::{resolve, Context, Finding, Level};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines().enumerate().map(|(i, l)| (i + 1, l))
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("scan rule pattern compiles"))
}

fn finding(rule: &'static str, line: usize, message: String, excerpt: &str) -> Finding {
    Finding::new(rule, Level::Error, line, message, excerpt.to_string())
}

fn session_link_findings(text: &str, out: &mut Vec<Finding>) {
    static RE: OnceLock<Regex> = OnceLock::new();
    let pattern = re(&RE, r"\S*claude\.ai/code/session_[A-Za-z0-9_-]*");
    for (line, content) in lines(text) {
        for m in pattern.find_iter(content) {
            out.push(finding(
                "scan-session-link",
                line,
                "a session link must never reach a public repository".to_string(),
                m.as_str(),
            ));
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

fn local_path_findings(text: &str, out: &mut Vec<Finding>) {
    static RE: OnceLock<Regex> = OnceLock::new();
    let pattern = re(&RE, r"[A-Za-z]:\\Users\\[^\s]*|/(?:home|Users)/[^/\s]+/");
    for (line, content) in lines(text) {
        for m in pattern.find_iter(content) {
            out.push(finding(
                "scan-local-path",
                line,
                "a local user path must never reach a public repository".to_string(),
                m.as_str(),
            ));
        }
    }
}

fn foreign_reference_findings(text: &str, cfg: &ScanConfig, out: &mut Vec<Finding>) {
    static RE: OnceLock<Regex> = OnceLock::new();
    if cfg.project_owner.is_empty() {
        return;
    }
    let pattern = re(&RE, r"\b([A-Za-z0-9][\w.-]*)/[A-Za-z0-9][\w.-]*#\d+\b");
    for (line, content) in lines(text) {
        for cap in pattern.captures_iter(content) {
            let (Some(owner), Some(whole)) = (cap.get(1), cap.get(0)) else {
                continue;
            };
            if owner.as_str().eq_ignore_ascii_case(&cfg.project_owner) {
                continue;
            }
            out.push(finding(
                "scan-foreign-reference",
                line,
                format!(
                    "'{}' is not this project's owner; check the reference is meant",
                    owner.as_str()
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
    cfg: ScanConfig,
    denylist: Denylist,
}

impl Rules {
    /// # Errors
    /// Returns an error if a configured denylist pattern is not valid regex.
    pub fn build(cfg: ScanConfig) -> Result<Self, String> {
        let denylist = Denylist::compile(&cfg.denylist)?;
        Ok(Rules { cfg, denylist })
    }

    /// Every finding in `text`, resolved and explained under `context`.
    #[must_use]
    pub fn scan_text(&self, text: &str, context: Context) -> Vec<Finding> {
        let mut out = Vec::new();
        session_link_findings(text, &mut out);
        coauthor_trailer_findings(text, &mut out);
        local_path_findings(text, &mut out);
        foreign_reference_findings(text, &self.cfg, &mut out);
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
/// or as given on the command line otherwise. A binary file is skipped.
///
/// # Errors
/// Returns an error if git cannot run, or a named path cannot be read.
pub fn scan_paths(
    dir: &Path,
    paths: &[PathBuf],
    rules: &Rules,
) -> Result<Vec<(String, Vec<Finding>)>, String> {
    let targets = resolve_scan_targets(dir, paths)?;
    let mut out = Vec::new();
    for (label, full) in targets {
        let bytes =
            std::fs::read(&full).map_err(|e| format!("cannot read {}: {e}", full.display()))?;
        if is_binary(&bytes) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let findings = rules.scan_text(&text, Context::Document);
        out.push((label, findings));
    }
    Ok(out)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Rules {
        Rules::build(ScanConfig::default()).expect("empty config builds")
    }

    fn rule_ids(findings: &[Finding]) -> Vec<&'static str> {
        findings.iter().map(|f| f.rule).collect()
    }

    #[test]
    fn a_session_link_fires() {
        let text = "See https://claude.ai/code/session_01AbCdEf for the discussion.\n";
        let found = rules().scan_text(text, Context::Document);
        assert_eq!(rule_ids(&found), vec!["scan-session-link"]);
        let f = found.first().expect("one finding");
        assert_eq!(f.level, Level::Error);
    }

    #[test]
    fn plain_text_has_no_session_link_finding() {
        let found = rules().scan_text("A plain sentence with no link at all.\n", Context::Document);
        assert!(rule_ids(&found).is_empty());
    }

    #[test]
    fn a_coauthor_trailer_fires() {
        let text = "Fix the bug.\n\nCo-Authored-By: Someone <someone@example.com>\n";
        let found = rules().scan_text(text, Context::Commit);
        assert_eq!(rule_ids(&found), vec!["scan-coauthor-trailer"]);
        let f = found.first().expect("one finding");
        assert_eq!(f.line, 3);
    }

    #[test]
    fn a_normal_commit_message_has_no_coauthor_finding() {
        let found = rules().scan_text("Fix the bug.\n", Context::Commit);
        assert!(rule_ids(&found).is_empty());
    }

    #[test]
    fn a_windows_user_path_fires() {
        let text = r"See D:\Users\pat\work\notes.md for the file.";
        let found = rules().scan_text(text, Context::Document);
        assert_eq!(rule_ids(&found), vec!["scan-local-path"]);
    }

    #[test]
    fn a_home_path_fires() {
        let found = rules().scan_text(
            "The file lives at /home/pat/work/notes.md.\n",
            Context::Document,
        );
        assert_eq!(rule_ids(&found), vec!["scan-local-path"]);
    }

    #[test]
    fn a_repository_relative_path_has_no_local_path_finding() {
        let found = rules().scan_text(
            "The file lives at crates/osf/src/scan/mod.rs.\n",
            Context::Document,
        );
        assert!(rule_ids(&found).is_empty());
    }

    #[test]
    fn a_foreign_reference_fires_when_an_owner_is_configured() {
        let cfg = ScanConfig {
            project_owner: "acme".to_string(),
            ..ScanConfig::default()
        };
        let rules = Rules::build(cfg).expect("config builds");
        let found = rules.scan_text("See other-org/tools#42 for the fix.\n", Context::Document);
        assert_eq!(rule_ids(&found), vec!["scan-foreign-reference"]);
    }

    #[test]
    fn a_reference_to_the_configured_owner_does_not_fire() {
        let cfg = ScanConfig {
            project_owner: "acme".to_string(),
            ..ScanConfig::default()
        };
        let rules = Rules::build(cfg).expect("config builds");
        let found = rules.scan_text("See acme/tools#42 for the fix.\n", Context::Document);
        assert!(rule_ids(&found).is_empty());
    }

    /// With no project owner configured, the rule must never fire: the
    /// tool has no way to tell a foreign reference from the project's own.
    #[test]
    fn foreign_reference_never_fires_with_no_project_owner_configured() {
        let found = rules().scan_text("See other-org/tools#42 for the fix.\n", Context::Document);
        assert!(rule_ids(&found).is_empty());
    }

    #[test]
    fn a_denylisted_name_fires_with_no_excerpt() {
        let cfg = ScanConfig {
            denylist: vec!["SecretCode".to_string()],
            ..ScanConfig::default()
        };
        let rules = Rules::build(cfg).expect("config builds");
        let found = rules.scan_text("The plan mentions SecretCode by name.\n", Context::Document);
        assert_eq!(rule_ids(&found), vec!["scan-denied-name"]);
        let f = found.first().expect("one finding");
        assert_eq!(f.excerpt, "");
    }

    #[test]
    fn text_with_no_denylisted_name_has_no_finding() {
        let cfg = ScanConfig {
            denylist: vec!["SecretCode".to_string()],
            ..ScanConfig::default()
        };
        let rules = Rules::build(cfg).expect("config builds");
        let found = rules.scan_text("Nothing sensitive here.\n", Context::Document);
        assert!(rule_ids(&found).is_empty());
    }

    /// The denylist match must never appear in the message, the excerpt, the
    /// rendered human line, the JSON line, or the SARIF report: every one of
    /// this crate's output formats.
    #[test]
    fn the_denylisted_text_never_appears_in_any_output_format() {
        let secret = "TotallyASecretOrgName";
        let cfg = ScanConfig {
            denylist: vec![secret.to_string()],
            ..ScanConfig::default()
        };
        let rules = Rules::build(cfg).expect("config builds");
        let text = format!("Some notes mention {secret} in passing.\n");
        let found = rules.scan_text(&text, Context::Document);
        assert_eq!(rule_ids(&found), vec!["scan-denied-name"]);
        let finding = found.first().expect("one finding").clone();

        assert!(!finding.excerpt.contains(secret));
        assert!(!finding.message.contains(secret));

        let rendered = finding.render("notes.txt", finding.level);
        assert!(!rendered.contains(secret), "{rendered}");

        let json = finding.to_json("notes.txt", finding.level);
        assert!(!json.contains(secret), "{json}");

        let tool = osf_lint_core::ToolInfo {
            name: "osf",
            version: "0.1.0",
            information_uri: "https://example.com",
        };
        let sarif = osf_lint_core::to_sarif(&[("notes.txt".to_string(), found)], &tool);
        let sarif_text = serde_json::to_string(&sarif).expect("sarif serialises");
        assert!(!sarif_text.contains(secret), "{sarif_text}");
    }

    #[test]
    fn every_scan_rule_id_has_metadata() {
        for id in [
            "scan-session-link",
            "scan-coauthor-trailer",
            "scan-local-path",
            "scan-foreign-reference",
            "scan-denied-name",
        ] {
            assert!(rule_meta(id).is_some(), "no metadata for {id}");
        }
    }

    #[test]
    fn every_finding_is_an_error_and_points_at_explain() {
        let cfg = ScanConfig {
            project_owner: "acme".to_string(),
            denylist: vec!["Secret".to_string()],
            ..ScanConfig::default()
        };
        let rules = Rules::build(cfg).expect("config builds");
        let text = "See https://claude.ai/code/session_1 and other/repo#1 and Secret and D:\\Users\\pat\\x and\nCo-Authored-By: X <x@example.com>\n";
        let found = rules.scan_text(text, Context::Commit);
        assert_eq!(found.len(), 5, "{found:?}");
        for f in &found {
            assert_eq!(f.level, Level::Error, "{}", f.rule);
            assert!(f.message.contains("osf explain"), "{}", f.message);
        }
    }

    #[test]
    fn a_binary_file_is_recognised() {
        assert!(is_binary(b"\x00\x01\x02"));
        assert!(!is_binary(b"plain text"));
    }
}
