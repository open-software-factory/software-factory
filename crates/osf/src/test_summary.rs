//! `osf status tests`: a summary of Rust tests added, changed and removed
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

/// A `status tests` run could not finish: a bad ref, or git failing to
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

/// Whether `path` looks like a test file in some language, by the same
/// name and location patterns [`crate::risk`] already uses to keep tests
/// out of the blast-radius count.
fn looks_like_a_test_file(path: &str) -> bool {
    RegexBuilder::new(crate::risk::TEST_FILES)
        .case_insensitive(true)
        .build()
        .expect("built-in test-file pattern compiles")
        .is_match(path)
}

/// `path` as it reads at `rev`, or `None` when it does not exist there
/// (added since, or removed since, depending on which side is asked).
fn read_at(dir: &Path, rev: &str, path: &str) -> Option<String> {
    let bytes = crate::git::content_at(dir, rev, path).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
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
    "no description".to_string()
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
    let mut cursor = parent.walk();
    for child in parent.named_children(&mut cursor) {
        match child.kind() {
            "line_comment" => {
                if let Some(text) = source.get(child.byte_range()).and_then(doc_comment_text) {
                    pending_doc.push(text.to_string());
                }
            }
            "attribute_item" => {
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
                        let entry_source = source.get(child.byte_range()).unwrap_or("").to_string();
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
            }
            _ => {
                pending_doc.clear();
                pending_test_attribute = false;
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

fn read_side(dir: &Path, rev: &str, path: &str) -> Side {
    let Some(text) = read_at(dir, rev, path) else {
        return Side::Absent;
    };
    match parse_rust(&text) {
        Some(tree) => Side::Parsed(collect_tests(&tree, &text)),
        None => Side::Unparsed,
    }
}

/// One changed `.rs` file's group, or `None` when neither side reads as
/// having any test change worth reporting.
fn rust_group(dir: &Path, base: &str, head: &str, path: &str) -> Group {
    let base_side = read_side(dir, base, path);
    let head_side = read_side(dir, head, path);
    if matches!(base_side, Side::Unparsed) || matches!(head_side, Side::Unparsed) {
        return Group {
            crate_name: crate_of(path),
            file: path.to_string(),
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
        crate_name: crate_of(path),
        file: path.to_string(),
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
/// Returns an error when git cannot list the changed files, such as when
/// `base` or `head` does not resolve.
pub fn summarize(dir: &Path, base: &str, head: &str) -> Result<Summary, TestSummaryError> {
    let files = crate::git::diff_name_only_between(dir, base, head)
        .map_err(|e| TestSummaryError(e.to_string()))?;

    let mut summary = Summary::default();
    for path in files {
        if under_test_fixtures(&path) {
            continue;
        }
        if !Path::new(&path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
        {
            if looks_like_a_test_file(&path) {
                summary.groups.push(Group {
                    crate_name: crate_of(&path),
                    file: path,
                    body: GroupBody::Unsupported,
                });
            }
            continue;
        }

        let group = rust_group(dir, base, head, &path);
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
/// block, and what `osf status tests` prints on its own: the totals, then
/// one group per file, each with its removed tests first, then its added
/// tests, then its changed tests named by description alone.
#[must_use]
pub fn render(summary: &Summary) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "**Tests**: {} added, {} changed, {} removed",
        summary.added, summary.changed, summary.removed
    )
    .expect("writing to a string never fails");

    for group in &summary.groups {
        match &group.body {
            GroupBody::Tests {
                removed,
                added,
                changed,
            } => {
                writeln!(
                    out,
                    "- `{}` ({}): {} added, {} changed, {} removed",
                    group.file,
                    group.crate_name,
                    added.len(),
                    changed.len(),
                    removed.len()
                )
                .expect("writing to a string never fails");
                for test in removed {
                    writeln!(out, "  - removed `{}`: {}", test.name, test.description)
                        .expect("writing to a string never fails");
                }
                for test in added {
                    writeln!(out, "  - added `{}`: {}", test.name, test.description)
                        .expect("writing to a string never fails");
                }
                for test in changed {
                    writeln!(out, "  - changed: {}", test.description)
                        .expect("writing to a string never fails");
                }
            }
            GroupBody::Unparsed => {
                writeln!(out, "- `{}` ({}): unparsed", group.file, group.crate_name)
                    .expect("writing to a string never fails");
            }
            GroupBody::Unsupported => {
                writeln!(
                    out,
                    "- `{}`: not yet supported, see open-software-factory/software-factory#188",
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
}
