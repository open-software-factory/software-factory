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

/// A dependency lockfile: content matches here are never a concurrency signal, since a lockfile's own text (a crate or package name) routinely contains a concurrency word with no bearing on this change.
const LOCKFILE_PATTERN: &str = r"(^|/)(Cargo\.lock|package-lock\.json|pnpm-lock\.yaml|yarn\.lock|Gemfile\.lock|poetry\.lock|composer\.lock|Pipfile\.lock)$";
/// A changed line that touches concurrency primitives, checked against real diff content, never a path.
const CONCURRENCY_CONTENT_PATTERN: &str = r"\b(thread|async|mutex|rwlock|atomic)";
/// A Rust item exported outside its own crate: `pub(crate)` never matches, since no whitespace follows `pub` there.
const RUST_PUBLIC_ITEM_PATTERN: &str = r"^\s*pub\s+(fn|struct|enum|trait)\b";
/// A TypeScript export declaration.
const TS_EXPORT_PATTERN: &str = r"^\s*export\s+(function|class|interface|const|enum)\b";
/// A clap-derived CLI flag: cheap to check alongside the Rust public-item pattern.
const CLI_FLAG_PATTERN: &str = r"#\[arg\(|#\[command\(";

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

/// One changed file's status and diff content, the raw material every
/// content-based signal reads instead of the file's path.
struct FileDiff {
    path: String,
    added: bool,
    added_hunks: Vec<Vec<String>>,
    removed_lines: Vec<String>,
}

/// Parses one `git diff --unified=0` patch into `hunks` (added lines, kept
/// grouped by hunk so a contiguous block stays intact) and `removed` (every
/// deleted line, path order not significant), both keyed by path.
fn merge_patch(
    patch: &str,
    hunks: &mut std::collections::BTreeMap<String, Vec<Vec<String>>>,
    removed: &mut std::collections::BTreeMap<String, Vec<String>>,
) {
    let mut current: Option<String> = None;
    for line in patch.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            current = Some(path.to_string());
            continue;
        }
        if line.starts_with("+++ ") || line.starts_with("--- ") {
            continue;
        }
        if line.starts_with("@@") {
            if let Some(path) = &current {
                hunks.entry(path.clone()).or_default().push(Vec::new());
            }
            continue;
        }
        if let Some(text) = line.strip_prefix('+') {
            if let Some(path) = &current {
                if let Some(hunk) = hunks.get_mut(path).and_then(|h| h.last_mut()) {
                    hunk.push(text.to_string());
                }
            }
            continue;
        }
        if let Some(text) = line.strip_prefix('-') {
            if let Some(path) = &current {
                removed
                    .entry(path.clone())
                    .or_default()
                    .push(text.to_string());
            }
        }
    }
}

/// Every changed file's status and diff content: the merge-base range, the
/// uncommitted diff against `HEAD`, and every untracked file's own content
/// counted as one added hunk, the same three scopes [`changed_files`] and
/// [`changed_lines`] already read.
fn collect_diffs(
    dir: &Path,
    base: &str,
    files: &BTreeSet<String>,
    untracked: &[String],
) -> Result<Vec<FileDiff>, String> {
    let merge_base_range = format!("{base}...HEAD");
    let mut added_status: std::collections::HashSet<String> = std::collections::HashSet::new();
    for source in [merge_base_range.as_str(), "HEAD"] {
        for (status, path) in
            crate::git::diff_name_status(dir, source).map_err(|e| e.to_string())?
        {
            if status == 'A' {
                added_status.insert(path);
            }
        }
    }
    for path in untracked {
        added_status.insert(path.clone());
    }

    let mut hunks: std::collections::BTreeMap<String, Vec<Vec<String>>> =
        std::collections::BTreeMap::new();
    let mut removed: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for source in [merge_base_range.as_str(), "HEAD"] {
        let patch = crate::git::diff_patch(dir, source).map_err(|e| e.to_string())?;
        merge_patch(&patch, &mut hunks, &mut removed);
    }
    for path in untracked {
        let full = crate::git::to_local_path(dir, path);
        if let Ok(text) = std::fs::read_to_string(&full) {
            hunks
                .entry(path.clone())
                .or_default()
                .push(text.lines().map(str::to_string).collect());
        }
    }

    Ok(files
        .iter()
        .map(|path| FileDiff {
            path: path.clone(),
            added: added_status.contains(path),
            added_hunks: hunks.remove(path).unwrap_or_default(),
            removed_lines: removed.remove(path).unwrap_or_default(),
        })
        .collect())
}

/// Whether `line`, from `path`, adds or removes an item outside this
/// project's own crate or module: a Rust `pub fn`/`struct`/`enum`/`trait`
/// (never `pub(crate)`, which has no space before its parenthesis), a
/// TypeScript `export`, or a clap CLI flag attribute.
fn line_is_public_surface(path: &str, line: &str) -> bool {
    let extension = Path::new(path)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();
    if extension.eq_ignore_ascii_case("rs") {
        return built_in(RUST_PUBLIC_ITEM_PATTERN).is_match(line)
            || built_in(CLI_FLAG_PATTERN).is_match(line);
    }
    if extension.eq_ignore_ascii_case("ts") || extension.eq_ignore_ascii_case("tsx") {
        return built_in(TS_EXPORT_PATTERN).is_match(line);
    }
    false
}

/// Whether the change adds or removes an exported item, read from the
/// diff's own content rather than which files it touched.
fn public_surface_changed(diffs: &[FileDiff]) -> bool {
    diffs.iter().any(|d| {
        d.added_hunks
            .iter()
            .flatten()
            .chain(&d.removed_lines)
            .any(|line| line_is_public_surface(&d.path, line))
    })
}

/// Whether a changed line, outside a lockfile, touches a concurrency
/// primitive: a lockfile's own text routinely names a crate or package
/// containing one of these words with no bearing on this change.
fn concurrency_changed(diffs: &[FileDiff]) -> bool {
    let lockfile = built_in(LOCKFILE_PATTERN);
    let keyword = built_in(CONCURRENCY_CONTENT_PATTERN);
    diffs.iter().any(|d| {
        !lockfile.is_match(&d.path)
            && (d
                .added_hunks
                .iter()
                .flatten()
                .any(|line| keyword.is_match(line))
                || d.removed_lines.iter().any(|line| keyword.is_match(line)))
    })
}

/// One added hunk's lines, trimmed and rejoined, so two hunks that differ
/// only in indentation still compare equal.
fn normalise_block(lines: &[String]) -> String {
    lines
        .iter()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether two different files each added a near-identical block of at
/// least six lines: the diff content itself, never a shared file name.
fn repeated_logic(diffs: &[FileDiff]) -> bool {
    let mut seen: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
    for diff in diffs {
        for hunk in &diff.added_hunks {
            if hunk.len() < 6 {
                continue;
            }
            let key = normalise_block(hunk);
            if key.trim().is_empty() {
                continue;
            }
            match seen.get(&key) {
                Some(&other) if other != diff.path => return true,
                Some(_) => {}
                None => {
                    seen.insert(key, &diff.path);
                }
            }
        }
    }
    false
}

/// The `[review] hot_paths` glob list from `root`'s own `osf.toml`, empty
/// when the file, the table or the key is missing or malformed: a
/// high-traffic path needs telemetry a diff cannot supply on its own, so
/// with nothing configured, this signal never fires.
fn hot_paths(root: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(root.join("osf.toml")) else {
        return Vec::new();
    };
    let Ok(value) = toml::from_str::<toml::Value>(&text) else {
        return Vec::new();
    };
    value
        .get("review")
        .and_then(|t| t.get("hot_paths"))
        .and_then(toml::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether any changed path matches one of `hot_paths`'s own globs.
fn matches_any_hot_path(hot_paths: &[String], diffs: &[FileDiff]) -> bool {
    hot_paths.iter().any(|pattern| {
        globset::Glob::new(pattern).is_ok_and(|glob| {
            let matcher = glob.compile_matcher();
            diffs.iter().any(|d| matcher.is_match(&d.path))
        })
    })
}

/// The review-lens trigger signals a change earns, in a fixed order,
/// whatever tier it landed on: each one reads the diff's content or an
/// explicit configuration, never only a changed path, since a path glob is
/// already what a lens's own `[trigger] paths` checks.
fn signals_for(diffs: &[FileDiff], hot_paths: &[String]) -> Vec<String> {
    let mut signals = Vec::new();
    let mut add = |earned: bool, name: &str| {
        if earned {
            signals.push(name.to_string());
        }
    };

    add(public_surface_changed(diffs), "public surface");
    add(concurrency_changed(diffs), "concurrency");
    add(diffs.iter().any(|d| d.added), "new-file");
    add(repeated_logic(diffs), "repeated-logic");
    add(matches_any_hot_path(hot_paths, diffs), "high-traffic path");

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
    let diffs = collect_diffs(dir, base, &files, &untracked)?;
    let signal_list = signals_for(&diffs, &hot_paths(&root));

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

    /// Builds one `FileDiff` tersely: each element of `added_hunks` is one
    /// hunk's added lines, kept as separate `Vec`s the way real hunk
    /// boundaries would.
    fn fd(
        path: &str,
        added: bool,
        added_hunks: Vec<Vec<&str>>,
        removed_lines: Vec<&str>,
    ) -> FileDiff {
        FileDiff {
            path: path.to_string(),
            added,
            added_hunks: added_hunks
                .into_iter()
                .map(|h| h.into_iter().map(str::to_string).collect())
                .collect(),
            removed_lines: removed_lines.into_iter().map(str::to_string).collect(),
        }
    }

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
    fn an_unrelated_change_earns_no_signal() {
        let diffs = vec![fd(
            "README.md",
            false,
            vec![vec!["Some plain prose."]],
            vec![],
        )];
        assert!(signals_for(&diffs, &[]).is_empty());
    }

    // C4 negative test: a lockfile-only change never earns "concurrency",
    // even though its own text names a crate containing a concurrency word.
    #[test]
    fn a_lockfile_change_earns_no_concurrency_signal() {
        let diffs = vec![fd(
            "Cargo.lock",
            false,
            vec![vec!["name = \"async-trait\"", "version = \"0.1.0\""]],
            vec![],
        )];
        assert!(!concurrency_changed(&diffs));
        assert!(!signals_for(&diffs, &[]).contains(&"concurrency".to_string()));
    }

    #[test]
    fn a_concurrency_keyword_in_a_source_file_earns_the_concurrency_signal() {
        let diffs = vec![fd(
            "src/worker.rs",
            false,
            vec![vec!["let guard = Mutex::new(0);"]],
            vec![],
        )];
        assert!(concurrency_changed(&diffs));
    }

    // C4 negative test: two README.md files sharing a name, but not their
    // added content, never earn "repeated-logic".
    #[test]
    fn two_readme_files_with_different_content_earn_no_repeated_logic_signal() {
        let diffs = vec![
            fd(
                "docs/a/README.md",
                false,
                vec![vec![
                    "line one a",
                    "line two a",
                    "line three a",
                    "line four a",
                    "line five a",
                    "line six a",
                ]],
                vec![],
            ),
            fd(
                "docs/b/README.md",
                false,
                vec![vec![
                    "line one b",
                    "line two b",
                    "line three b",
                    "line four b",
                    "line five b",
                    "line six b",
                ]],
                vec![],
            ),
        ];
        assert!(!repeated_logic(&diffs));
    }

    #[test]
    fn two_files_sharing_a_near_identical_added_block_earn_repeated_logic() {
        let block = vec![
            "fn helper() {",
            "    step_one();",
            "    step_two();",
            "    step_three();",
            "    step_four();",
            "}",
        ];
        let diffs = vec![
            fd("src/a.rs", false, vec![block.clone()], vec![]),
            fd("src/b.rs", false, vec![block], vec![]),
        ];
        assert!(repeated_logic(&diffs));
    }

    // C4 negative test: a `pub(crate)` item, which has no space before its
    // parenthesis, is not a public-surface change.
    #[test]
    fn a_pub_crate_item_is_not_a_public_surface() {
        let diffs = vec![fd(
            "src/lib.rs",
            false,
            vec![vec!["pub(crate) fn helper() {}"]],
            vec![],
        )];
        assert!(!public_surface_changed(&diffs));
    }

    #[test]
    fn a_new_pub_fn_is_a_public_surface() {
        let diffs = vec![fd(
            "src/lib.rs",
            false,
            vec![vec!["pub fn helper() {}"]],
            vec![],
        )];
        assert!(public_surface_changed(&diffs));
    }

    #[test]
    fn a_removed_pub_fn_is_also_a_public_surface() {
        let diffs = vec![fd(
            "src/lib.rs",
            false,
            vec![],
            vec!["pub fn old_helper() {}"],
        )];
        assert!(public_surface_changed(&diffs));
    }

    #[test]
    fn a_typescript_export_is_a_public_surface() {
        let diffs = vec![fd(
            "web/api.ts",
            false,
            vec![vec!["export function fetchUser() {}"]],
            vec![],
        )];
        assert!(public_surface_changed(&diffs));
    }

    #[test]
    fn an_added_file_earns_the_new_file_signal() {
        let diffs = vec![fd("src/new.rs", true, vec![], vec![])];
        assert!(signals_for(&diffs, &[]).contains(&"new-file".to_string()));
    }

    #[test]
    fn a_modified_file_alone_earns_no_new_file_signal() {
        let diffs = vec![fd(
            "src/existing.rs",
            false,
            vec![vec!["let x = 1;"]],
            vec![],
        )];
        assert!(!signals_for(&diffs, &[]).contains(&"new-file".to_string()));
    }

    #[test]
    fn high_traffic_path_only_fires_when_configured() {
        let diffs = vec![fd("src/hot/handler.rs", false, vec![], vec![])];
        assert!(!matches_any_hot_path(&[], &diffs));
        assert!(matches_any_hot_path(&["**/hot/**".to_string()], &diffs));
    }

    #[test]
    fn every_triggered_lens_signal_can_be_emitted_by_signals_for() {
        let root = crate::test_support::TempDir::new("osf-risk-signals-catalogue");
        let catalogue = crate::lenses::load(&root, None).expect("loads");
        let block = vec![
            "fn helper() {",
            "    step_one();",
            "    step_two();",
            "    step_three();",
            "    step_four();",
            "}",
        ];
        let diffs = vec![
            fd(
                "src/api.rs",
                false,
                vec![vec!["pub fn new_endpoint() {}"]],
                vec![],
            ),
            fd(
                "src/worker.rs",
                false,
                vec![vec!["let lock = Mutex::new(0);"]],
                vec![],
            ),
            fd("src/new.rs", true, vec![], vec![]),
            fd("src/a.rs", false, vec![block.clone()], vec![]),
            fd("src/b.rs", false, vec![block], vec![]),
            fd("src/hot/handler.rs", false, vec![], vec![]),
        ];
        let hot = vec!["**/hot/**".to_string()];
        let emitted = signals_for(&diffs, &hot);

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
