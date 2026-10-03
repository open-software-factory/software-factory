//! Finds the work item a pull request is for. The work item is the issue the
//! pull request names on its `Issue:` line, or else the first issue it closes
//! with a closing keyword. Only an issue in the pull request's own repository
//! is read, through the code host's API, and the text that comes back is
//! saved for the reviewer jobs. The issue's text is written by people, so it
//! is untrusted input, as the pull request's own text is: it reaches a
//! reviewer only as data, after osf's secret redaction.
//!
//! A pull request that links no issue, or links one that cannot be read,
//! saves a reason instead of a body. The spec and acceptance lens then reports
//! could-not-run with that reason.

use regex::Regex;
use std::process::Command;
use std::sync::OnceLock;

/// The longest issue body saved, in characters. A longer body is cut and says so.
const MAX_BODY_CHARS: usize = 60_000;

/// The reason saved when a pull request in `repository` names no issue.
#[must_use]
pub fn no_issue_linked(repository: &str) -> String {
    format!("no issue is linked: add an Issue line that names the issue this pull request is for, or close one with a closing keyword such as Closes {repository}#N. Link one so the spec and acceptance lens can run")
}

/// What the code host holds for an issue number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    /// An issue, with its title and body.
    Issue { title: String, body: String },
    /// The number names a pull request, which is no work item.
    NotAnIssue,
    /// No such issue, or it cannot be read.
    NotFound,
}

/// Where issues are read from. The real source is [`GhIssues`].
pub trait IssueSource {
    /// Reads issue `number` of `repository`.
    ///
    /// # Errors
    /// Names the reason when the source itself fails, such as a network error.
    /// A missing issue is [`Fetched::NotFound`], not an error.
    fn fetch(&self, repository: &str, number: u64) -> Result<Fetched, String>;
}

/// The work item for a pull request, or why there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkItem {
    Found {
        /// `owner/repo#N`, with the repository the pull request lives in.
        reference: String,
        title: String,
        body: String,
    },
    Missing(String),
}

/// One reference found in text: the repository it names, if any, and the number.
type Reference = (Option<String>, u64);

fn reference_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?i)https://github\.com/([a-z0-9_.-]+/[a-z0-9_.-]+)/issues/(\d+)|([a-z0-9_.-]+/[a-z0-9_.-]+)#(\d+)|(?:^|[^\w/#&])#(\d+)",
        )
        .expect("pattern compiles")
    })
}

fn closing_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?i)\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\b:?[ \t]+(\S+)")
            .expect("pattern compiles")
    })
}

/// Every issue reference in `text`, in order.
fn references(text: &str) -> Vec<Reference> {
    let mut found = Vec::new();
    for caps in reference_pattern().captures_iter(text) {
        let parsed = if let (Some(repo), Some(n)) = (caps.get(1), caps.get(2)) {
            n.as_str()
                .parse()
                .ok()
                .map(|n| (Some(repo.as_str().to_string()), n))
        } else if let (Some(repo), Some(n)) = (caps.get(3), caps.get(4)) {
            n.as_str()
                .parse()
                .ok()
                .map(|n| (Some(repo.as_str().to_string()), n))
        } else {
            caps.get(5)
                .and_then(|n| n.as_str().parse().ok())
                .map(|n| (None, n))
        };
        if let Some(reference) = parsed {
            if !found.contains(&reference) {
                found.push(reference);
            }
        }
    }
    found
}

/// The issues `body` names, in order: first on its `Issue:` lines, then after a closing keyword.
fn candidates(body: &str) -> Vec<Reference> {
    let mut found: Vec<Reference> = Vec::new();
    let mut add = |items: Vec<Reference>| {
        for item in items {
            if !found.contains(&item) {
                found.push(item);
            }
        }
    };
    for line in body.lines() {
        let text = line.trim_start_matches(['-', '*', '>', ' ', '\t']);
        let lower = text.to_ascii_lowercase();
        if lower.starts_with("issue:") || lower.starts_with("issues:") {
            add(references(text));
        }
    }
    for caps in closing_pattern().captures_iter(body) {
        if let Some(token) = caps.get(1) {
            add(references(token.as_str()));
        }
    }
    found
}

/// The work item for the pull request whose body is `pr_body`, in `repository`.
///
/// # Errors
/// Returns the source's own error when it fails. A pull request with no
/// readable linked issue is [`WorkItem::Missing`], not an error.
pub fn find(source: &dyn IssueSource, repository: &str, pr_body: &str) -> Result<WorkItem, String> {
    let all = candidates(pr_body);
    let own = all.iter().find_map(|(repo, number)| match repo {
        Some(named) if !named.eq_ignore_ascii_case(repository) => None,
        _ => Some(*number),
    });
    let Some(number) = own else {
        return Ok(WorkItem::Missing(match all.first() {
            Some((Some(other), n)) => format!(
                "the only issue linked is {other}#{n}, which is in another repository, and only an issue in {repository} is read. Link an issue in {repository}"
            ),
            _ => no_issue_linked(repository),
        }));
    };
    let reference = format!("{repository}#{number}");
    Ok(match source.fetch(repository, number)? {
        Fetched::Issue { title, body } => WorkItem::Found {
            reference,
            title,
            body: cut(&body),
        },
        Fetched::NotAnIssue => WorkItem::Missing(format!(
            "{reference} is a pull request, not an issue. Link the issue this pull request is for"
        )),
        Fetched::NotFound => WorkItem::Missing(format!(
            "{reference} was not found or cannot be read. Link an issue in {repository} that exists"
        )),
    })
}

/// `body`, cut to [`MAX_BODY_CHARS`] with a note when it was longer.
fn cut(body: &str) -> String {
    if body.chars().count() <= MAX_BODY_CHARS {
        return body.to_string();
    }
    let kept: String = body.chars().take(MAX_BODY_CHARS).collect();
    format!("{kept}\n\n[cut: the issue body is longer than {MAX_BODY_CHARS} characters]")
}

impl WorkItem {
    /// The JSON text saved for the reviewer jobs.
    #[must_use]
    pub fn to_json(&self) -> String {
        let value = match self {
            WorkItem::Found {
                reference,
                title,
                body,
            } => serde_json::json!({"issue": reference, "title": title, "body": body}),
            WorkItem::Missing(reason) => serde_json::json!({"missing": reason}),
        };
        value.to_string()
    }
}

/// The work item text a reviewer is given, from the JSON that [`WorkItem::to_json`] saved.
///
/// # Errors
/// Returns the saved reason when the pull request had no readable work
/// item, or says the text is not in the expected form.
pub fn read_saved(text: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| format!("the saved work item is not valid JSON: {e}"))?;
    if let Some(reason) = value.get("missing").and_then(serde_json::Value::as_str) {
        return Err(reason.to_string());
    }
    let field = |name: &str| value.get(name).and_then(serde_json::Value::as_str);
    match (field("issue"), field("title"), field("body")) {
        (Some(issue), Some(title), Some(body)) => Ok(format!("Issue: {issue} ({title})\n\n{body}")),
        _ => Err("the saved work item is not in the expected form".to_string()),
    }
}

/// Reads issues through `gh api`, with the token in `GH_TOKEN`.
#[derive(Debug, Clone, Copy)]
pub struct GhIssues;

impl IssueSource for GhIssues {
    fn fetch(&self, repository: &str, number: u64) -> Result<Fetched, String> {
        let output = Command::new("gh")
            .args(["api", &format!("repos/{repository}/issues/{number}")])
            .output()
            .map_err(|e| format!("cannot run gh: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("HTTP 404") || stderr.contains("HTTP 410") {
                return Ok(Fetched::NotFound);
            }
            return Err(format!(
                "could not read {repository}#{number}: {}",
                stderr.lines().next().unwrap_or("gh failed").trim()
            ));
        }
        let value: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("{repository}#{number}: the answer is not JSON: {e}"))?;
        if value.get("pull_request").is_some_and(|p| !p.is_null()) {
            return Ok(Fetched::NotAnIssue);
        }
        let title = value
            .get("title")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("{repository}#{number}: the answer has no title"))?;
        let body = value
            .get("body")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        Ok(Fetched::Issue {
            title: title.to_string(),
            body: body.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A source that holds a few issues and records what it was asked for.
    struct Fake {
        asked: RefCell<Vec<(String, u64)>>,
        answer: Result<Fetched, String>,
    }

    impl Fake {
        fn answering(answer: Result<Fetched, String>) -> Self {
            Self {
                asked: RefCell::new(Vec::new()),
                answer,
            }
        }

        fn issue(body: &str) -> Self {
            Self::answering(Ok(Fetched::Issue {
                title: "Build the check".to_string(),
                body: body.to_string(),
            }))
        }
    }

    impl IssueSource for Fake {
        fn fetch(&self, repository: &str, number: u64) -> Result<Fetched, String> {
            self.asked
                .borrow_mut()
                .push((repository.to_string(), number));
            self.answer.clone()
        }
    }

    const REPO: &str = "open-software-factory/software-factory";

    #[test]
    fn the_issue_line_names_the_work_item() {
        let body = "Issue: [open-software-factory/software-factory#137 (Build the check)](https://github.com/open-software-factory/software-factory/issues/137)\n\n## What and why\nText.";
        assert_eq!(candidates(body), vec![(Some(REPO.to_string()), 137)]);
    }

    #[test]
    fn an_issue_line_with_a_bare_number_or_a_link_alone_is_read() {
        assert_eq!(candidates("Issue: #12"), vec![(None, 12)]);
        assert_eq!(
            candidates(
                "- Issue: https://github.com/open-software-factory/software-factory/issues/9"
            ),
            vec![(
                Some("open-software-factory/software-factory".to_string()),
                9
            )]
        );
    }

    #[test]
    fn a_closing_keyword_names_the_work_item_when_no_issue_line_does() {
        assert_eq!(candidates("Closes #44 and more"), vec![(None, 44)]);
        assert_eq!(
            candidates("This fixes: open-software-factory/software-factory#5."),
            vec![(
                Some("open-software-factory/software-factory".to_string()),
                5
            )]
        );
        assert_eq!(candidates("Resolved #7"), vec![(None, 7)]);
    }

    #[test]
    fn the_issue_line_comes_before_a_closing_keyword() {
        let found = candidates("Closes #44\n\nIssue: #12");
        assert_eq!(found, vec![(None, 12), (None, 44)]);
    }

    #[test]
    fn a_number_in_prose_with_no_keyword_names_nothing() {
        assert!(candidates(
            "See #5 and also open-software-factory/software-factory#6 for background."
        )
        .is_empty());
        assert!(candidates("An entity &#123; is not a reference.").is_empty());
    }

    #[test]
    fn the_issue_the_pull_request_names_is_fetched_from_its_own_repository() {
        let source = Fake::issue("## Done when\n- it works");
        let item = find(&source, REPO, "Issue: #137\n").expect("finds");
        assert_eq!(
            item,
            WorkItem::Found {
                reference: format!("{REPO}#137"),
                title: "Build the check".to_string(),
                body: "## Done when\n- it works".to_string(),
            }
        );
        assert_eq!(*source.asked.borrow(), vec![(REPO.to_string(), 137)]);
    }

    #[test]
    fn with_no_linked_issue_the_reason_says_to_link_one() {
        let source = Fake::issue("unused");
        let item = find(&source, REPO, "A body that names no issue.").expect("finds");
        let WorkItem::Missing(reason) = item else {
            panic!("expected Missing");
        };
        assert!(reason.contains("Link one"), "{reason}");
        assert!(source.asked.borrow().is_empty(), "nothing was fetched");
    }

    #[test]
    fn an_issue_in_another_repository_is_not_read() {
        let source = Fake::issue("unused");
        let item = find(&source, REPO, "Issue: open-software-factory/other-repo#3").expect("finds");
        let WorkItem::Missing(reason) = item else {
            panic!("expected Missing");
        };
        assert!(
            reason.contains("open-software-factory/other-repo#3")
                && reason.contains("Link an issue"),
            "{reason}"
        );
        assert!(source.asked.borrow().is_empty());
    }

    #[test]
    fn a_pull_request_or_a_missing_issue_is_a_reason_not_a_body() {
        for (answer, expected) in [
            (Fetched::NotAnIssue, "is a pull request"),
            (Fetched::NotFound, "was not found"),
        ] {
            let source = Fake::answering(Ok(answer));
            let item = find(&source, REPO, "Issue: #3").expect("finds");
            let WorkItem::Missing(reason) = item else {
                panic!("expected Missing");
            };
            assert!(reason.contains(expected), "{reason}");
        }
    }

    #[test]
    fn a_failing_source_stops_instead_of_guessing() {
        let source = Fake::answering(Err("network down".to_string()));
        let e = find(&source, REPO, "Issue: #3").expect_err("stops");
        assert!(e.contains("network down"), "{e}");
    }

    #[test]
    fn a_very_long_body_is_cut_with_a_note() {
        let source = Fake::issue(&"x".repeat(MAX_BODY_CHARS + 10));
        let WorkItem::Found { body, .. } = find(&source, REPO, "Issue: #3").expect("finds") else {
            panic!("expected Found");
        };
        assert!(body.contains("[cut: the issue body is longer"), "{body}");
        assert!(body.chars().count() < MAX_BODY_CHARS + 100);
    }

    #[test]
    fn the_saved_form_reads_back_as_the_work_item_text_or_the_reason() {
        let found = WorkItem::Found {
            reference: "open-software-factory/software-factory#3".to_string(),
            title: "A title".to_string(),
            body: "## Done when\n- yes".to_string(),
        };
        let text = read_saved(&found.to_json()).expect("reads");
        assert!(
            text.starts_with("Issue: open-software-factory/software-factory#3 (A title)"),
            "{text}"
        );
        assert!(text.contains("## Done when"), "{text}");
        let missing = WorkItem::Missing("link one".to_string());
        assert_eq!(read_saved(&missing.to_json()), Err("link one".to_string()));
        assert!(read_saved("{}").is_err());
        assert!(read_saved("not json").is_err());
    }
}
