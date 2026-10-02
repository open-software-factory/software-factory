//! `osf pr section write`: replace one marked section of a pull request
//! description in place, or append it when the markers are missing, and
//! leave every other byte of the description untouched.
//!
//! The markers are `<!-- osf:<name>:start -->` and `<!-- osf:<name>:end
//! -->`. Matching them is a plain substring search, never a regular
//! expression, so a section name that happens to hold a regex
//! metacharacter needs no escaping and cannot change what counts as a
//! match.

use regex::{Regex, RegexBuilder};
use std::fmt;
use std::process::Command;
use std::sync::OnceLock;

/// A section write could not be planned or could not reach the pull
/// request. Distinct from the write running and finding nothing to change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionError {
    MarkerCount {
        name: String,
        starts: usize,
        ends: usize,
    },
    Reversed {
        name: String,
    },
    Gh(String),
}

impl fmt::Display for SectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SectionError::MarkerCount { name, starts, ends } => write!(
                f,
                "the description has {starts} start marker(s) and {ends} end marker(s) for \
                 '{name}'; it needs one of each, or neither. Nothing was changed."
            ),
            SectionError::Reversed { name } => write!(
                f,
                "the description's end marker for '{name}' comes before its start marker. \
                 Nothing was changed."
            ),
            SectionError::Gh(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for SectionError {}

fn markers(name: &str) -> (String, String) {
    (
        format!("<!-- osf:{name}:start -->"),
        format!("<!-- osf:{name}:end -->"),
    )
}

/// Every non-overlapping position at which `needle` starts in `haystack`.
fn find_all(haystack: &str, needle: &str) -> Vec<usize> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut positions = Vec::new();
    let mut from = 0;
    while let Some(at) = haystack.get(from..).and_then(|rest| rest.find(needle)) {
        let absolute = from + at;
        positions.push(absolute);
        from = absolute + needle.len();
    }
    positions
}

fn picture_link_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        RegexBuilder::new(r"<a\b[^>]*>\s*(<picture\b[^>]*>.*?</picture>)\s*</a>")
            .dot_matches_new_line(true)
            .build()
            .expect("fixed pattern compiles")
    })
}

/// Removes an `<a ...>` element whose only content, aside from
/// whitespace, is one `<picture>...</picture>`, keeping the `<picture>`
/// itself. A `<picture>` wrapped in a link always shows its light-mode
/// `<img>` fallback, even in dark mode, the way GitHub renders a pull
/// request description; the same `<picture>` on its own switches source
/// correctly. A link that wraps anything else, and a picture with no link
/// around it, are left exactly as they were.
fn unwrap_picture_links(content: &str) -> std::borrow::Cow<'_, str> {
    picture_link_pattern().replace_all(content, "$1")
}

/// Puts `content` into `body` as the section named `name`: replacing the
/// text between the two markers when both are there exactly once, or
/// appending a new section at the end when neither marker is there.
/// `content` is trimmed of leading and trailing whitespace first, matching
/// how a file written by hand usually ends in a trailing newline that
/// should not become a blank line inside the markers.
///
/// Every byte of `body` outside the replaced or appended span is returned
/// unchanged, including its line endings: this never rewrites `\r\n` to
/// `\n` or back.
///
/// # Errors
/// Returns [`SectionError::MarkerCount`] when `body` carries a start marker
/// without a matching end marker, an end marker without a start, or either
/// marker more than once. Returns [`SectionError::Reversed`] when both
/// markers are there but the end marker comes first.
pub fn apply(body: &str, name: &str, content: &str) -> Result<String, SectionError> {
    let (start, end) = markers(name);
    let unwrapped = unwrap_picture_links(content);
    let trimmed = unwrapped.trim();
    let section = format!("{start}\n{trimmed}\n{end}");

    let starts = find_all(body, &start);
    let ends = find_all(body, &end);

    match (starts.as_slice(), ends.as_slice()) {
        ([], []) => {
            let trimmed_body = body.trim_end();
            let separator = if trimmed_body.is_empty() { "" } else { "\n\n" };
            Ok(format!("{trimmed_body}{separator}{section}\n"))
        }
        (&[start_at], &[end_at]) => {
            if start_at >= end_at {
                return Err(SectionError::Reversed {
                    name: name.to_string(),
                });
            }
            let end_of_end = end_at + end.len();
            let mut result = String::with_capacity(body.len() + section.len());
            result.push_str(body.get(..start_at).unwrap_or_default());
            result.push_str(&section);
            result.push_str(body.get(end_of_end..).unwrap_or_default());
            Ok(result)
        }
        (s, e) => Err(SectionError::MarkerCount {
            name: name.to_string(),
            starts: s.len(),
            ends: e.len(),
        }),
    }
}

fn repo_args(repo: Option<&str>) -> Vec<String> {
    match repo {
        Some(r) => vec!["--repo".to_string(), r.to_string()],
        None => Vec::new(),
    }
}

/// Reads a pull request's description through `gh`, using its own detection
/// of the repository when `repo` is `None`.
///
/// # Errors
/// Returns an error when `gh` cannot run or exits non-zero.
pub fn fetch_body(repo: Option<&str>, pr: u64) -> Result<String, SectionError> {
    let mut args = vec!["pr".to_string(), "view".to_string(), pr.to_string()];
    args.extend(repo_args(repo));
    args.extend([
        "--json".to_string(),
        "body".to_string(),
        "--jq".to_string(),
        ".body".to_string(),
    ]);
    let output = Command::new("gh")
        .args(&args)
        .output()
        .map_err(|e| SectionError::Gh(format!("cannot run gh: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SectionError::Gh(format!(
            "gh pr view failed for pull request #{pr}: {}",
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Writes a pull request's description back through `gh`, via a temporary
/// file, the same way `osf pr status apply` does.
///
/// # Errors
/// Returns an error when the temporary file cannot be written, or `gh`
/// cannot run or exits non-zero.
pub fn write_body(repo: Option<&str>, pr: u64, body: &str) -> Result<(), SectionError> {
    let tmp = std::env::temp_dir().join(format!("osf-pr-section-{}.md", std::process::id()));
    std::fs::write(&tmp, body)
        .map_err(|e| SectionError::Gh(format!("cannot write a temporary file: {e}")))?;
    let mut args = vec!["pr".to_string(), "edit".to_string(), pr.to_string()];
    args.extend(repo_args(repo));
    args.extend([
        "--body-file".to_string(),
        tmp.to_string_lossy().into_owned(),
    ]);
    let run = Command::new("gh").args(&args).output();
    let _ = std::fs::remove_file(&tmp);
    let output = run.map_err(|e| SectionError::Gh(format!("cannot run gh: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SectionError::Gh(format!(
            "gh pr edit failed for pull request #{pr}: {}",
            stderr.trim()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_the_text_between_existing_markers() {
        let body = "Intro.\n\n<!-- osf:outline:start -->\nold outline\n<!-- osf:outline:end -->\n\nTail.\n";
        let out = apply(body, "outline", "new outline\n").expect("replaces");
        assert_eq!(
            out,
            "Intro.\n\n<!-- osf:outline:start -->\nnew outline\n<!-- osf:outline:end -->\n\nTail.\n"
        );
    }

    #[test]
    fn appends_a_new_section_when_markers_are_absent() {
        let body = "Intro.\n\nTail.\n";
        let out = apply(body, "outline", "the outline\n").expect("appends");
        assert_eq!(
            out,
            "Intro.\n\nTail.\n\n<!-- osf:outline:start -->\nthe outline\n<!-- osf:outline:end -->\n"
        );
    }

    #[test]
    fn appends_with_no_separator_when_the_body_is_empty() {
        let out = apply("", "outline", "the outline").expect("appends");
        assert_eq!(
            out,
            "<!-- osf:outline:start -->\nthe outline\n<!-- osf:outline:end -->\n"
        );
    }

    #[test]
    fn a_start_marker_with_no_end_marker_is_refused() {
        let body = "<!-- osf:outline:start -->\nstray\n";
        let err = apply(body, "outline", "x").expect_err("refused");
        assert_eq!(
            err,
            SectionError::MarkerCount {
                name: "outline".to_string(),
                starts: 1,
                ends: 0,
            }
        );
    }

    #[test]
    fn a_marker_repeated_is_refused() {
        let body = "<!-- osf:outline:start -->\na\n<!-- osf:outline:end -->\n<!-- osf:outline:start -->\nb\n<!-- osf:outline:end -->\n";
        let err = apply(body, "outline", "x").expect_err("refused");
        assert_eq!(
            err,
            SectionError::MarkerCount {
                name: "outline".to_string(),
                starts: 2,
                ends: 2,
            }
        );
    }

    #[test]
    fn reversed_markers_are_refused() {
        let body = "<!-- osf:outline:end -->\nstray\n<!-- osf:outline:start -->\n";
        let err = apply(body, "outline", "x").expect_err("refused");
        assert_eq!(
            err,
            SectionError::Reversed {
                name: "outline".to_string(),
            }
        );
    }

    #[test]
    fn text_outside_the_markers_is_byte_identical() {
        let body = "Header with trailing spaces.  \n\n<!-- osf:outline:start -->\nold\n<!-- osf:outline:end -->\n\nFooter with a tab\there.\n";
        let out = apply(body, "outline", "new").expect("replaces");
        assert!(out.starts_with("Header with trailing spaces.  \n\n"));
        assert!(out.ends_with("\n\nFooter with a tab\there.\n"));
    }

    #[test]
    fn a_name_with_regex_characters_is_matched_literally() {
        let name = "a.b*c(d)";
        let body =
            format!("before\n\n<!-- osf:{name}:start -->\nold\n<!-- osf:{name}:end -->\n\nafter\n");
        let out = apply(&body, name, "new").expect("replaces");
        assert!(out.contains("new"));
        assert!(!out.contains("old"));
        assert!(out.starts_with("before\n\n"));
        assert!(out.ends_with("\n\nafter\n"));
    }

    #[test]
    fn a_name_with_regex_characters_does_not_match_a_different_name() {
        // If the name were used as a regex instead of a literal, "a.b" would
        // also match "aXb". It must not.
        let body = "<!-- osf:a.b:start -->\nkeep\n<!-- osf:a.b:end -->\n";
        let out = apply(body, "aXb", "new").expect("appends, finding no existing markers");
        assert!(out.contains("keep"), "{out}");
        assert!(out.contains("<!-- osf:aXb:start -->"), "{out}");
    }

    #[test]
    fn a_crlf_body_keeps_its_line_endings_outside_the_section() {
        let body = "Intro.\r\n\r\n<!-- osf:outline:start -->\r\nold\r\n<!-- osf:outline:end -->\r\n\r\nTail.\r\n";
        let out = apply(body, "outline", "new").expect("replaces");
        assert!(out.starts_with("Intro.\r\n\r\n"));
        assert!(out.ends_with("\r\n\r\nTail.\r\n"));
        assert!(out.contains("new"));
        assert!(!out.contains("old"));
    }

    #[test]
    fn a_crlf_body_with_no_markers_appends_a_new_section() {
        let body = "Intro.\r\nTail.\r\n";
        let out = apply(body, "outline", "new").expect("appends");
        assert!(out.starts_with("Intro.\r\nTail."));
        assert!(out.ends_with("<!-- osf:outline:end -->\n"));
    }

    #[test]
    fn applying_twice_is_idempotent() {
        let body = "Intro.\n";
        let once = apply(body, "outline", "the outline").expect("first apply");
        let twice = apply(&once, "outline", "the outline").expect("second apply");
        assert_eq!(once, twice);
    }

    #[test]
    fn a_wrapped_picture_is_unwrapped() {
        let content = r#"<a href="dark.svg"><picture><source media="(prefers-color-scheme: dark)" srcset="dark.svg"><img src="light.svg"></picture></a>"#;
        let out = unwrap_picture_links(content);
        assert_eq!(
            out,
            r#"<picture><source media="(prefers-color-scheme: dark)" srcset="dark.svg"><img src="light.svg"></picture>"#
        );
    }

    #[test]
    fn two_wrapped_pictures_are_both_unwrapped() {
        let content = r#"<a href="a.svg"><picture><img src="a.svg"></picture></a> and <a href="b.svg"><picture><img src="b.svg"></picture></a>"#;
        let out = unwrap_picture_links(content);
        assert_eq!(
            out,
            r#"<picture><img src="a.svg"></picture> and <picture><img src="b.svg"></picture>"#
        );
    }

    #[test]
    fn a_link_with_other_content_stays() {
        let content =
            r#"<a href="a.svg">see the diagram <picture><img src="a.svg"></picture> above</a>"#;
        let out = unwrap_picture_links(content);
        assert_eq!(out, content);
    }

    #[test]
    fn a_picture_without_a_link_is_unchanged() {
        let content = r#"<picture><source srcset="dark.svg"><img src="light.svg"></picture>"#;
        let out = unwrap_picture_links(content);
        assert_eq!(out, content);
    }

    #[test]
    fn a_wrapped_picture_may_have_whitespace_around_it_inside_the_link() {
        let content = "<a href=\"a.svg\">\n  <picture><img src=\"a.svg\"></picture>\n</a>";
        let out = unwrap_picture_links(content);
        assert_eq!(out, r#"<picture><img src="a.svg"></picture>"#);
    }

    #[test]
    fn unwrapping_a_picture_link_in_the_content_never_touches_text_outside_the_section() {
        let body = "Header text with an <a href=\"x.svg\">unrelated link</a>.\n\n<!-- osf:pr-lens:start -->\nold\n<!-- osf:pr-lens:end -->\n\nFooter.\n";
        let content = r#"<a href="a.svg"><picture><img src="a.svg"></picture></a>"#;
        let out = apply(body, "pr-lens", content).expect("replaces");
        assert!(out.starts_with("Header text with an <a href=\"x.svg\">unrelated link</a>.\n\n"));
        assert!(out.ends_with("\n\nFooter.\n"));
        assert!(out.contains("<picture><img src=\"a.svg\"></picture>"));
        assert!(!out.contains("<a href=\"a.svg\">"));
    }
}
