//! `osf pr tree render`: the collapsed file table for the `osf:tree` block of
//! a pull request description. Each row has coloured count chips and a
//! log-scale size bar, drawn with GitHub's maths support, so a reviewer sees
//! where the change is big before reading a path. Rendering is a pure
//! function of the change list, so the same changes always give the same
//! bytes. The layout rules live in `.github/PULL_REQUEST_TEMPLATE.md`.

use crate::git::{self, ChangedPath, GitError};
use regex::{Regex, RegexBuilder};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

/// At most this many files get one row each; more are grouped by component.
const FLAT_LIMIT: usize = 15;
/// The widest size bar, in pixels, for the row with the most changed lines.
const MAX_BAR: f64 = 80.0;
/// Directory levels that name a component.
const COMPONENT_DEPTH: usize = 3;

const ADDED_BAR: &str = "033a16";
const ADDED_TEXT: &str = "aff5b4";
const REMOVED_BAR: &str = "67060c";
const REMOVED_TEXT: &str = "ffdcd7";

const DOC_OR_INFRA: &str = r"(?i)\.(md|txt|rst|adoc|ya?ml|toml|json|lock)$|^(docs|\.github|\.devcontainer)/|(^|/)Dockerfile[^/]*$";

/// How one file changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Added,
    Modified,
    Deleted,
    Renamed,
}

impl Status {
    fn letter(self) -> char {
        match self {
            Status::Added => 'A',
            Status::Modified => 'M',
            Status::Deleted => 'D',
            Status::Renamed => 'R',
        }
    }
}

/// One changed file and its line counts. A binary file has two zeros.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub status: Status,
    pub added: u64,
    pub removed: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Code,
    Tests,
    Docs,
}

impl Group {
    const ALL: [Group; 3] = [Group::Code, Group::Tests, Group::Docs];

    fn title(self) -> &'static str {
        match self {
            Group::Code => "Code",
            Group::Tests => "Tests",
            Group::Docs => "Docs, build and infra",
        }
    }

    fn lower(self) -> &'static str {
        match self {
            Group::Code => "code",
            Group::Tests => "tests",
            Group::Docs => "docs, build and infra",
        }
    }
}

/// Sorts a path into one of the three groups, by name and location.
struct Classifier {
    tests: Regex,
    docs: Regex,
}

impl Classifier {
    fn new() -> Self {
        Classifier {
            tests: RegexBuilder::new(crate::changeset_risk::TEST_FILES)
                .case_insensitive(true)
                .build()
                .expect("built-in test-file pattern compiles"),
            docs: Regex::new(DOC_OR_INFRA).expect("fixed pattern"),
        }
    }

    fn group(&self, path: &str) -> Group {
        if self.tests.is_match(path) {
            Group::Tests
        } else if self.docs.is_match(path) {
            Group::Docs
        } else {
            Group::Code
        }
    }
}

/// One table row: a file, or a component holding several files.
struct Row {
    label: String,
    files: usize,
    status: Status,
    added: u64,
    removed: u64,
}

impl Row {
    fn total(&self) -> u64 {
        self.added + self.removed
    }
}

/// The directory of `path`, cut to [`COMPONENT_DEPTH`] levels; a file at the
/// root is its own component.
fn component_of(path: &str) -> String {
    let mut parts: Vec<&str> = path.split('/').collect();
    let file = parts.pop().unwrap_or(path);
    if parts.is_empty() {
        return file.to_string();
    }
    parts.truncate(COMPONENT_DEPTH);
    parts.join("/")
}

fn rows_for(files: &[&FileChange], flat: bool) -> Vec<Row> {
    if flat {
        let mut rows: Vec<Row> = files
            .iter()
            .map(|f| Row {
                label: f.path.clone(),
                files: 1,
                status: f.status,
                added: f.added,
                removed: f.removed,
            })
            .collect();
        rows.sort_by(|a, b| a.label.cmp(&b.label));
        return rows;
    }
    let mut by_component: Vec<Row> = Vec::new();
    for f in files {
        let label = component_of(&f.path);
        if let Some(row) = by_component.iter_mut().find(|r| r.label == label) {
            row.files += 1;
            row.added += f.added;
            row.removed += f.removed;
        } else {
            by_component.push(Row {
                label,
                files: 1,
                status: f.status,
                added: f.added,
                removed: f.removed,
            });
        }
    }
    by_component.sort_by(|a, b| {
        b.total()
            .cmp(&a.total())
            .then_with(|| a.label.cmp(&b.label))
    });
    by_component
}

fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// A count chip: the number on a dark ground, as wide as its text. A zero
/// count is a dot.
fn chip(sign: char, n: u64, ground: &str, text_colour: &str) -> String {
    if n == 0 {
        return "·".to_string();
    }
    let text = format!("{sign}{}", thousands(n));
    let width = 8 * text.chars().count() + 10;
    format!(
        r"$\rlap{{\color{{#{ground}}}{{\rule[-5px]{{{width}px}}{{18px}}}}}}\hspace{{5px}}\color{{#{text_colour}}}{{\texttt{{{text}}}}}$"
    )
}

/// The width of the whole size bar, on a log scale against the biggest row.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn bar_width(total: u64, biggest: u64) -> u64 {
    let scale = |n: u64| f64::from(u32::try_from(n).unwrap_or(u32::MAX)).ln_1p();
    (MAX_BAR * scale(total) / scale(biggest)).round() as u64
}

/// The size cell: an added bar and a removed bar, split by their share of
/// the changed lines. A bar that rounds to nothing is left out.
fn size_cell(added: u64, removed: u64, biggest: u64) -> String {
    let total = added + removed;
    if total == 0 {
        return "·".to_string();
    }
    let width = bar_width(total, biggest);
    let added_px = (width * added * 2 + total) / (total * 2);
    let removed_px = width - added_px;
    let mut out = String::from("$");
    if added_px > 0 {
        write!(
            out,
            r"\color{{#{ADDED_BAR}}}{{\rule{{{added_px}px}}{{8px}}}}"
        )
        .expect("writing to a string never fails");
    }
    if added_px > 0 && removed_px > 0 {
        out.push_str(r"\hspace{2px}");
    }
    if removed_px > 0 {
        write!(
            out,
            r"\color{{#{REMOVED_BAR}}}{{\rule{{{removed_px}px}}{{8px}}}}"
        )
        .expect("writing to a string never fails");
    }
    out.push('$');
    out
}

fn totals(rows: &[Row]) -> (usize, u64, u64) {
    rows.iter().fold((0, 0, 0), |(n, a, r), row| {
        (n + row.files, a + row.added, r + row.removed)
    })
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("{n} {word}")
    } else {
        format!("{n} {word}s")
    }
}

fn path_cell(path: &str, previous_dir: &mut String) -> String {
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
    let dir = if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };
    let cell = if dir.is_empty() || dir == *previous_dir {
        format!("`{name}`")
    } else {
        format!("<sub>{dir}</sub><br>`{name}`")
    };
    *previous_dir = dir;
    cell
}

fn write_table(out: &mut String, rows: &[Row], flat: bool, biggest: u64) {
    if flat {
        out.push_str("| File | Change | Added | Removed | Size |\n|:--|:-:|--:|--:|:--|\n");
    } else {
        out.push_str("| Component | Files | Added | Removed | Size |\n|:--|--:|--:|--:|:--|\n");
    }
    let mut previous_dir = String::new();
    for row in rows {
        let (first, second) = if flat {
            (
                path_cell(&row.label, &mut previous_dir),
                row.status.letter().to_string(),
            )
        } else {
            (format!("`{}`", row.label), row.files.to_string())
        };
        writeln!(
            out,
            "| {first} | {second} | {} | {} | {} |",
            chip('+', row.added, ADDED_BAR, ADDED_TEXT),
            chip('-', row.removed, REMOVED_BAR, REMOVED_TEXT),
            size_cell(row.added, row.removed, biggest),
        )
        .expect("writing to a string never fails");
    }
}

/// Renders the collapsed file table, or an empty string when nothing changed.
/// Up to 15 files get one row each; more get one row per component, biggest
/// first. Files fall into Code, Tests, and Docs, build and infra.
#[must_use]
pub fn render(changes: &[FileChange]) -> String {
    if changes.is_empty() {
        return String::new();
    }
    let flat = changes.len() <= FLAT_LIMIT;
    let classifier = Classifier::new();
    let groups: Vec<(Group, Vec<Row>)> = Group::ALL
        .iter()
        .filter_map(|&g| {
            let members: Vec<&FileChange> = changes
                .iter()
                .filter(|c| classifier.group(&c.path) == g)
                .collect();
            (!members.is_empty()).then(|| (g, rows_for(&members, flat)))
        })
        .collect();
    let biggest = groups
        .iter()
        .flat_map(|(_, rows)| rows.iter().map(Row::total))
        .max()
        .unwrap_or(1)
        .max(1);

    let counts: Vec<String> = groups
        .iter()
        .map(|(g, rows)| format!("{} {}", totals(rows).0, g.lower()))
        .collect();
    let mut out = format!(
        "<details>\n<summary><b>All {}</b>: {}</summary>\n\n",
        plural(changes.len(), "file"),
        counts.join(", ")
    );
    for (group, rows) in &groups {
        let (n, added, removed) = totals(rows);
        write!(
            out,
            "**{}** ({}, +{} −{})\n\n",
            group.title(),
            plural(n, "file"),
            thousands(added),
            thousands(removed)
        )
        .expect("writing to a string never fails");
        write_table(&mut out, rows, flat, biggest);
        out.push('\n');
    }
    if flat {
        out.push_str(
            "<sub>A added · M modified · D deleted · R renamed. Size: log scale of lines changed.</sub>\n",
        );
    } else {
        writeln!(
            out,
            "<sub>Grouped by component because the PR changes more than {FLAT_LIMIT} files. Size: log scale of lines changed.</sub>"
        )
        .expect("writing to a string never fails");
    }
    out.push_str("\n</details>\n");
    out
}

/// Reads the changes between `base` and `head`, at their merge base, with
/// line counts from `git diff --numstat`.
///
/// # Errors
/// Returns an error when git cannot run or `base` or `head` does not resolve.
pub fn collect(dir: &Path, base: &str, head: &str) -> Result<Vec<FileChange>, GitError> {
    let statuses: HashMap<String, Status> = git::diff_name_status_between(dir, base, head)?
        .into_iter()
        .map(|change| {
            let status = match &change {
                ChangedPath::Added(_) => Status::Added,
                ChangedPath::Modified(_) => Status::Modified,
                ChangedPath::Deleted(_) => Status::Deleted,
                ChangedPath::Renamed { .. } => Status::Renamed,
            };
            (change.display_path().to_string(), status)
        })
        .collect();
    let counts = git::diff_numstat_between(dir, base, head)?;
    Ok(counts
        .into_iter()
        .map(|c| FileChange {
            status: statuses.get(&c.path).copied().unwrap_or(Status::Modified),
            path: c.path,
            added: c.added,
            removed: c.removed,
        })
        .collect())
}
