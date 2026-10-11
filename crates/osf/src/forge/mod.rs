//! Provider-neutral forge interface: the seam between the factory and a code
//! host. Nothing in this module names a provider, a URL or a command.

pub mod fake;
pub mod report;

pub use report::{report_change, Report, ReportError, ReportRequest, ReviewDraft, Step};

use serde::Serialize;
use std::fmt;

/// Identifies a pull request: `repo` is `owner/name`, and `pr` is the
/// selector string the status commands already accept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PullRequestId {
    pub repo: String,
    pub pr: String,
}

/// A branch to create, from an existing commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewBranch {
    pub repo: String,
    pub name: String,
    pub from_sha: String,
}

/// A branch the forge recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Branch {
    pub name: String,
    pub sha: String,
}

/// A pull request to open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewPullRequest {
    pub repo: String,
    pub head: String,
    pub base: String,
    pub title: String,
    pub body: String,
}

/// A review verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    Approve,
    RequestChanges,
}

impl Verdict {
    /// The event name a forge expects for this verdict.
    #[must_use]
    pub const fn as_event(self) -> &'static str {
        match self {
            Verdict::Approve => "APPROVE",
            Verdict::RequestChanges => "REQUEST_CHANGES",
        }
    }
}

/// One inline review comment at a file and line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReviewComment {
    pub path: String,
    pub line: u64,
    pub body: String,
}

/// A review to post.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewReview {
    pub pull_request: PullRequestId,
    pub head_sha: String,
    pub verdict: Verdict,
    pub body: String,
    pub comments: Vec<ReviewComment>,
}

/// A review the forge posted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PostedReview {
    pub verdict: Verdict,
    pub advisory: bool,
    pub inline: usize,
    pub id: String,
    pub state: String,
    pub url: String,
}

/// One check reported by a forge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    pub name: String,
    pub state: String,
    pub bucket: String,
}

/// The checks a forge reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckStatus {
    pub checks: Vec<Check>,
}

/// The result of a read: found data, found nothing, or could not read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReadOutcome<T> {
    Found(T),
    Empty,
    Unknown(String),
}

/// A forge operation failed: the forge refused it, or it could not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForgeError {
    Rejected(String),
    Failed(String),
}

impl fmt::Display for ForgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ForgeError::Rejected(text) | ForgeError::Failed(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for ForgeError {}

/// What an identity may do about verdicts. An empty known list means the
/// identity may post a comment but no verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerdictCapability {
    Known(Vec<Verdict>),
    Unknown(String),
}

/// Whether an identity can read the check status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReadCapability {
    Readable,
    Unreadable(String),
    Unknown(String),
}

/// What the current identity may do on a pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    pub verdicts: VerdictCapability,
    pub check_status: ReadCapability,
}

/// The seam between the factory and a code host.
pub trait Forge: Send + Sync {
    /// Creates a branch and returns what the forge recorded.
    ///
    /// # Errors
    /// Returns an error when the forge refuses or the operation cannot run.
    fn create_branch(&self, branch: &NewBranch) -> Result<Branch, ForgeError>;

    /// Opens a pull request and returns its identifier.
    ///
    /// # Errors
    /// Returns an error when the forge refuses or the operation cannot run.
    fn open_pull_request(&self, request: &NewPullRequest) -> Result<PullRequestId, ForgeError>;

    /// Replaces the block in a pull request's description.
    ///
    /// # Errors
    /// Returns an error when the description cannot be read or written.
    fn write_status_block(&self, pr: &PullRequestId, block: &str) -> Result<(), ForgeError>;

    /// Posts a review, falling back to a comment on a self-review refusal.
    ///
    /// # Errors
    /// Returns an error when the review is rejected or the post fails.
    fn post_review(&self, request: &NewReview) -> Result<PostedReview, ForgeError>;

    /// Reads the check status, telling "could not read" from "found nothing".
    fn read_check_status(&self, pr: &PullRequestId) -> ReadOutcome<CheckStatus>;

    /// Reports what the current identity may do on a pull request.
    fn capabilities(&self, pr: &PullRequestId) -> Capabilities;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TinyForge;

    #[allow(clippy::unused_self)]
    impl Forge for TinyForge {
        fn create_branch(&self, _branch: &NewBranch) -> Result<Branch, ForgeError> {
            Ok(Branch {
                name: String::new(),
                sha: String::new(),
            })
        }

        fn open_pull_request(
            &self,
            _request: &NewPullRequest,
        ) -> Result<PullRequestId, ForgeError> {
            Ok(PullRequestId {
                repo: String::new(),
                pr: String::new(),
            })
        }

        fn write_status_block(&self, _pr: &PullRequestId, _block: &str) -> Result<(), ForgeError> {
            Ok(())
        }

        fn post_review(&self, _request: &NewReview) -> Result<PostedReview, ForgeError> {
            Ok(PostedReview {
                verdict: Verdict::Approve,
                advisory: false,
                inline: 0,
                id: String::new(),
                state: String::new(),
                url: String::new(),
            })
        }

        fn read_check_status(&self, _pr: &PullRequestId) -> ReadOutcome<CheckStatus> {
            ReadOutcome::Empty
        }

        fn capabilities(&self, _pr: &PullRequestId) -> Capabilities {
            Capabilities {
                verdicts: VerdictCapability::Known(Vec::new()),
                check_status: ReadCapability::Readable,
            }
        }
    }

    #[test]
    fn verdict_as_event_names_the_forge_event() {
        assert_eq!(Verdict::Approve.as_event(), "APPROVE");
        assert_eq!(Verdict::RequestChanges.as_event(), "REQUEST_CHANGES");
    }

    #[test]
    fn forge_error_display_prints_the_inner_text_only() {
        assert_eq!(
            ForgeError::Rejected("refused".to_string()).to_string(),
            "refused"
        );
        assert_eq!(
            ForgeError::Failed("failed".to_string()).to_string(),
            "failed"
        );
    }

    #[test]
    fn serde_output_is_plain_json() {
        assert_eq!(
            serde_json::to_string(&Verdict::RequestChanges).expect("serializes"),
            r#""request-changes""#
        );
        assert_eq!(
            serde_json::to_string(&ReadOutcome::<Check>::Unknown("boom".to_string()))
                .expect("serializes"),
            r#"{"unknown":"boom"}"#
        );
        let capabilities = Capabilities {
            verdicts: VerdictCapability::Known(vec![Verdict::Approve]),
            check_status: ReadCapability::Unreadable("not accessible".to_string()),
        };
        assert_eq!(
            serde_json::to_string(&capabilities).expect("serializes"),
            r#"{"verdicts":{"known":["approve"]},"check_status":{"unreadable":"not accessible"}}"#
        );
    }

    #[test]
    fn the_module_text_names_no_provider() {
        let text = include_str!("mod.rs").to_lowercase();
        let banned = [["git", "hub"].concat(), ["g", "h"].concat()];
        let words: Vec<&str> = text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .collect();
        for word in banned {
            assert!(
                !words.contains(&word.as_str()),
                "the provider-neutral module must not name a provider"
            );
        }
    }

    #[test]
    fn the_trait_is_object_safe() {
        let forge = TinyForge;
        let dynamic: &dyn Forge = &forge;
        let id = PullRequestId {
            repo: "open-software-factory/demo".to_string(),
            pr: "1".to_string(),
        };
        assert!(matches!(dynamic.read_check_status(&id), ReadOutcome::Empty));
    }
}
