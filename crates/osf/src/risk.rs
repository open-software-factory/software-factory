//! `osf risk`: the blast radius of a change, as one of three tiers with the
//! reasons behind the answer. Deterministic: the same change always gives
//! the same answer, on any machine, under any locale. No model is involved.

use regex::{Regex, RegexBuilder};
use std::collections::BTreeSet;
use std::path::Path;

/// How far a change can reach if it carries a defect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Low,
    Normal,
    High,
}

impl Tier {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Low => "low",
            Tier::Normal => "normal",
            Tier::High => "high",
        }
    }
}

/// A review axis a change earns on top of its tier, whatever the tier is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    UiProofAccessibility,
    DocumentsDesignAdrs,
    DependencyLicenceSupplyChain,
}

impl Axis {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Axis::UiProofAccessibility => "ui-proof-accessibility",
            Axis::DocumentsDesignAdrs => "documents-design-adrs",
            Axis::DependencyLicenceSupplyChain => "dependency-licence-supply-chain",
        }
    }
}

/// The tier a change earns, why, which extra axes it adds, and the counts
/// behind that answer.
#[derive(Debug)]
pub struct Report {
    pub tier: Tier,
    pub reasons: Vec<String>,
    pub axes_add: Vec<Axis>,
    pub files: usize,
    pub lines: usize,
    pub base: String,
    pub head: String,
    signal_list: Vec<String>,
}

impl Report {
    /// The review-lens trigger signals this change earns, by the same exact
    /// strings a lens's `[trigger] signals` names, in a fixed order.
    #[must_use]
    pub fn signals(&self) -> Vec<String> {
        self.signal_list.clone()
    }

    #[must_use]
    pub fn render_human(&self) -> String {
        use std::fmt::Write as _;
        let mut out = format!("tier: {}\n", self.tier.as_str());
        for reason in &self.reasons {
            writeln!(out, "reason: {reason}").expect("writing to a string never fails");
        }
        for axis in &self.axes_add {
            writeln!(out, "axis-add: {}", axis.as_str()).expect("writing to a string never fails");
        }
        for signal in &self.signal_list {
            writeln!(out, "signal: {signal}").expect("writing to a string never fails");
        }
        writeln!(out, "files: {}", self.files).expect("writing to a string never fails");
        writeln!(out, "lines: {}", self.lines).expect("writing to a string never fails");
        writeln!(out, "base: {}", self.base).expect("writing to a string never fails");
        write!(out, "head: {}", self.head).expect("writing to a string never fails");
        out
    }

    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "tier": self.tier.as_str(),
            "reasons": self.reasons,
            "axes_add": self.axes_add.iter().map(|a| a.as_str()).collect::<Vec<_>>(),
            "signals": self.signal_list,
            "files": self.files,
            "lines": self.lines,
            "base": self.base,
            "head": self.head,
        })
    }
}

/// A path where a defect reaches money, identity, data shape, or the
/// platform itself.
const HIGH_PATTERNS: &[&str] = &[
    r"(^|/)(auth|oauth|oidc|keycloak|secrets?|credentials?|tokens?|passwords?|crypt)",
    r"(^|/)(payments?|billing|money|valuation|pricing|balances?|ledger|wallet|transactions?)",
    r"(^|/)migrations?/|\.sql$|(^|/)schemas?/",
    r"(^|/)(infra|terraform|cdk|bicep|pulumi|helm|k8s)/|\.tf$|^\.github/workflows/|(^|/)Dockerfile|(^|/)\.devcontainer/",
];

/// Above this many changed lines, a change is high blast radius regardless of path.
const MAX_LINES: usize = 600;
/// Above this many changed files, a change is high blast radius regardless of path.
const MAX_FILES: usize = 12;
/// At or under this many files and lines, with no high-blast-radius path, a
/// change is low blast radius.
const SMALL_CHANGE_FILES: usize = 2;
const SMALL_CHANGE_LINES: usize = 80;

/// A directory whose content counts as code for the docs-only rule, even
/// when a file inside it is itself Markdown: the shared `.agents` folder,
/// and every supported coding agent's own directory, read from the one
/// list in `crate::agents` so no agent is named here on its own.
fn code_dirs_pattern() -> String {
    let dirs = std::iter::once(".agents")
        .chain(crate::agents::state_dirs())
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join("|");
    format!("^({dirs})/")
}
const DOC_FILES: &str = r"\.(md|txt|rst|adoc)$|^docs/";
const TEST_FILES: &str = r"(^|/)(tests?|spec|specs|__tests__|test_data|fixtures)/|[._-](test|tests|spec)\.[a-z]+$|_test\.go$|Tests?\.cs$|Test\.java$|Tests?\.kt$|_test\.dart$";
const UI_PATTERN: &str = r"\.(dart|tsx|jsx|vue|svelte|xaml|razor|html|css|scss)$|(^|/)(screens?|widgets?|pages?|views?|components?)/";
const DOCS_PATTERN: &str = r"^docs/|(^|/)(adr|decisions|design)/.*\.md$";
const DEPS_PATTERN: &str = r"(^|/)(package(-lock)?\.json|pnpm-lock\.yaml|yarn\.lock|packages\.lock\.json|Directory\.Packages\.props|.*\.csproj|build\.gradle(\.kts)?|gradle\.lockfile|libs\.versions\.toml|pubspec\.(yaml|lock)|pyproject\.toml|uv\.lock|requirements[^/]*\.txt|Cargo\.(toml|lock)|go\.(mod|sum))$";

/// A path that spends money to run: infrastructure-as-code.
const COST_PATTERN: &str = r"\.(tf|bicep)$|(^|/)infra/";
/// A path that draws a module or crate boundary.
const MODULE_PATTERN: &str = r"(^|/)(mod|lib)\.rs$";
/// A manifest that names a project's own dependencies.
const MANIFEST_PATTERN: &str = r"(^|/)Cargo\.toml$|\.csproj$";
/// A path where a change reaches a caller outside this codebase.
const INTERFACE_PATTERN: &str = r"(^|/)api/|\.proto$|(^|/)openapi/|\.graphql$";
/// A query file, whose cost scales with the data it reads.
const PERFORMANCE_VOLUME_PATTERN: &str = r"\.sql$";
/// A benchmark or a loop: a path a request runs through often.
const PERFORMANCE_TRAFFIC_PATTERN: &str = r"(^|/)benches?/|loop";
/// A path naming a thread, an async task or a lock.
const CONCURRENCY_PATTERN: &str = r"thread|async|lock";
/// A path naming a retry or a circuit breaker: a call to something outside this process.
const RELIABILITY_PATTERN: &str = r"(^|/)retry/|circuit";
/// A path under a shared helper or utility directory.
const HELPER_PATTERN: &str = r"(^|/)(helpers?|utils?)/";

/// Compiles a pattern this module owns: a bug that stops it compiling is
/// caught by the test suite, never by a person running the command.
fn built_in(pattern: &str) -> Regex {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .expect("built-in risk pattern compiles")
}

/// Compiles one line from a repository's own high-blast-radius path file.
///
/// # Errors
/// Returns an error if the line is not a valid regular expression.
fn repo_supplied(pattern: &str) -> Result<Regex, String> {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .map_err(|e| format!("'{pattern}' in the risk-paths file is not a valid pattern: {e}"))
}

fn any_match(patterns: &[Regex], path: &str) -> bool {
    patterns.iter().any(|p| p.is_match(path))
}

/// Extra high-blast-radius path patterns, one per non-comment, non-blank
/// line of `.agents/risk-paths.txt` at the repository root.
///
/// A repository under review controls this file, yet reading it here is
/// still safe: the file can only add patterns to the high tier, never
/// remove one, so it can tighten this check's answer and never loosen it.
///
/// # Errors
/// Returns an error if the file exists but cannot be read.
fn repo_high_path_lines(root: &Path) -> Result<Vec<String>, String> {
    let path = root.join(".agents").join("risk-paths.txt");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect())
}

/// The sum of added and deleted lines across every non-binary entry in one
/// `git diff --numstat` output: a binary file reports `-` for both counts
/// and contributes nothing, the same as `awk '$1 != "-"'` does.
fn sum_numstat(raw: &[String]) -> usize {
    raw.iter()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let added = fields.next()?;
            let deleted = fields.next()?;
            if added == "-" || deleted == "-" {
                return None;
            }
            let added: usize = added.trim().parse().ok()?;
            let deleted: usize = deleted.trim().parse().ok()?;
            Some(added + deleted)
        })
        .sum()
}

/// Every line in an untracked file: it appears in no diff, so every line of
/// it is a changed line. Counted the way `wc -l` counts, by newline bytes,
/// so a file without a trailing newline is not over-counted by one line.
/// A plain scan, not the `bytecount` crate: this runs once per untracked
/// file, never on a hot path worth a SIMD dependency.
#[allow(clippy::naive_bytecount)]
fn untracked_file_lines(dir: &Path, git_path: &str) -> Result<usize, String> {
    let full = crate::git::to_local_path(dir, git_path);
    let bytes = std::fs::read(&full).map_err(|e| format!("cannot read {}: {e}", full.display()))?;
    Ok(bytes.iter().filter(|&&b| b == b'\n').count())
}

/// Shortens a list of matched paths to the first five, noting how many more
/// there were, the same shape a person reads comfortably in a terminal.
fn shorten(mut matched: Vec<&str>) -> String {
    matched.sort_unstable();
    let total = matched.len();
    matched.truncate(5);
    let shown = matched.join(", ");
    if total > 5 {
        format!("{shown}, and {} more", total - 5)
    } else {
        shown
    }
}

/// Every file that changed between `base` and `HEAD`, plus the working
/// tree: committed changes, uncommitted changes, and untracked files, once
/// each. Also returns the untracked subset on its own, since its line
/// count is computed differently from a tracked file's.
fn changed_files(dir: &Path, base: &str) -> Result<(BTreeSet<String>, Vec<String>), String> {
    let merge_base_range = format!("{base}...HEAD");
    let mut files = BTreeSet::new();
    for f in crate::git::diff_name_only(dir, &merge_base_range).map_err(|e| e.to_string())? {
        files.insert(f);
    }
    for f in crate::git::diff_name_only(dir, "HEAD").map_err(|e| e.to_string())? {
        files.insert(f);
    }
    let untracked = crate::git::untracked_files(dir).map_err(|e| e.to_string())?;
    for f in &untracked {
        files.insert(f.clone());
    }
    Ok((files, untracked))
}

/// How many changed lines the change carries: committed and uncommitted
/// diffs by `numstat`, plus every line of every untracked file.
fn changed_lines(dir: &Path, base: &str, untracked: &[String]) -> Result<usize, String> {
    let merge_base_range = format!("{base}...HEAD");
    let mut lines =
        sum_numstat(&crate::git::diff_numstat(dir, &merge_base_range).map_err(|e| e.to_string())?);
    lines += sum_numstat(&crate::git::diff_numstat(dir, "HEAD").map_err(|e| e.to_string())?);
    for path in untracked {
        lines += untracked_file_lines(dir, path)?;
    }
    Ok(lines)
}

/// A file that is not purely documentation: either it lives under a
/// directory this project treats as code regardless of extension, or it
/// does not look like a documentation file at all.
fn is_non_doc(code_dirs: &Regex, doc_files: &Regex, path: &str) -> bool {
    code_dirs.is_match(path) || !doc_files.is_match(path)
}

/// The tier and its reasons, worked out from the file and line counts and
/// the high-blast-radius path matches. Does not yet know about the extra
/// review axes; see [`axes_for`].
fn tier_for(
    files: &[&str],
    file_count: usize,
    line_count: usize,
    high: &[Regex],
) -> (Tier, Vec<String>) {
    let mut reasons = Vec::new();
    let mut tier = Tier::Normal;

    let high_hits: Vec<&str> = files
        .iter()
        .copied()
        .filter(|f| any_match(high, f))
        .collect();
    if !high_hits.is_empty() {
        tier = Tier::High;
        reasons.push(format!("high-blast-radius paths: {}", shorten(high_hits)));
    }
    if line_count > MAX_LINES {
        tier = Tier::High;
        reasons.push(format!(
            "{line_count} changed lines, over the {MAX_LINES} limit"
        ));
    }
    if file_count > MAX_FILES {
        tier = Tier::High;
        reasons.push(format!(
            "{file_count} changed files, over the {MAX_FILES} limit"
        ));
    }

    if !matches!(tier, Tier::High) {
        let code_dirs = built_in(&code_dirs_pattern());
        let doc_files = built_in(DOC_FILES);
        let test_files = built_in(TEST_FILES);
        let docs_only = files.iter().all(|f| !is_non_doc(&code_dirs, &doc_files, f));
        let tests_only = files.iter().all(|f| test_files.is_match(f));
        if docs_only {
            tier = Tier::Low;
            reasons.push(format!("documentation only, {file_count} files"));
        } else if tests_only {
            tier = Tier::Low;
            reasons.push(format!("tests only, {file_count} files"));
        } else if file_count <= SMALL_CHANGE_FILES && line_count < SMALL_CHANGE_LINES {
            tier = Tier::Low;
            reasons.push(format!(
                "{file_count} files, {line_count} lines, no high-blast-radius path"
            ));
        } else {
            reasons.push(format!(
                "{file_count} files, {line_count} lines, no high-blast-radius path, not docs-only or tests-only"
            ));
        }
    }

    (tier, reasons)
}

/// The extra review axes a change earns, in a fixed order, whatever tier it landed on.
fn axes_for(files: &[&str]) -> Vec<Axis> {
    let ui = built_in(UI_PATTERN);
    let docs = built_in(DOCS_PATTERN);
    let deps = built_in(DEPS_PATTERN);
    let mut axes = Vec::new();
    if files.iter().any(|f| ui.is_match(f)) {
        axes.push(Axis::UiProofAccessibility);
    }
    if files.iter().any(|f| docs.is_match(f)) {
        axes.push(Axis::DocumentsDesignAdrs);
    }
    if files.iter().any(|f| deps.is_match(f)) {
        axes.push(Axis::DependencyLicenceSupplyChain);
    }
    axes
}

/// Whether two changed files share a base name, the cheap sign of logic
/// copied from one file into another rather than reused.
fn repeated_logic(files: &[&str]) -> bool {
    let mut names: Vec<&str> = files
        .iter()
        .map(|f| f.rsplit('/').next().unwrap_or(f))
        .collect();
    names.sort_unstable();
    names.windows(2).any(|pair| pair.first() == pair.get(1))
}

/// The review-lens trigger signals a change earns, in a fixed order,
/// whatever tier it landed on: the counterpart to [`axes_for`], but naming
/// the exact strings a lens's `[trigger] signals` list matches against,
/// rather than the three built-in axes.
fn signals_for(files: &[&str], untracked: &[String]) -> Vec<String> {
    let ui = built_in(UI_PATTERN);
    let deps = built_in(DEPS_PATTERN);
    let docs = built_in(DOCS_PATTERN);
    let cost = built_in(COST_PATTERN);
    let module = built_in(MODULE_PATTERN);
    let manifest = built_in(MANIFEST_PATTERN);
    let interface = built_in(INTERFACE_PATTERN);
    let perf_volume = built_in(PERFORMANCE_VOLUME_PATTERN);
    let perf_traffic = built_in(PERFORMANCE_TRAFFIC_PATTERN);
    let concurrency = built_in(CONCURRENCY_PATTERN);
    let reliability = built_in(RELIABILITY_PATTERN);
    let helper = built_in(HELPER_PATTERN);

    let mut signals = Vec::new();
    let mut add = |earned: bool, name: &str| {
        if earned {
            signals.push(name.to_string());
        }
    };

    add(files.iter().any(|f| ui.is_match(f)), "user-visible change");
    add(files.iter().any(|f| deps.is_match(f)), "dependency delta");
    add(
        files.iter().any(|f| docs.is_match(f)),
        "new-decision-record",
    );
    add(files.iter().any(|f| cost.is_match(f)), "cost");
    add(files.iter().any(|f| module.is_match(f)), "new-module");
    add(files.iter().any(|f| manifest.is_match(f)), "new-dependency");
    add(
        files.iter().any(|f| interface.is_match(f)),
        "public surface",
    );
    add(
        files.iter().any(|f| interface.is_match(f)),
        "external interface",
    );
    add(
        files.iter().any(|f| perf_volume.is_match(f)),
        "performance and data volume",
    );
    add(
        files.iter().any(|f| perf_traffic.is_match(f)),
        "high-traffic path",
    );
    add(files.iter().any(|f| concurrency.is_match(f)), "concurrency");
    add(
        files.iter().any(|f| reliability.is_match(f)),
        "external-call",
    );
    add(files.iter().any(|f| reliability.is_match(f)), "timeout");
    add(files.iter().any(|f| helper.is_match(f)), "new-helper");
    add(!untracked.is_empty(), "new-file");
    add(repeated_logic(files), "repeated-logic");

    signals
}

/// Assesses the blast radius of the change between `base` and the working
/// tree in `dir`: every commit since their merge base, plus anything
/// uncommitted, plus anything untracked.
///
/// # Errors
/// Returns an error if `base` does not resolve to a commit, if git cannot
/// run, if the repository's high-path file cannot be read or holds an
/// invalid pattern, or if there is no changed file to report on at all.
pub fn assess(dir: &Path, base: &str) -> Result<Report, String> {
    if !crate::git::commit_exists(dir, base) {
        return Err(format!("base '{base}' does not resolve"));
    }
    let root = crate::git::repo_root(dir).map_err(|e| e.to_string())?;
    let head = crate::git::head_short_sha(dir).map_err(|e| e.to_string())?;

    let (files, untracked) = changed_files(dir, base)?;
    if files.is_empty() {
        return Err(format!("no changed files against {base}"));
    }
    let line_count = changed_lines(dir, base, &untracked)?;

    let mut high: Vec<Regex> = HIGH_PATTERNS.iter().map(|p| built_in(p)).collect();
    for extra in repo_high_path_lines(&root)? {
        high.push(repo_supplied(&extra)?);
    }

    let file_refs: Vec<&str> = files.iter().map(String::as_str).collect();
    let (tier, reasons) = tier_for(&file_refs, files.len(), line_count, &high);
    let axes_add = axes_for(&file_refs);
    let signal_list = signals_for(&file_refs, &untracked);

    Ok(Report {
        tier,
        reasons,
        axes_add,
        files: files.len(),
        lines: line_count,
        base: base.to_string(),
        head,
        signal_list,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorten_keeps_five_and_counts_the_rest() {
        let matched = vec!["f", "e", "d", "c", "b", "a"];
        assert_eq!(shorten(matched), "a, b, c, d, e, and 1 more");
    }

    #[test]
    fn shorten_with_few_matches_lists_them_all() {
        let matched = vec!["b", "a"];
        assert_eq!(shorten(matched), "a, b");
    }

    #[test]
    fn sum_numstat_skips_binary_markers() {
        let raw = vec!["3\t2\ta.txt".to_string(), "-\t-\tbinary.png".to_string()];
        assert_eq!(sum_numstat(&raw), 5);
    }

    #[test]
    fn an_unrelated_file_earns_no_signal() {
        assert!(signals_for(&["README.md"], &[]).is_empty());
    }

    #[test]
    fn two_files_sharing_a_name_earn_the_repeated_logic_signal() {
        let signals = signals_for(&["src/a/parser.rs", "src/b/parser.rs"], &[]);
        assert!(
            signals.contains(&"repeated-logic".to_string()),
            "{signals:?}"
        );
    }

    #[test]
    fn an_untracked_file_earns_the_new_file_signal() {
        let untracked = vec!["src/new.rs".to_string()];
        let signals = signals_for(&["src/new.rs"], &untracked);
        assert!(signals.contains(&"new-file".to_string()), "{signals:?}");
    }

    #[test]
    fn every_triggered_lens_signal_can_be_emitted_by_signals_for() {
        let root = crate::test_support::TempDir::new("osf-risk-signals-catalogue");
        let catalogue = crate::lenses::load(&root, None).expect("loads");
        let files = vec![
            "lib/screens/home.dart",
            "package.json",
            "docs/architecture/0001-decision.md",
            "infra/main.tf",
            "crates/osf/src/mod.rs",
            "Cargo.toml",
            "src/api/handlers.rs",
            "reports/query.sql",
            "benches/bench_loop.rs",
            "src/worker/thread_pool.rs",
            "src/net/retry/backoff.rs",
            "src/utils/format.rs",
            "src/a/parser.rs",
            "src/b/parser.rs",
        ];
        let untracked = vec!["new/file.rs".to_string()];
        let emitted = signals_for(&files, &untracked);

        for lens in &catalogue.lenses {
            for signal in &lens.trigger.signals {
                assert!(
                    emitted.contains(signal),
                    "{} names the trigger signal '{signal}', which risk::signals_for never emits",
                    lens.name
                );
            }
        }
    }
}
