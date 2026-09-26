//! Checks each finding's quoted code against the file and line it names: a
//! finding counts only when the text is really there.
//!
//! A quote is kept when its trimmed text is found within [`LINE_TOLERANCE`]
//! lines of the line the finding names, so a model's own line count
//! drifting by a line or two does not lose a real finding. Every line, in
//! the file and in the quote, is compared after trimming leading and
//! trailing whitespace, and line endings are read through [`str::lines`],
//! which reads a trailing `\r` the same as none: a quote typed against an
//! LF checkout still matches a CRLF file, and the other way round. A quote
//! of one line is found as a substring of a candidate line; a quote of
//! several lines is matched as that whole block, line by line, so a quote
//! copied verbatim out of a multi-line snippet still lines up. A path that
//! resolves outside the repository, or that names no file under it, is
//! dropped without reading any line at all.

use crate::answer::AnswerFinding;
use std::path::{Component, Path, PathBuf};

/// How many lines a quote's named line may drift and still be found: the
/// line itself, and up to two lines either side.
const LINE_TOLERANCE: usize = 2;

/// The findings kept because their quote checks out, and the findings
/// dropped, each with the reason.
pub struct Checked {
    pub kept: Vec<AnswerFinding>,
    pub dropped: Vec<(AnswerFinding, String)>,
}

/// Checks every finding in `findings` against the files under `root`.
#[must_use]
pub fn check(root: &Path, findings: &[AnswerFinding]) -> Checked {
    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    for finding in findings {
        match verify(root, finding) {
            Ok(()) => kept.push(finding.clone()),
            Err(reason) => dropped.push((
                finding.clone(),
                format!("{}:{}: {reason}", finding.path, finding.line),
            )),
        }
    }
    Checked { kept, dropped }
}

/// `Ok(())` when `finding`'s quote is really at the file and line it names, `Err` naming why not otherwise.
fn verify(root: &Path, finding: &AnswerFinding) -> Result<(), String> {
    let resolved = resolve_under_root(root, &finding.path)?;
    let content =
        std::fs::read_to_string(&resolved).map_err(|e| format!("could not be read: {e}"))?;
    let file_lines: Vec<&str> = content.lines().collect();
    if quote_found(&file_lines, finding.line, &finding.quote) {
        Ok(())
    } else {
        Err("quoted text not found near this line".to_string())
    }
}

/// `path` resolved under `root`, as a file that exists there without leaving `root`.
fn resolve_under_root(root: &Path, path: &str) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(path.replace('\\', "/"));
    if candidate.is_absolute() {
        return Err("outside the repository".to_string());
    }
    let root_collapsed = lexically_normalize(root);
    let joined_collapsed = lexically_normalize(&root.join(&candidate));
    if !joined_collapsed.starts_with(&root_collapsed) {
        return Err("outside the repository".to_string());
    }
    if !joined_collapsed.is_file() {
        return Err("no such file".to_string());
    }
    Ok(joined_collapsed)
}

/// `path`'s `.` and `..` components collapsed left to right, without touching the filesystem.
fn lexically_normalize(path: &Path) -> PathBuf {
    let mut out: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out.into_iter().collect()
}

/// Whether `quote`, trimmed line by line, is really in `file_lines` within
/// [`LINE_TOLERANCE`] lines of `named_line`.
fn quote_found(file_lines: &[&str], named_line: u64, quote: &str) -> bool {
    let quote_lines: Vec<&str> = quote.lines().map(str::trim).collect();
    if quote_lines.is_empty() || quote_lines.iter().all(|line| line.is_empty()) {
        return false;
    }
    let width = quote_lines.len();
    let joined_quote = quote_lines.join("\n");
    let named_index = usize::try_from(named_line.saturating_sub(1)).unwrap_or(usize::MAX);
    let first = named_index.saturating_sub(LINE_TOLERANCE);
    let last = named_index.saturating_add(LINE_TOLERANCE);

    (first..=last).any(|start| {
        file_lines
            .get(start..start.saturating_add(width))
            .is_some_and(|window| block_matches(window, &joined_quote, width))
    })
}

/// Whether `window`, trimmed line by line, holds `joined_quote`: contained in the one line when `width` is 1, or equal to the whole trimmed block otherwise.
fn block_matches(window: &[&str], joined_quote: &str, width: usize) -> bool {
    if width == 1 {
        window
            .first()
            .is_some_and(|line| line.trim().contains(joined_quote))
    } else {
        let joined_window = window
            .iter()
            .map(|line| line.trim())
            .collect::<Vec<_>>()
            .join("\n");
        joined_window == joined_quote
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::answer::{Action, Severity};
    use crate::test_support::TempDir;

    fn finding(path: &str, line: u64, quote: &str) -> AnswerFinding {
        AnswerFinding {
            path: path.to_string(),
            line,
            quote: quote.to_string(),
            severity: Severity::Minor,
            action: Action::Justify,
            body: "note".to_string(),
        }
    }

    /// A fresh temp repository holding one file, its parent directories created first.
    fn root_with_file(name: &str, file_name: &str, content: &str) -> TempDir {
        let root = TempDir::new(name);
        let path = root.join(file_name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("dirs");
        }
        std::fs::write(&path, content).expect("write");
        root
    }

    #[test]
    fn a_quote_at_its_line_is_kept_and_an_invented_one_is_dropped() {
        let root = root_with_file(
            "quotes-basic",
            "src/lib.rs",
            "fn one() {}\nfn two() {}\nfn three() {}\n",
        );
        let findings = vec![
            finding("src/lib.rs", 2, "fn two() {}"),
            finding("src/lib.rs", 2, "fn invented() {}"),
        ];
        let checked = check(&root, &findings);
        assert_eq!(checked.kept.len(), 1, "{:?}", checked.dropped);
        assert_eq!(checked.dropped.len(), 1);
        let kept = checked.kept.first().expect("one kept");
        assert_eq!(kept.quote, "fn two() {}");
        let (dropped_finding, reason) = checked.dropped.first().expect("one dropped");
        assert_eq!(dropped_finding.quote, "fn invented() {}");
        assert!(reason.contains("src/lib.rs"), "{reason}");
        assert!(reason.contains('2'), "{reason}");
    }

    #[test]
    fn trailing_whitespace_is_tolerated_on_either_side() {
        let root = root_with_file(
            "quotes-trailing-ws",
            "src/lib.rs",
            "fn one() {}\nlet x = 1;   \nfn three() {}\n",
        );
        let findings = vec![finding("src/lib.rs", 2, "let x = 1;")];
        let checked = check(&root, &findings);
        assert_eq!(checked.kept.len(), 1, "{:?}", checked.dropped);
    }

    #[test]
    fn a_crlf_file_matches_an_lf_quote() {
        let root = root_with_file(
            "quotes-crlf",
            "src/lib.rs",
            "fn one() {}\r\nlet y = 2;\r\nfn three() {}\r\n",
        );
        let findings = vec![finding("src/lib.rs", 2, "let y = 2;")];
        let checked = check(&root, &findings);
        assert_eq!(checked.kept.len(), 1, "{:?}", checked.dropped);
    }

    #[test]
    fn a_quote_spanning_several_lines_is_kept() {
        let root = root_with_file(
            "quotes-multiline",
            "src/lib.rs",
            "fn one() {}\nfn two() {\n    let x = 1;\n    x + 1\n}\n",
        );
        let findings = vec![finding(
            "src/lib.rs",
            2,
            "fn two() {\n    let x = 1;\n    x + 1\n}",
        )];
        let checked = check(&root, &findings);
        assert_eq!(checked.kept.len(), 1, "{:?}", checked.dropped);
    }

    #[test]
    fn a_quote_two_lines_off_is_kept_but_three_lines_off_is_dropped() {
        let root = root_with_file(
            "quotes-drift",
            "src/lib.rs",
            "one\ntwo\nthree\nfour\ntarget line\nsix\nseven\neight\nnine\nten\n",
        );
        let close = finding("src/lib.rs", 7, "target line");
        let far = finding("src/lib.rs", 8, "target line");
        let checked = check(&root, &[close, far]);
        assert_eq!(checked.kept.len(), 1, "{:?}", checked.dropped);
        assert_eq!(checked.dropped.len(), 1);
        let kept = checked.kept.first().expect("one kept");
        assert_eq!(kept.line, 7);
        let (dropped_finding, _) = checked.dropped.first().expect("one dropped");
        assert_eq!(dropped_finding.line, 8);
    }

    #[test]
    fn a_quote_of_a_nonexistent_path_is_dropped() {
        let root = TempDir::new("quotes-missing-file");
        let findings = vec![finding("src/missing.rs", 1, "anything")];
        let checked = check(&root, &findings);
        assert_eq!(checked.kept.len(), 0);
        let (_, reason) = checked.dropped.first().expect("one dropped");
        assert!(reason.contains("no such file"), "{reason}");
        assert!(reason.contains("src/missing.rs"), "{reason}");
    }

    #[test]
    fn a_quote_of_a_path_outside_the_repository_is_dropped() {
        let root = TempDir::new("quotes-outside-root");
        let findings = vec![
            finding("../outside.rs", 1, "anything"),
            finding("/etc/passwd", 1, "anything"),
        ];
        let checked = check(&root, &findings);
        assert_eq!(checked.kept.len(), 0);
        assert_eq!(checked.dropped.len(), 2);
        for (_, reason) in &checked.dropped {
            assert!(reason.contains("outside the repository"), "{reason}");
        }
    }
}
