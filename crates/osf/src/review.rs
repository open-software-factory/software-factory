//! `osf review post`: publish code-review findings on a pull request as one
//! native review, with inline comments at file and line, a short summary
//! body, and a verdict.
//!
//! GitHub refuses a formal review from the pull request's own author. When
//! that happens the same findings post instead as an ordinary comment, with
//! the verdict marked advisory, so a review is never silently lost. The
//! part that decides what to send, and how to react to a refusal, is pure
//! and holds no network call; only [`fetch_head_sha`] and [`post_via_gh`]
//! run an external command.

use std::fmt;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Deserialize;
use serde_json::{json, Value};

/// A byte budget on the final review or comment body. GitHub reads it, and
/// a body over this is refused rather than silently truncated.
pub const BODY_LIMIT: usize = 2000;

/// One finding, as produced by a review pass.
///
/// A finding with no `line` cannot be anchored inline and is appended to
/// the body instead, under "Findings outside the diff". A finding whose
/// `action` is `dismiss` is never posted at all.
#[derive(Debug, Clone, Deserialize)]
pub struct Finding {
    pub id: String,
    pub path: String,
    pub line: Option<u64>,
    pub severity: String,
    pub action: String,
    pub body: String,
}

/// Why a plan, or the advisory fallback built from it, could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewError {
    NotArray,
    Parse(String),
    BodyTooLong { len: usize, limit: usize },
    AdvisoryTooLong { len: usize, limit: usize },
}

impl fmt::Display for ReviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReviewError::NotArray => write!(f, "findings must be a JSON array"),
            ReviewError::Parse(e) => write!(f, "cannot read the findings: {e}"),
            ReviewError::BodyTooLong { len, limit } => write!(
                f,
                "the review body is {len} characters once the unanchored findings are appended, \
                 limit is {limit}. Shorten the summary, or give those findings a line."
            ),
            ReviewError::AdvisoryTooLong { len, limit } => write!(
                f,
                "the advisory body is {len} characters, limit is {limit}. Shorten the summary."
            ),
        }
    }
}

impl std::error::Error for ReviewError {}

/// The verdict a set of findings earns: `REQUEST_CHANGES` when a posted
/// finding's action is in the block list, otherwise `APPROVE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    RequestChanges,
    Approve,
}

impl Verdict {
    /// The value GitHub's review `event` field expects.
    #[must_use]
    pub const fn as_event(self) -> &'static str {
        match self {
            Verdict::RequestChanges => "REQUEST_CHANGES",
            Verdict::Approve => "APPROVE",
        }
    }
}

/// One inline comment: a finding anchored to a file and line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub path: String,
    pub line: u64,
    pub body: String,
}

/// What a set of findings decided to send: the verdict, the finished body,
/// and the inline comments. Nothing here has touched the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub verdict: Verdict,
    pub body: String,
    pub comments: Vec<Comment>,
}

/// Splits a comma-separated block list into trimmed, non-empty entries.
#[must_use]
pub fn parse_block_on(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn unanchored_line(f: &Finding) -> String {
    format!(
        "- **{}** ({}, {}) `{}`: {}",
        f.id,
        f.severity,
        f.action,
        f.path,
        f.body.replace('\n', " ")
    )
}

/// Decides the verdict, builds the final body, and lays out the inline
/// comments for `findings`. A finding whose action is `dismiss` is dropped
/// before any of this runs; it is never posted anywhere.
///
/// # Errors
/// Returns [`ReviewError::BodyTooLong`] when the summary, with any
/// unanchored findings appended, is over [`BODY_LIMIT`] characters.
pub fn plan_review(
    findings: &[Finding],
    summary: &str,
    block_on: &[String],
) -> Result<Plan, ReviewError> {
    let posted: Vec<&Finding> = findings.iter().filter(|f| f.action != "dismiss").collect();
    let blocking = posted
        .iter()
        .any(|f| block_on.iter().any(|b| b == &f.action));
    let verdict = if blocking {
        Verdict::RequestChanges
    } else {
        Verdict::Approve
    };

    let unanchored: Vec<&&Finding> = posted.iter().filter(|f| f.line.is_none()).collect();
    let mut body = summary.trim_end_matches('\n').to_string();
    if !unanchored.is_empty() {
        let lines: Vec<String> = unanchored.iter().map(|f| unanchored_line(f)).collect();
        body.push_str("\n\n**Findings outside the diff**\n");
        body.push_str(&lines.join("\n"));
    }
    if body.len() > BODY_LIMIT {
        return Err(ReviewError::BodyTooLong {
            len: body.len(),
            limit: BODY_LIMIT,
        });
    }

    let comments = posted
        .iter()
        .filter_map(|f| {
            f.line.map(|line| Comment {
                path: f.path.clone(),
                line,
                body: format!("**{}** · {} · {}\n\n{}", f.id, f.severity, f.action, f.body),
            })
        })
        .collect();

    Ok(Plan {
        verdict,
        body,
        comments,
    })
}

/// Reads a findings file and checks it holds a JSON array of findings.
///
/// # Errors
/// Returns an error if `path` cannot be read, is not JSON, is not a JSON
/// array, or does not match the finding shape.
pub fn load_findings(path: &Path) -> Result<Vec<Finding>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| ReviewError::Parse(e.to_string()).to_string())?;
    if !value.is_array() {
        return Err(ReviewError::NotArray.to_string());
    }
    serde_json::from_value(value).map_err(|e| ReviewError::Parse(e.to_string()).to_string())
}

fn comment_json(comment: &Comment) -> Value {
    json!({
        "path": comment.path,
        "line": comment.line,
        "side": "RIGHT",
        "body": comment.body,
    })
}

/// The request body a GitHub review posts: the head commit, the final
/// body, the verdict event, and every inline comment.
#[must_use]
pub fn payload(head_sha: &str, body: &str, event: &str, comments: &[Comment]) -> Value {
    json!({
        "commit_id": head_sha,
        "body": body,
        "event": event,
        "comments": comments.iter().map(comment_json).collect::<Vec<_>>(),
    })
}

/// One attempt at posting a review or comment: it landed, GitHub refused it
/// because the reviewer and the author share one identity, or it failed
/// for some other reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostAttempt {
    Success {
        id: String,
        state: String,
        url: String,
    },
    RefusedSelfReview,
    Error(String),
}

/// The three outcomes a caller of `osf review post` can see: a formal
/// review landed, it landed as a fallback comment after a refusal, or
/// nothing landed and this says why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Reviewed {
        verdict: Verdict,
        n_inline: usize,
        id: String,
        state: String,
        url: String,
    },
    FallbackComment {
        verdict: Verdict,
        n_inline: usize,
        id: String,
        state: String,
        url: String,
    },
    Rejected(ReviewError),
    PostFailed(String),
}

/// What to do after the first post attempt: either the outcome is already
/// final, or a second, advisory attempt is needed with this payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextStep {
    Done(Outcome),
    RetryAsComment(Value),
}

fn value_to_plain_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn parse_post_success(text: &str) -> Result<(String, String, String), String> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("cannot read gh's response: {e}"))?;
    let id = value
        .get("id")
        .map(value_to_plain_string)
        .ok_or_else(|| "gh's response has no id".to_string())?;
    let state = value
        .get("state")
        .and_then(Value::as_str)
        .ok_or_else(|| "gh's response has no state".to_string())?
        .to_string();
    let url = value
        .get("html_url")
        .and_then(Value::as_str)
        .ok_or_else(|| "gh's response has no html_url".to_string())?
        .to_string();
    Ok((id, state, url))
}

/// Turns the raw result of one `gh` call into a [`PostAttempt`]. Pure: it
/// never runs a command, so it can be tested with a literal string standing
/// in for whatever `gh` printed.
#[must_use]
pub fn classify_post_result(success: bool, output_text: &str) -> PostAttempt {
    if success {
        return match parse_post_success(output_text) {
            Ok((id, state, url)) => PostAttempt::Success { id, state, url },
            Err(e) => PostAttempt::Error(e),
        };
    }
    if output_text.contains("on your own pull request") {
        PostAttempt::RefusedSelfReview
    } else {
        PostAttempt::Error(output_text.trim().to_string())
    }
}

/// Builds the advisory comment payload that follows a self-review refusal:
/// the same findings, the same verdict named but marked advisory, posted
/// as `COMMENT` instead of `REQUEST_CHANGES` or `APPROVE`.
///
/// # Errors
/// Returns [`ReviewError::AdvisoryTooLong`] when the advisory prefix pushes
/// the body over [`BODY_LIMIT`].
pub fn build_advisory_payload(
    plan: &Plan,
    verdict: Verdict,
    head_sha: &str,
) -> Result<Value, ReviewError> {
    let advisory_body = format!(
        "**Verdict (advisory): {}** — posted as a comment, because the reviewer and the author \
         share one GitHub identity.\n\n{}",
        verdict.as_event(),
        plan.body
    );
    if advisory_body.len() > BODY_LIMIT {
        return Err(ReviewError::AdvisoryTooLong {
            len: advisory_body.len(),
            limit: BODY_LIMIT,
        });
    }
    Ok(payload(head_sha, &advisory_body, "COMMENT", &plan.comments))
}

/// Reacts to the first post attempt: done on success or on an
/// unrecoverable error, or a second attempt is needed as a comment. Pure:
/// takes the attempt as data, never runs `gh` itself.
#[must_use]
pub fn after_first_attempt(attempt: PostAttempt, plan: &Plan, head_sha: &str) -> NextStep {
    match attempt {
        PostAttempt::Success { id, state, url } => NextStep::Done(Outcome::Reviewed {
            verdict: plan.verdict,
            n_inline: plan.comments.len(),
            id,
            state,
            url,
        }),
        PostAttempt::RefusedSelfReview => {
            match build_advisory_payload(plan, plan.verdict, head_sha) {
                Ok(advisory) => NextStep::RetryAsComment(advisory),
                Err(e) => NextStep::Done(Outcome::Rejected(e)),
            }
        }
        PostAttempt::Error(e) => NextStep::Done(Outcome::PostFailed(e)),
    }
}

/// Reacts to the second, advisory post attempt. Pure, for the same reason
/// as [`after_first_attempt`].
#[must_use]
pub fn after_fallback_attempt(attempt: PostAttempt, plan: &Plan) -> Outcome {
    match attempt {
        PostAttempt::Success { id, state, url } => Outcome::FallbackComment {
            verdict: plan.verdict,
            n_inline: plan.comments.len(),
            id,
            state,
            url,
        },
        PostAttempt::RefusedSelfReview => {
            Outcome::PostFailed("gh refused the fallback comment as well".to_string())
        }
        PostAttempt::Error(e) => Outcome::PostFailed(e),
    }
}

fn parse_head_sha_json(text: &str) -> Result<String, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("cannot read the pull request: {e}"))?;
    value
        .get("headRefOid")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "the pull request has no head commit".to_string())
}

/// The head commit of a pull request's branch, the one a review anchors
/// its comments to.
///
/// # Errors
/// Returns an error if `gh` cannot run, fails, or its answer holds no
/// head commit.
pub fn fetch_head_sha(repo: &str, pr: u64) -> Result<String, String> {
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            &pr.to_string(),
            "--repo",
            repo,
            "--json",
            "headRefOid",
        ])
        .output()
        .map_err(|e| format!("cannot run gh: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "could not read the head commit of {repo}#{pr}: {}",
            stderr.trim()
        ));
    }
    parse_head_sha_json(&String::from_utf8_lossy(&output.stdout))
        .map_err(|e| format!("{e} ({repo}#{pr})"))
}

/// Posts one review or comment payload to a pull request. Thin: it only
/// shells out and hands the raw result to [`classify_post_result`].
#[must_use]
pub fn post_via_gh(repo: &str, pr: u64, payload: &Value) -> PostAttempt {
    let mut child = match Command::new("gh")
        .args([
            "api",
            "-X",
            "POST",
            &format!("repos/{repo}/pulls/{pr}/reviews"),
            "--input",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return PostAttempt::Error(format!("cannot run gh: {e}")),
    };
    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(payload.to_string().as_bytes()) {
            return PostAttempt::Error(format!("cannot write to gh: {e}"));
        }
    }
    let output = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => return PostAttempt::Error(format!("cannot run gh: {e}")),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        classify_post_result(true, &stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        classify_post_result(false, &format!("{stdout}{stderr}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(id: &str, path: &str, line: Option<u64>, severity: &str, action: &str) -> Finding {
        Finding {
            id: id.to_string(),
            path: path.to_string(),
            line,
            severity: severity.to_string(),
            action: action.to_string(),
            body: format!("body of {id}"),
        }
    }

    fn default_block_on() -> Vec<String> {
        parse_block_on("must-fix,should-fix")
    }

    #[test]
    fn parses_a_comma_separated_block_list() {
        assert_eq!(
            parse_block_on("must-fix, should-fix ,"),
            vec!["must-fix".to_string(), "should-fix".to_string()]
        );
    }

    #[test]
    fn parse_head_sha_json_reads_the_field() {
        assert_eq!(
            parse_head_sha_json(r#"{"headRefOid":"abc123"}"#),
            Ok("abc123".to_string())
        );
    }

    #[test]
    fn parse_head_sha_json_rejects_an_empty_commit() {
        assert!(parse_head_sha_json(r#"{"headRefOid":""}"#).is_err());
    }

    #[test]
    fn classify_post_result_reads_a_successful_response() {
        let text = r#"{"id":42,"state":"APPROVED","html_url":"https://example.invalid/1"}"#;
        assert_eq!(
            classify_post_result(true, text),
            PostAttempt::Success {
                id: "42".to_string(),
                state: "APPROVED".to_string(),
                url: "https://example.invalid/1".to_string(),
            }
        );
    }

    #[test]
    fn classify_post_result_recognizes_the_self_review_refusal() {
        let text = "gh: Review cannot be requested on your own pull request (HTTP 422)";
        assert_eq!(
            classify_post_result(false, text),
            PostAttempt::RefusedSelfReview
        );
    }

    #[test]
    fn classify_post_result_treats_other_failures_as_errors() {
        let text = "gh: some other failure (HTTP 500)";
        assert_eq!(
            classify_post_result(false, text),
            PostAttempt::Error(text.to_string())
        );
    }

    // A should-fix finding with the default block list gives
    // REQUEST_CHANGES, one inline comment, and the null-line finding in
    // the body.
    #[test]
    fn a_should_fix_finding_blocks_by_default() {
        let findings = vec![
            finding("cor-01", "README.md", Some(1), "major", "should-fix"),
            finding("hyg-01", "AGENTS.md", None, "minor", "maybe-fix"),
        ];
        let plan = plan_review(&findings, "Summary.\n", &default_block_on()).expect("plans");
        assert_eq!(plan.verdict, Verdict::RequestChanges);
        assert_eq!(plan.comments.len(), 1);
        let comment = plan.comments.first().expect("one inline comment");
        assert_eq!(comment.path, "README.md");
        assert_eq!(comment.line, 1);
        assert!(
            plan.body.contains("Findings outside the diff"),
            "{}",
            plan.body
        );
    }

    // Blocking only on must-fix gives APPROVE for the same findings.
    #[test]
    fn blocking_only_on_must_fix_approves_a_should_fix_finding() {
        let findings = vec![
            finding("cor-01", "README.md", Some(1), "major", "should-fix"),
            finding("hyg-01", "AGENTS.md", None, "minor", "maybe-fix"),
        ];
        let plan =
            plan_review(&findings, "Summary.\n", &parse_block_on("must-fix")).expect("plans");
        assert_eq!(plan.verdict, Verdict::Approve);
    }

    // A dismissed finding is posted neither inline nor in the body.
    #[test]
    fn a_dismissed_finding_is_dropped_entirely() {
        let findings = vec![
            finding("cor-01", "README.md", Some(1), "major", "should-fix"),
            finding("sec-09", "README.md", Some(2), "major", "dismiss"),
            finding("hyg-09", "AGENTS.md", None, "minor", "dismiss"),
        ];
        let plan = plan_review(&findings, "Summary.\n", &default_block_on()).expect("plans");
        assert_eq!(plan.comments.len(), 1);
        assert!(!plan.body.contains("hyg-09"), "{}", plan.body);
    }

    // The limit applies to the final body, unanchored findings included.
    #[test]
    fn an_oversized_final_body_is_refused_with_the_reason_named() {
        let long_body = "x".repeat(1900);
        let findings = vec![Finding {
            id: "big-01".to_string(),
            path: "README.md".to_string(),
            line: None,
            severity: "minor".to_string(),
            action: "maybe-fix".to_string(),
            body: long_body,
        }];
        let summary = "A short review summary, well under the limit on its own. ".repeat(4);
        let err =
            plan_review(&findings, &summary, &default_block_on()).expect_err("body is too long");
        assert!(err.to_string().contains("review body is"), "{err}");
    }

    // A summary over 2,000 characters is refused with the limit named.
    #[test]
    fn a_long_summary_alone_is_refused_with_the_limit_named() {
        let findings = vec![finding(
            "cor-01",
            "README.md",
            Some(1),
            "major",
            "should-fix",
        )];
        let long_summary = "x".repeat(2100);
        let err = plan_review(&findings, &long_summary, &default_block_on())
            .expect_err("summary is too long");
        assert!(err.to_string().contains("limit is 2000"), "{err}");
    }

    // Nothing in the specification exercises the self-review fallback
    // against a network, since a dry run never posts. These tests drive
    // the pure decision the fallback rests on directly.
    #[test]
    fn a_self_review_refusal_falls_back_to_an_advisory_comment() {
        let findings = vec![finding(
            "cor-01",
            "README.md",
            Some(1),
            "major",
            "should-fix",
        )];
        let plan = plan_review(&findings, "Summary.\n", &default_block_on()).expect("plans");
        let next = after_first_attempt(PostAttempt::RefusedSelfReview, &plan, "deadbeef");
        let NextStep::RetryAsComment(advisory) = next else {
            panic!("expected a fallback comment, got {next:?}");
        };
        assert_eq!(
            advisory.get("event"),
            Some(&Value::String("COMMENT".to_string()))
        );
        let body = advisory
            .get("body")
            .and_then(Value::as_str)
            .expect("body is a string");
        assert!(
            body.starts_with("**Verdict (advisory): REQUEST_CHANGES**"),
            "{body}"
        );
        assert!(body.contains("Summary."), "{body}");
    }

    #[test]
    fn a_successful_fallback_comment_is_reported_as_such() {
        let findings = vec![finding(
            "cor-01",
            "README.md",
            Some(1),
            "major",
            "should-fix",
        )];
        let plan = plan_review(&findings, "Summary.\n", &default_block_on()).expect("plans");
        let attempt = PostAttempt::Success {
            id: "7".to_string(),
            state: "COMMENTED".to_string(),
            url: "https://example.invalid/7".to_string(),
        };
        let outcome = after_fallback_attempt(attempt, &plan);
        assert_eq!(
            outcome,
            Outcome::FallbackComment {
                verdict: Verdict::RequestChanges,
                n_inline: 1,
                id: "7".to_string(),
                state: "COMMENTED".to_string(),
                url: "https://example.invalid/7".to_string(),
            }
        );
    }

    #[test]
    fn a_successful_first_attempt_is_reported_as_a_review_not_a_fallback() {
        let findings = vec![finding(
            "cor-01",
            "README.md",
            Some(1),
            "major",
            "should-fix",
        )];
        let plan = plan_review(&findings, "Summary.\n", &default_block_on()).expect("plans");
        let attempt = PostAttempt::Success {
            id: "9".to_string(),
            state: "CHANGES_REQUESTED".to_string(),
            url: "https://example.invalid/9".to_string(),
        };
        let next = after_first_attempt(attempt, &plan, "deadbeef");
        assert_eq!(
            next,
            NextStep::Done(Outcome::Reviewed {
                verdict: Verdict::RequestChanges,
                n_inline: 1,
                id: "9".to_string(),
                state: "CHANGES_REQUESTED".to_string(),
                url: "https://example.invalid/9".to_string(),
            })
        );
    }

    #[test]
    fn an_advisory_body_over_the_limit_is_rejected_before_a_retry() {
        let findings = vec![Finding {
            id: "cor-01".to_string(),
            path: "README.md".to_string(),
            line: Some(1),
            severity: "major".to_string(),
            action: "should-fix".to_string(),
            body: "x".to_string(),
        }];
        let long_summary = "x".repeat(1990);
        let plan = plan_review(&findings, &long_summary, &default_block_on()).expect("plans");
        let next = after_first_attempt(PostAttempt::RefusedSelfReview, &plan, "deadbeef");
        assert!(
            matches!(next, NextStep::Done(Outcome::Rejected(_))),
            "{next:?}"
        );
    }

    #[test]
    fn a_non_refusal_failure_is_reported_as_a_post_failure_not_a_fallback() {
        let findings = vec![finding(
            "cor-01",
            "README.md",
            Some(1),
            "major",
            "should-fix",
        )];
        let plan = plan_review(&findings, "Summary.\n", &default_block_on()).expect("plans");
        let next = after_first_attempt(
            PostAttempt::Error("gh: rate limited".to_string()),
            &plan,
            "deadbeef",
        );
        assert_eq!(
            next,
            NextStep::Done(Outcome::PostFailed("gh: rate limited".to_string()))
        );
    }

    #[test]
    fn findings_that_are_not_a_json_array_are_refused() {
        let dir = std::env::temp_dir().join(format!(
            "osf-review-test-{}-{}",
            std::process::id(),
            "not-an-array"
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("findings.json");
        std::fs::write(&path, r#"{"id":"not-an-array"}"#).expect("write fixture");
        let err = load_findings(&path).expect_err("not an array");
        assert!(err.contains("must be a JSON array"), "{err}");
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}
