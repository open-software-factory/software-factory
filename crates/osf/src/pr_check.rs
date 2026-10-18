//! `osf pr check`: the structural checks on a pull request's title and
//! description, and the hand-written text the prose and hygiene checks then
//! read.
//!
//! The description holds blocks a tool writes between
//! `<!-- osf:NAME:start ... -->` and `<!-- osf:NAME:end -->` markers, see
//! [`crate::marker`]. Those blocks are cut out before anything else reads the
//! text, so a tool's output is never judged as if a person wrote it.
//!
//! Coverage:
//! - Checks: the title is not empty; the description holds every `## `
//!   heading the pull request template holds outside its comments; each
//!   tool-written block opens and closes exactly once, or not at all.
//! - Reads the template for both lists. The heading list and the block
//!   names come from it, never from a copy in this file.
//! - Does not check: that the text under a heading is right, that a block
//!   between markers was really written by a tool, the stack footer, or
//!   the prose and hygiene of the text. `osf scan` and `osf lint writing`
//!   check those on the files this command writes.

use crate::marker;
use std::fmt;

/// The title and description with every tool-written block cut out.
#[derive(Debug, PartialEq, Eq)]
pub struct HandWritten {
    pub title: String,
    pub body: String,
}

/// One thing wrong with a pull request's title or description.
#[derive(Debug, PartialEq, Eq)]
pub enum Problem {
    EmptyTitle,
    MissingHeading(String),
    BlockMarkers {
        name: String,
        starts: usize,
        ends: usize,
    },
    BlockReversed(String),
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problem::EmptyTitle => f.write_str("the title is empty"),
            Problem::MissingHeading(h) => write!(
                f,
                "the description has no `{h}` heading, which the pull request template requires"
            ),
            Problem::BlockMarkers { name, starts, ends } => write!(
                f,
                "the description has {starts} start marker(s) and {ends} end marker(s) for the \
                 '{name}' block; it needs one of each, or neither"
            ),
            Problem::BlockReversed(name) => write!(
                f,
                "the description's end marker for the '{name}' block comes before its start marker"
            ),
        }
    }
}

/// Removes every `<!-- ... -->` comment and every fenced code block, so a
/// heading inside either does not count. Returns the remaining lines.
fn visible_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_comment = false;
    let mut fence: Option<String> = None;
    for line in text.lines() {
        let mut kept = String::new();
        let mut rest = line;
        loop {
            if in_comment {
                match rest.find("-->") {
                    Some(at) => {
                        in_comment = false;
                        rest = rest.get(at + 3..).unwrap_or_default();
                    }
                    None => break,
                }
            } else if let Some(at) = rest.find("<!--") {
                kept.push_str(rest.get(..at).unwrap_or_default());
                in_comment = true;
                rest = rest.get(at + 4..).unwrap_or_default();
            } else {
                kept.push_str(rest);
                break;
            }
        }
        let trimmed = kept.trim_start();
        let mark = ["```", "~~~"].iter().find(|m| trimmed.starts_with(**m));
        match (&fence, mark) {
            (Some(open), Some(m)) if open == m => fence = None,
            (None, Some(m)) => fence = Some((*m).to_string()),
            (None, None) => out.push(kept),
            _ => {}
        }
    }
    out
}

/// Every second-level heading in `text` outside a comment or a code fence,
/// in order, with trailing whitespace removed.
#[must_use]
pub fn headings(text: &str) -> Vec<String> {
    visible_lines(text)
        .iter()
        .filter(|l| l.starts_with("## "))
        .map(|l| l.trim_end().to_string())
        .collect()
}

/// The names of the blocks a tool writes, read from the start markers the
/// template holds: `<!-- osf:NAME:start ... -->`.
#[must_use]
pub fn generated_block_names(template: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (at, _) in template.match_indices("<!-- osf:") {
        let rest = template.get(at + "<!-- osf:".len()..).unwrap_or_default();
        let Some((name, after)) = rest.split_once(':') else {
            continue;
        };
        if after.starts_with("start") && !name.is_empty() && !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
    }
    names
}

/// `body` with every block named in `names` cut out, each replaced by one
/// line break so the text around it stays apart.
///
/// # Errors
/// Returns a [`Problem`] when a block's markers do not pair up once each, or
/// its end marker comes first.
pub fn strip_generated(body: &str, names: &[String]) -> Result<String, Problem> {
    let mut text = body.to_string();
    for name in names {
        let starts = marker::find_starts(&text, name);
        let ends = marker::find_ends(&text, name);
        match (starts.as_slice(), ends.as_slice()) {
            ([], []) => {}
            (&[(start_at, _)], &[(end_at, end_len)]) => {
                if start_at >= end_at {
                    return Err(Problem::BlockReversed(name.clone()));
                }
                let tail = text.get(end_at + end_len..).unwrap_or_default();
                text = format!("{}\n{tail}", text.get(..start_at).unwrap_or_default());
            }
            _ => {
                return Err(Problem::BlockMarkers {
                    name: name.clone(),
                    starts: starts.len(),
                    ends: ends.len(),
                })
            }
        }
    }
    Ok(text)
}

/// Checks the title and description against the template, and returns the
/// hand-written text for the prose and hygiene checks.
///
/// # Errors
/// Returns every [`Problem`] found. A malformed block stops the heading
/// check, since the text it would read is not known.
pub fn check(title: &str, body: &str, template: &str) -> Result<HandWritten, Vec<Problem>> {
    let mut problems = Vec::new();
    if title.trim().is_empty() {
        problems.push(Problem::EmptyTitle);
    }
    let names = generated_block_names(template);
    let hand = match strip_generated(body, &names) {
        Ok(h) => h,
        Err(p) => {
            problems.push(p);
            return Err(problems);
        }
    };
    let present = headings(&hand);
    for required in headings(template) {
        if !present.contains(&required) {
            problems.push(Problem::MissingHeading(required));
        }
    }
    if problems.is_empty() {
        Ok(HandWritten {
            title: title.to_string(),
            body: hand,
        })
    } else {
        Err(problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE: &str = "\
Issue: link

## What and why

<!--
## Not a heading
-->

<!-- osf:pr-lens:start head=<sha> -->
<!-- osf:pr-lens:end -->

```
## Also not a heading
```

<!-- osf:status:start head=<sha> -->
<!-- osf:status:end -->

## Testing notes
";

    fn good_body() -> String {
        "Issue: x\n\n## What and why\n\nText.\n\n## Testing notes\n\nMore.\n".to_string()
    }

    #[test]
    fn headings_skip_comments_and_fences() {
        assert_eq!(
            headings(TEMPLATE),
            vec!["## What and why", "## Testing notes"]
        );
    }

    #[test]
    fn block_names_come_from_the_template() {
        assert_eq!(generated_block_names(TEMPLATE), vec!["pr-lens", "status"]);
    }

    #[test]
    fn a_good_description_passes() {
        let ok = check("Add a check", &good_body(), TEMPLATE).expect("passes");
        assert_eq!(ok.title, "Add a check");
    }

    #[test]
    fn an_empty_title_fails() {
        let err = check("  ", &good_body(), TEMPLATE).expect_err("fails");
        assert_eq!(err, vec![Problem::EmptyTitle]);
    }

    #[test]
    fn a_missing_heading_fails() {
        let body = "## What and why\n\nText.\n";
        let err = check("T", body, TEMPLATE).expect_err("fails");
        assert_eq!(
            err,
            vec![Problem::MissingHeading("## Testing notes".to_string())]
        );
    }

    #[test]
    fn a_heading_inside_a_comment_does_not_count() {
        let body = "## What and why\n\n<!--\n## Testing notes\n-->\n";
        assert!(check("T", body, TEMPLATE).is_err());
    }

    #[test]
    fn a_null_body_fails_for_every_heading() {
        let err = check("T", "", TEMPLATE).expect_err("fails");
        assert_eq!(err.len(), 2);
    }

    #[test]
    fn a_tool_block_is_cut_out() {
        let body = format!(
            "{}\n<!-- osf:status:start head=abc -->\nsecret text\n<!-- osf:status:end -->\n",
            good_body()
        );
        let ok = check("T", &body, TEMPLATE).expect("passes");
        assert!(!ok.body.contains("secret text"));
        assert!(ok.body.contains("More."));
    }

    #[test]
    fn a_heading_inside_a_tool_block_does_not_count() {
        let body = "## What and why\n\n<!-- osf:status:start head=abc -->\n## Testing notes\n<!-- osf:status:end -->\n";
        let err = check("T", body, TEMPLATE).expect_err("fails");
        assert_eq!(
            err,
            vec![Problem::MissingHeading("## Testing notes".to_string())]
        );
    }

    #[test]
    fn an_unclosed_block_fails() {
        let body = format!("{}<!-- osf:status:start head=abc -->\ntext\n", good_body());
        let err = check("T", &body, TEMPLATE).expect_err("fails");
        assert!(matches!(
            &err[..],
            [Problem::BlockMarkers {
                starts: 1,
                ends: 0,
                ..
            }]
        ));
    }

    #[test]
    fn a_reversed_block_fails() {
        let body = format!(
            "{}<!-- osf:status:end -->\n<!-- osf:status:start head=abc -->\n",
            good_body()
        );
        let err = check("T", &body, TEMPLATE).expect_err("fails");
        assert_eq!(err, vec![Problem::BlockReversed("status".to_string())]);
    }

    #[test]
    fn a_crlf_body_keeps_its_headings() {
        let body = good_body().replace('\n', "\r\n");
        assert!(check("T", &body, TEMPLATE).is_ok());
    }
}
