//! The engine's Report step as provider-neutral run code: open the pull
//! request, write the status block, post the review, then read the check
//! status and the capabilities, in that one order.

use crate::forge::{
    Capabilities, CheckStatus, Forge, ForgeError, NewPullRequest, NewReview, PostedReview,
    PullRequestId, ReadOutcome, ReviewComment, Verdict,
};
use std::fmt;

/// The review the engine wants posted, before the pull request exists.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReviewDraft {
    pub head_sha: String,
    pub verdict: Verdict,
    pub body: String,
    pub comments: Vec<ReviewComment>,
}

/// One input to the Report step.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReportRequest {
    pub pull_request: NewPullRequest,
    pub status_block: String,
    pub review: ReviewDraft,
}

/// What the Report step observed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Report {
    pub pull_request: PullRequestId,
    pub review: PostedReview,
    pub checks: ReadOutcome<CheckStatus>,
    pub capabilities: Capabilities,
}

/// The Report step that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Step {
    OpenPullRequest,
    WriteStatusBlock,
    PostReview,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Step::OpenPullRequest => "open pull request",
            Step::WriteStatusBlock => "write status block",
            Step::PostReview => "post review",
        })
    }
}

/// A failed Report step and what the forge reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportError {
    pub step: Step,
    pub error: ForgeError,
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let step = self.step;
        let error = &self.error;
        write!(f, "{step}: {error}")
    }
}

impl std::error::Error for ReportError {}

/// Runs the Report step against `forge`, in one fixed order.
///
/// # Errors
/// Returns a [`ReportError`] naming the step when the forge refuses it, and
/// stops there: nothing after the failed step runs.
pub fn report_change(forge: &dyn Forge, request: &ReportRequest) -> Result<Report, ReportError> {
    let pull_request = forge
        .open_pull_request(&request.pull_request)
        .map_err(|error| ReportError {
            step: Step::OpenPullRequest,
            error,
        })?;
    forge
        .write_status_block(&pull_request, &request.status_block)
        .map_err(|error| ReportError {
            step: Step::WriteStatusBlock,
            error,
        })?;
    let review = NewReview {
        pull_request: pull_request.clone(),
        head_sha: request.review.head_sha.clone(),
        verdict: request.review.verdict,
        body: request.review.body.clone(),
        comments: request.review.comments.clone(),
    };
    let review = forge.post_review(&review).map_err(|error| ReportError {
        step: Step::PostReview,
        error,
    })?;
    let checks = forge.read_check_status(&pull_request);
    let capabilities = forge.capabilities(&pull_request);
    Ok(Report {
        pull_request,
        review,
        checks,
        capabilities,
    })
}

#[cfg(test)]
mod tests {
    use crate::forge::fake::{Call, FakeForge, Operation};
    use crate::forge::report::{
        report_change, Report, ReportError, ReportRequest, ReviewDraft, Step,
    };
    use crate::forge::{
        Capabilities, Check, CheckStatus, Forge, ForgeError, NewPullRequest, NewReview,
        PostedReview, PullRequestId, ReadCapability, ReadOutcome, ReviewComment, Verdict,
        VerdictCapability,
    };

    fn request() -> ReportRequest {
        ReportRequest {
            pull_request: NewPullRequest {
                repo: "open-software-factory/demo".to_string(),
                head: "work/item-1".to_string(),
                base: "main".to_string(),
                title: "A change".to_string(),
                body: "Body.".to_string(),
            },
            status_block: "the block".to_string(),
            review: ReviewDraft {
                head_sha: "cafe1234".to_string(),
                verdict: Verdict::RequestChanges,
                body: "Summary.".to_string(),
                comments: vec![ReviewComment {
                    path: "src/lib.rs".to_string(),
                    line: 7,
                    body: "Fix this.".to_string(),
                }],
            },
        }
    }

    fn opened() -> PullRequestId {
        PullRequestId {
            repo: "open-software-factory/demo".to_string(),
            pr: "1".to_string(),
        }
    }

    fn review_to_post() -> NewReview {
        let draft = request();
        NewReview {
            pull_request: opened(),
            head_sha: draft.review.head_sha,
            verdict: draft.review.verdict,
            body: draft.review.body,
            comments: draft.review.comments,
        }
    }

    fn happy_report() -> Report {
        Report {
            pull_request: opened(),
            review: PostedReview {
                verdict: Verdict::RequestChanges,
                advisory: false,
                inline: 1,
                id: "1".to_string(),
                state: "COMMENTED".to_string(),
                url: "https://example.invalid/review/1".to_string(),
            },
            checks: ReadOutcome::Empty,
            capabilities: Capabilities {
                verdicts: VerdictCapability::Known(vec![Verdict::Approve, Verdict::RequestChanges]),
                check_status: ReadCapability::Readable,
            },
        }
    }

    #[test]
    fn the_happy_path_calls_every_step_in_order() {
        let forge = FakeForge::new();
        let report = report_change(&forge, &request()).expect("reports");
        assert_eq!(report, happy_report());
        assert_eq!(
            forge.calls(),
            vec![
                Call::OpenPullRequest(request().pull_request),
                Call::WriteStatusBlock {
                    pr: opened(),
                    block: "the block".to_string(),
                },
                Call::PostReview(review_to_post()),
                Call::ReadCheckStatus(opened()),
                Call::Capabilities(opened()),
            ]
        );
    }

    #[test]
    fn a_failed_open_pull_request_names_the_step_and_stops() {
        let forge = FakeForge::new().fail(
            Operation::OpenPullRequest,
            ForgeError::Rejected("refused".to_string()),
        );
        let error = report_change(&forge, &request()).expect_err("fails");
        assert_eq!(
            error,
            ReportError {
                step: Step::OpenPullRequest,
                error: ForgeError::Rejected("refused".to_string()),
            }
        );
        assert_eq!(
            forge.calls(),
            vec![Call::OpenPullRequest(request().pull_request)]
        );
    }

    #[test]
    fn a_failed_write_status_block_names_the_step_and_stops() {
        let forge = FakeForge::new().fail(
            Operation::WriteStatusBlock,
            ForgeError::Rejected("refused".to_string()),
        );
        let error = report_change(&forge, &request()).expect_err("fails");
        assert_eq!(
            error,
            ReportError {
                step: Step::WriteStatusBlock,
                error: ForgeError::Rejected("refused".to_string()),
            }
        );
        assert_eq!(
            forge.calls(),
            vec![
                Call::OpenPullRequest(request().pull_request),
                Call::WriteStatusBlock {
                    pr: opened(),
                    block: "the block".to_string(),
                },
            ]
        );
    }

    #[test]
    fn a_failed_post_review_names_the_step_and_stops() {
        let forge = FakeForge::new().fail(
            Operation::PostReview,
            ForgeError::Rejected("refused".to_string()),
        );
        let error = report_change(&forge, &request()).expect_err("fails");
        assert_eq!(
            error,
            ReportError {
                step: Step::PostReview,
                error: ForgeError::Rejected("refused".to_string()),
            }
        );
        assert_eq!(
            forge.calls(),
            vec![
                Call::OpenPullRequest(request().pull_request),
                Call::WriteStatusBlock {
                    pr: opened(),
                    block: "the block".to_string(),
                },
                Call::PostReview(review_to_post()),
            ]
        );
    }

    #[test]
    fn an_unknown_check_read_is_carried_through_as_unknown() {
        let mut forge = FakeForge::new();
        forge.check_status = ReadOutcome::Unknown("rate limited".to_string());
        let report = report_change(&forge, &request()).expect("reports");
        assert_eq!(
            report.checks,
            ReadOutcome::Unknown("rate limited".to_string())
        );
    }

    #[test]
    fn an_empty_check_read_is_carried_through_as_empty() {
        let forge = FakeForge::new();
        let report = report_change(&forge, &request()).expect("reports");
        assert_eq!(report.checks, ReadOutcome::Empty);
    }

    #[test]
    fn a_scripted_check_status_serves_one_answer_per_report() {
        let pending = ReadOutcome::Found(CheckStatus {
            checks: vec![Check {
                name: "ci".to_string(),
                state: "PENDING".to_string(),
                bucket: "pending".to_string(),
            }],
        });
        let pass = ReadOutcome::Found(CheckStatus {
            checks: vec![Check {
                name: "ci".to_string(),
                state: "SUCCESS".to_string(),
                bucket: "pass".to_string(),
            }],
        });
        let forge = FakeForge::new().script_check_status(vec![pending.clone(), pass.clone()]);
        let report = report_change(&forge, &request()).expect("reports");
        assert_eq!(report.checks, pending);
        assert_eq!(forge.read_check_status(&opened()), pass);
    }

    #[test]
    fn the_report_serializes_to_json() {
        let forge = FakeForge::new();
        let report = report_change(&forge, &request()).expect("reports");
        let value = serde_json::to_value(&report).expect("serializes");
        assert_eq!(
            value
                .get("pull_request")
                .and_then(|entry| entry.get("pr"))
                .and_then(serde_json::Value::as_str),
            Some("1")
        );
        assert_eq!(
            value
                .get("review")
                .and_then(|entry| entry.get("state"))
                .and_then(serde_json::Value::as_str),
            Some("COMMENTED")
        );
        assert_eq!(value.get("checks"), Some(&serde_json::json!("empty")));
        assert_eq!(
            value
                .get("capabilities")
                .and_then(|entry| entry.get("check_status")),
            Some(&serde_json::json!("readable"))
        );
    }

    #[test]
    fn a_report_error_displays_the_step_then_the_error() {
        let error = ReportError {
            step: Step::PostReview,
            error: ForgeError::Failed("could not post".to_string()),
        };
        assert_eq!(error.to_string(), "post review: could not post");
    }

    #[test]
    fn step_displays_and_serializes_as_kebab_case() {
        assert_eq!(Step::OpenPullRequest.to_string(), "open pull request");
        assert_eq!(Step::WriteStatusBlock.to_string(), "write status block");
        assert_eq!(Step::PostReview.to_string(), "post review");
        assert_eq!(
            serde_json::to_string(&Step::WriteStatusBlock).expect("serializes"),
            r#""write-status-block""#
        );
    }

    #[test]
    fn the_module_text_names_no_provider() {
        let text = include_str!("report.rs").to_lowercase();
        let banned = [["git", "hub"].concat(), ["g", "h"].concat()];
        let words: Vec<&str> = text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .collect();
        for word in banned {
            assert!(
                !words.contains(&word.as_str()),
                "the provider-neutral step must not name a provider"
            );
        }
    }
}
