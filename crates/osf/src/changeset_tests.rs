//! `osf changeset tests`: a summary of Rust tests added, changed and removed
//! between two refs, read by parsing the base and head versions of each
//! changed `.rs` file with the tree-sitter Rust grammar. Never builds or
//! runs the change's code, so the summary cannot drift from what a build
//! would have seen, and it cannot be fooled by code that would not compile.
//!
//! This covers Rust only. A non-Rust test file is named in the summary as
//! not yet supported rather than silently skipped; extending the same
//! summary to other languages is
//! open-software-factory/software-factory#188.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use regex::RegexBuilder;
use tree_sitter::{Node, Parser};

/// A `changeset tests` run could not finish: a bad ref, or git failing to
/// run. A file the parser cannot read is not this error; it is reported
/// inside the summary as unparsed instead, so one unreadable file never
/// stops the rest of the summary from being built.
#[derive(Debug)]
pub struct TestSummaryError(String);

impl std::fmt::Display for TestSummaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TestSummaryError {}

/// One test's identity (its module path plus its function name, such as
/// `tests::a_thing_works`) and the one-line, plain-English statement of
/// what it checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestEntry {
    pub name: String,
    pub description: String,
}

/// What one changed file has to say about its tests.
#[derive(Debug)]
pub enum GroupBody {
    /// The usual case: the file parsed on both sides, with these tests
    /// added, changed, or removed between them.
    Tests {
        removed: Vec<TestEntry>,
        added: Vec<TestEntry>,
        changed: Vec<TestEntry>,
    },
    /// The Rust grammar could not read this file on at least one side.
    Unparsed,
    /// A test file in a language this build does not parse yet; see
    /// open-software-factory/software-factory#188.
    Unsupported,
}

/// One changed file's worth of test changes, grouped by the crate it
/// belongs to.
#[derive(Debug)]
pub struct Group {
    pub crate_name: String,
    pub file: String,
    pub body: GroupBody,
}

/// Every test change between two refs, plus the three totals across all of
/// them. A file with no test change at all carries no group; it would add
/// nothing a reviewer needs to see.
#[derive(Debug, Default)]
pub struct Summary {
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
    pub groups: Vec<Group>,
}

/// A path segment this build never looks inside: fixtures exist to be
/// read by a test, not to be counted as one.
fn under_test_fixtures(path: &str) -> bool {
    path.starts_with("tests/fixtures/") || path.contains("/tests/fixtures/")
}

/// The crate a path belongs to, read from the `crates/<name>/...` layout
/// this workspace uses; anything outside that layout is the workspace
/// root itself.
fn crate_of(path: &str) -> String {
    path.strip_prefix("crates/")
        .and_then(|rest| rest.split('/').next())
        .filter(|name| !name.is_empty())
        .map_or_else(|| "(workspace root)".to_string(), str::to_string)
}

/// Whether `path` names a Rust source file, by its extension alone.
fn is_rust_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
}

/// Whether `path` looks like a test file in some language, by the same
/// name and location patterns [`crate::changeset_risk`] already uses to keep tests
/// out of the blast-radius count.
fn looks_like_a_test_file(path: &str) -> bool {
    RegexBuilder::new(crate::changeset_risk::TEST_FILES)
        .case_insensitive(true)
        .build()
        .expect("built-in test-file pattern compiles")
        .is_match(path)
}

/// What reading one path at one revision came back with.
enum RawContent {
    /// The path does not exist at that revision at all.
    Absent,
    /// The path exists but git could not read it, or its bytes are not
    /// valid UTF-8. Never lossy-converted: content this build cannot trust
    /// as text is reported as unparsed, not guessed at.
    Unparsed,
    Text(String),
}

/// `path` as it reads at `rev`. Checks existence first, so a path missing
/// at that revision is [`RawContent::Absent`] rather than folded into the
/// same case as a read that failed for some other reason.
fn read_at(dir: &Path, rev: &str, path: &str) -> RawContent {
    match crate::git::path_exists_at(dir, rev, path) {
        Ok(false) => return RawContent::Absent,
        Err(_) => return RawContent::Unparsed,
        Ok(true) => {}
    }
    match crate::git::content_at(dir, rev, path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => RawContent::Text(text),
            Err(_) => RawContent::Unparsed,
        },
        Err(_) => RawContent::Unparsed,
    }
}

/// One test found while walking a parsed file: the source text of its
/// whole item, compared byte for byte to tell a changed test from an
/// unchanged one, and its already-computed description.
struct FoundTest {
    source: String,
    description: String,
}

/// Parses `source` with the Rust grammar. `None` when the grammar itself
/// cannot be loaded (never happens outside a broken build) or the tree
/// comes back with a syntax error: either way, this file cannot be read
/// reliably enough to trust what it says about tests.
fn parse_rust(source: &str) -> Option<tree_sitter::Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .ok()?;
    let tree = parser.parse(source, None)?;
    if tree.root_node().has_error() {
        return None;
    }
    Some(tree)
}

/// A `///` line's text with the marker and one leading space removed, or
/// `None` when the line is not an outer doc comment: a plain `//` comment,
/// an inner `//!` comment (which documents the enclosing item, not the
/// next one), or a `////`-or-longer banner comment.
fn doc_comment_text(text: &str) -> Option<&str> {
    let trimmed = text.trim_end_matches(['\n', '\r']);
    let rest = trimmed.strip_prefix("///")?;
    if rest.starts_with('/') {
        return None;
    }
    Some(rest.trim())
}

/// Whether an `attribute_item` node marks the function that follows it as
/// a test: `#[test]`, `#[tokio::test]`, `#[async_std::test]`, or any other
/// attribute whose path ends in `test`. `#[cfg(test)]` does not match: its
/// path is `cfg`, with `test` only inside its arguments.
fn is_test_attribute(attribute_item: Node, source: &str) -> bool {
    let Some(attribute) = attribute_item.named_child(0) else {
        return false;
    };
    let Some(path) = attribute.named_child(0) else {
        return false;
    };
    let Some(text) = source.get(path.byte_range()) else {
        return false;
    };
    text == "test" || text.ends_with("::test")
}

/// `name` split on underscores and rejoined with spaces, the way
/// `a_missing_block_is_started` reads as "a missing block is started".
fn humanize(name: &str) -> String {
    name.split('_')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether `text` says anything at all: at least one letter or digit.
fn is_readable(text: &str) -> bool {
    text.chars().any(char::is_alphanumeric)
}

/// What [`describe`] returns when neither the doc comment nor the name
/// reads as anything. An added test still has to say which test it is
/// without showing the raw name, so [`render`] adds the file when it
/// meets this exact text.
const NO_DESCRIPTION: &str = "no description";

/// The one-line description for a test: its doc comment when it has one
/// and the doc comment reads as something; otherwise its name turned into
/// words, when that reads as something; otherwise a plain admission that
/// this build could not describe it.
fn describe(doc_lines: &[String], fn_name: &str) -> String {
    let from_doc = doc_lines.join(" ");
    let from_doc = from_doc.trim();
    if is_readable(from_doc) {
        return from_doc.to_string();
    }
    let from_name = humanize(fn_name);
    if is_readable(&from_name) {
        return from_name;
    }
    NO_DESCRIPTION.to_string()
}

/// Walks one list of siblings (a file's top level, or one module's body),
/// tracking the doc comment lines and whether a test attribute has been
/// seen since the last item, and recursing into every nested module with
/// its name added to `path`. Every test found is inserted keyed by its
/// full module path plus function name, so two files can be compared test
/// for test by that key.
fn walk_children(
    parent: Node,
    path: &[String],
    source: &str,
    out: &mut BTreeMap<String, FoundTest>,
) {
    let mut pending_doc: Vec<String> = Vec::new();
    let mut pending_test_attribute = false;
    // The byte offset of the first doc comment or attribute seen since the
    // last item, so a test's compared source can start there: an added
    // `#[ignore]` or an edited doc comment then changes that source, and so
    // counts the test as changed, even though its body did not move.
    let mut pending_start: Option<usize> = None;
    let mut cursor = parent.walk();
    for child in parent.named_children(&mut cursor) {
        match child.kind() {
            "line_comment" | "block_comment" => {
                // A doc comment (`///`, `//!`, or `/** */`) carries an
                // `outer` or `inner` field in the grammar; a plain comment,
                // including a `////`-or-longer banner, carries neither and
                // resets tracking just like any other unrelated node.
                let is_doc = child.child_by_field_name("outer").is_some()
                    || (child.kind() == "line_comment"
                        && child.child_by_field_name("inner").is_some());
                if is_doc {
                    pending_start.get_or_insert(child.start_byte());
                    if let Some(text) = source.get(child.byte_range()).and_then(doc_comment_text) {
                        pending_doc.push(text.to_string());
                    }
                } else {
                    pending_doc.clear();
                    pending_test_attribute = false;
                    pending_start = None;
                }
            }
            "attribute_item" => {
                pending_start.get_or_insert(child.start_byte());
                if is_test_attribute(child, source) {
                    pending_test_attribute = true;
                }
            }
            "function_item" => {
                if pending_test_attribute {
                    if let Some(name) = child
                        .child_by_field_name("name")
                        .and_then(|n| source.get(n.byte_range()))
                    {
                        let mut full_path = path.to_vec();
                        full_path.push(name.to_string());
                        let key = full_path.join("::");
                        let start = pending_start.unwrap_or_else(|| child.start_byte());
                        let entry_source = source
                            .get(start..child.end_byte())
                            .unwrap_or("")
                            .to_string();
                        let description = describe(&pending_doc, name);
                        out.insert(
                            key,
                            FoundTest {
                                source: entry_source,
                                description,
                            },
                        );
                    }
                }
                pending_doc.clear();
                pending_test_attribute = false;
                pending_start = None;
            }
            "mod_item" => {
                if let (Some(name), Some(body)) = (
                    child
                        .child_by_field_name("name")
                        .and_then(|n| source.get(n.byte_range())),
                    child.child_by_field_name("body"),
                ) {
                    let mut full_path = path.to_vec();
                    full_path.push(name.to_string());
                    walk_children(body, &full_path, source, out);
                }
                pending_doc.clear();
                pending_test_attribute = false;
                pending_start = None;
            }
            _ => {
                pending_doc.clear();
                pending_test_attribute = false;
                pending_start = None;
            }
        }
    }
}

/// Every test in `source`, keyed by module path plus function name.
fn collect_tests(tree: &tree_sitter::Tree, source: &str) -> BTreeMap<String, FoundTest> {
    let mut out = BTreeMap::new();
    walk_children(tree.root_node(), &[], source, &mut out);
    out
}

/// One changed `.rs` file's tests on both sides: absent when the file did
/// not exist there, unparsed when the grammar could not read it, or the
/// tests it holds.
enum Side {
    Absent,
    Unparsed,
    Parsed(BTreeMap<String, FoundTest>),
}

/// `path`'s tests at `rev`, or [`Side::Absent`] when `path` is `None`: the
/// side of a rename (or an add/delete) that has no file there at all.
fn read_side(dir: &Path, rev: &str, path: Option<&str>) -> Side {
    let Some(path) = path else {
        return Side::Absent;
    };
    match read_at(dir, rev, path) {
        RawContent::Absent => Side::Absent,
        RawContent::Unparsed => Side::Unparsed,
        RawContent::Text(text) => match parse_rust(&text) {
            Some(tree) => Side::Parsed(collect_tests(&tree, &text)),
            None => Side::Unparsed,
        },
    }
}

/// One changed `.rs` file's group. `base_path` and `head_path` are the same
/// path for an ordinary add, modify, or delete, and the old and new paths
/// for a rename, so a renamed file is compared as one file rather than a
/// deletion plus an unrelated addition. `display_name` is the path and
/// crate the group is reported under.
fn rust_group(
    dir: &Path,
    base: &str,
    head: &str,
    base_path: Option<&str>,
    head_path: Option<&str>,
    display_name: &str,
) -> Group {
    let base_side = read_side(dir, base, base_path);
    let head_side = read_side(dir, head, head_path);
    if matches!(base_side, Side::Unparsed) || matches!(head_side, Side::Unparsed) {
        return Group {
            crate_name: crate_of(display_name),
            file: display_name.to_string(),
            body: GroupBody::Unparsed,
        };
    }
    let empty = BTreeMap::new();
    let base_tests = match &base_side {
        Side::Parsed(tests) => tests,
        Side::Absent | Side::Unparsed => &empty,
    };
    let head_tests = match &head_side {
        Side::Parsed(tests) => tests,
        Side::Absent | Side::Unparsed => &empty,
    };

    let keys: BTreeSet<&String> = base_tests.keys().chain(head_tests.keys()).collect();
    let mut removed = Vec::new();
    let mut added = Vec::new();
    let mut changed = Vec::new();
    for key in keys {
        match (base_tests.get(key), head_tests.get(key)) {
            (Some(b), None) => removed.push(TestEntry {
                name: key.clone(),
                description: b.description.clone(),
            }),
            (None, Some(h)) => added.push(TestEntry {
                name: key.clone(),
                description: h.description.clone(),
            }),
            (Some(b), Some(h)) if b.source != h.source => changed.push(TestEntry {
                name: key.clone(),
                description: h.description.clone(),
            }),
            _ => {}
        }
    }

    Group {
        crate_name: crate_of(display_name),
        file: display_name.to_string(),
        body: GroupBody::Tests {
            removed,
            added,
            changed,
        },
    }
}

/// Builds the test summary for every file that changed between `base` and
/// `head`, by parsing rather than by building or running the change.
///
/// # Errors
/// Returns an error when git cannot list the changed files, or resolve the
/// merge base of `base` and `head`, such as when either does not resolve
/// or the two share no common ancestor.
pub fn summarize(dir: &Path, base: &str, head: &str) -> Result<Summary, TestSummaryError> {
    // The "before" side is read at the merge base, not at `base` itself, so
    // a commit `base` has gained on its own since the two diverged is never
    // read as part of this change. The file list uses the same merge base,
    // since a three-dot `git diff` range resolves it the same way.
    let merge_base =
        crate::git::merge_base(dir, base, head).map_err(|e| TestSummaryError(e.to_string()))?;
    let changes = crate::git::diff_name_status_between(dir, base, head)
        .map_err(|e| TestSummaryError(e.to_string()))?;

    let mut summary = Summary::default();
    for change in changes {
        let display_name = change.display_path();
        if under_test_fixtures(display_name) {
            continue;
        }
        // A rename is judged by each side's own extension, not by
        // `display_name` alone: a `.rs` file renamed away from Rust still
        // has its old tests to report as removed, and a file renamed into
        // `.rs` still has its new tests to report as added. Each side is
        // only ever parsed as Rust when that side's own path is `.rs`.
        let base_is_rust = change.base_path().is_some_and(is_rust_path);
        let head_is_rust = change.head_path().is_some_and(is_rust_path);
        if !base_is_rust && !head_is_rust {
            if looks_like_a_test_file(display_name) {
                summary.groups.push(Group {
                    crate_name: crate_of(display_name),
                    file: display_name.to_string(),
                    body: GroupBody::Unsupported,
                });
            }
            continue;
        }

        let group = rust_group(
            dir,
            &merge_base,
            head,
            change.base_path().filter(|_| base_is_rust),
            change.head_path().filter(|_| head_is_rust),
            display_name,
        );
        match &group.body {
            GroupBody::Tests {
                removed,
                added,
                changed,
            } => {
                if removed.is_empty() && added.is_empty() && changed.is_empty() {
                    continue;
                }
                summary.added += added.len();
                summary.changed += changed.len();
                summary.removed += removed.len();
            }
            GroupBody::Unparsed | GroupBody::Unsupported => {}
        }
        summary.groups.push(group);
    }

    summary
        .groups
        .sort_by(|a, b| (&a.crate_name, &a.file).cmp(&(&b.crate_name, &b.file)));
    Ok(summary)
}

/// Renders a [`Summary`] as the Markdown this build puts in the status
/// block, and what `osf changeset tests` prints on its own: the totals, then
/// one group per file, each with its removed tests first, by name and
/// description, then its added and changed tests by description alone.
/// Only a removed test is ever named: issue #15 shows the raw identifier
/// nowhere else. An added test with no readable description still says
/// which one it is, by naming the file instead of the test.
#[must_use]
pub fn render(summary: &Summary) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "**Tests**: {} added, {} changed, {} removed",
        summary.added, summary.changed, summary.removed
    )
    .expect("writing to a string never fails");

    // One line per crate, above that crate's files: issue #15 asks for two
    // levels, the crate's own totals and then its files underneath.
    let mut crate_totals: BTreeMap<&str, (usize, usize, usize)> = BTreeMap::new();
    for group in &summary.groups {
        if let GroupBody::Tests {
            removed,
            added,
            changed,
        } = &group.body
        {
            let totals = crate_totals.entry(group.crate_name.as_str()).or_default();
            totals.0 += added.len();
            totals.1 += changed.len();
            totals.2 += removed.len();
        }
    }

    let mut current_crate: Option<&str> = None;
    for group in &summary.groups {
        if current_crate != Some(group.crate_name.as_str()) {
            current_crate = Some(group.crate_name.as_str());
            let (a, c, r) = crate_totals
                .get(group.crate_name.as_str())
                .copied()
                .unwrap_or_default();
            writeln!(
                out,
                "- `{}`: {a} added, {c} changed, {r} removed",
                group.crate_name
            )
            .expect("writing to a string never fails");
        }

        match &group.body {
            GroupBody::Tests {
                removed,
                added,
                changed,
            } => {
                writeln!(
                    out,
                    "  - `{}`: {} added, {} changed, {} removed",
                    group.file,
                    added.len(),
                    changed.len(),
                    removed.len()
                )
                .expect("writing to a string never fails");
                for test in removed {
                    writeln!(out, "    - removed `{}`: {}", test.name, test.description)
                        .expect("writing to a string never fails");
                }
                for test in added {
                    if test.description == NO_DESCRIPTION {
                        writeln!(out, "    - added: no description (in {})", group.file)
                            .expect("writing to a string never fails");
                    } else {
                        writeln!(out, "    - added: {}", test.description)
                            .expect("writing to a string never fails");
                    }
                }
                for test in changed {
                    writeln!(out, "    - changed: {}", test.description)
                        .expect("writing to a string never fails");
                }
            }
            GroupBody::Unparsed => {
                writeln!(out, "  - `{}`: unparsed", group.file)
                    .expect("writing to a string never fails");
            }
            GroupBody::Unsupported => {
                writeln!(
                    out,
                    "  - `{}`: not yet supported, see open-software-factory/software-factory#188",
                    group.file
                )
                .expect("writing to a string never fails");
            }
        }
    }

    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway git repository for testing [`read_side`] directly,
    /// small enough not to need the integration tests' shared fixture.
    struct RawRepo {
        dir: std::path::PathBuf,
    }

    impl RawRepo {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("osf-changeset-tests-readat-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("temp repo dir creates");
            let repo = RawRepo { dir };
            repo.git(&["init", "-q", "-b", "main"]);
            repo.git(&["config", "user.email", "test@example.com"]);
            repo.git(&["config", "user.name", "Test"]);
            repo
        }

        fn git(&self, args: &[&str]) -> std::process::Output {
            let output = std::process::Command::new("git")
                .current_dir(&self.dir)
                .args(args)
                .output()
                .expect("git runs");
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            output
        }

        fn write_bytes(&self, path: &str, content: &[u8]) {
            std::fs::write(self.dir.join(path), content).expect("fixture file writes");
        }

        fn commit(&self, message: &str) -> String {
            self.git(&["add", "-A"]);
            self.git(&["commit", "-q", "-m", message]);
            String::from_utf8(self.git(&["rev-parse", "HEAD"]).stdout)
                .expect("a commit hash is ASCII")
                .trim()
                .to_string()
        }
    }

    impl Drop for RawRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn a_path_missing_at_a_commit_reads_as_absent() {
        let repo = RawRepo::new("absent");
        repo.write_bytes("a.rs", b"fn x() {}\n");
        let base = repo.commit("base");
        assert!(matches!(
            read_side(&repo.dir, &base, Some("missing.rs")),
            Side::Absent
        ));
    }

    #[test]
    fn content_that_is_not_valid_utf8_is_unparsed_not_lossy_converted() {
        let repo = RawRepo::new("invalid-utf8");
        repo.write_bytes("a.rs", &[0xFF, 0xFE, b'f', b'n']);
        let base = repo.commit("base");
        assert!(matches!(
            read_side(&repo.dir, &base, Some("a.rs")),
            Side::Unparsed
        ));
    }

    #[test]
    fn a_plain_comment_is_not_a_doc_comment() {
        assert_eq!(doc_comment_text("// just a comment"), None);
    }

    #[test]
    fn an_inner_doc_comment_is_not_an_outer_one() {
        assert_eq!(doc_comment_text("//! module level"), None);
    }

    #[test]
    fn a_banner_of_four_or_more_slashes_is_not_a_doc_comment() {
        assert_eq!(doc_comment_text("//// a banner line"), None);
    }

    #[test]
    fn an_outer_doc_comment_keeps_its_text_trimmed() {
        assert_eq!(doc_comment_text("///   spaced out  "), Some("spaced out"));
    }

    #[test]
    fn humanize_splits_on_underscores() {
        assert_eq!(
            humanize("a_missing_block_is_started"),
            "a missing block is started"
        );
    }

    #[test]
    fn a_name_of_only_underscores_has_no_readable_description() {
        assert_eq!(describe(&[], "___"), "no description");
    }

    #[test]
    fn a_blank_doc_comment_falls_back_to_the_name() {
        assert_eq!(describe(&[String::new()], "a_kept_test"), "a kept test");
    }

    #[test]
    fn crate_of_reads_the_crates_directory_layout() {
        assert_eq!(crate_of("crates/osf/src/status.rs"), "osf");
        assert_eq!(crate_of("README.md"), "(workspace root)");
    }

    #[test]
    fn under_test_fixtures_matches_a_leading_or_nested_path() {
        assert!(under_test_fixtures("tests/fixtures/sample.rs"));
        assert!(under_test_fixtures("crates/osf/tests/fixtures/sample.rs"));
        assert!(!under_test_fixtures("crates/osf/tests/status.rs"));
    }

    #[test]
    fn looks_like_a_test_file_matches_common_test_layouts() {
        assert!(looks_like_a_test_file("service/tests/test_thing.py"));
        assert!(!looks_like_a_test_file("service/src/thing.py"));
    }

    #[test]
    fn looks_like_a_test_file_matches_a_prefix_named_file_at_the_repository_root() {
        assert!(looks_like_a_test_file("test_thing.py"));
        assert!(looks_like_a_test_file("deep/nested/test_thing.py"));
        assert!(!looks_like_a_test_file("contest_winners.py"));
    }

    fn one_added_test_group(name: &str, description: &str) -> Summary {
        Summary {
            added: 1,
            changed: 0,
            removed: 0,
            groups: vec![Group {
                crate_name: "osf".to_string(),
                file: "crates/osf/src/thing.rs".to_string(),
                body: GroupBody::Tests {
                    removed: Vec::new(),
                    added: vec![TestEntry {
                        name: name.to_string(),
                        description: description.to_string(),
                    }],
                    changed: Vec::new(),
                },
            }],
        }
    }

    #[test]
    fn render_writes_an_added_test_by_description_alone() {
        let summary = one_added_test_group("an_added_test", "an added test");
        let text = render(&summary);
        assert!(text.contains("- added: an added test"));
        assert!(!text.contains("an_added_test"), "{text}");
    }

    #[test]
    fn render_names_the_file_when_an_added_test_has_no_description() {
        let summary = one_added_test_group("___", NO_DESCRIPTION);
        let text = render(&summary);
        assert!(text.contains("added: no description (in crates/osf/src/thing.rs)"));
        assert!(!text.contains('_'), "{text}");
    }
}
