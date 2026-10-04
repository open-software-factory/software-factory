//! An in-memory [`Forge`] test double: it records every call as plain data and
//! performs no process, file, network or clock work of any kind.

use crate::forge::{
    Branch, Capabilities, CheckStatus, Forge, ForgeError, NewBranch, NewPullRequest, NewReview,
    PostedReview, PullRequestId, ReadCapability, ReadOutcome, Verdict, VerdictCapability,
};
use std::cell::RefCell;

/// One call the double recorded, in the order it happened.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum Call {
    CreateBranch(NewBranch),
    OpenPullRequest(NewPullRequest),
    WriteStatusBlock { pr: PullRequestId, block: String },
    PostReview(NewReview),
    ReadCheckStatus(PullRequestId),
    Capabilities(PullRequestId),
}

/// The four operations the double can be told to fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    CreateBranch,
    OpenPullRequest,
    WriteStatusBlock,
    PostReview,
}

/// A [`Forge`] that answers from memory and records every call.
pub struct FakeForge {
    calls: RefCell<Vec<Call>>,
    failures: Vec<(Operation, ForgeError)>,
    /// The answer [`Forge::read_check_status`] returns.
    pub check_status: ReadOutcome<CheckStatus>,
    /// The answer [`Forge::capabilities`] returns.
    pub capabilities: Capabilities,
}

impl FakeForge {
    /// A double with the documented defaults and no scripted failure.
    #[must_use]
    pub fn new() -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            failures: Vec::new(),
            check_status: ReadOutcome::Empty,
            capabilities: Capabilities {
                verdicts: VerdictCapability::Known(vec![Verdict::Approve, Verdict::RequestChanges]),
                check_status: ReadCapability::Readable,
            },
        }
    }

    /// Makes `operation` fail with `error` after still recording its call.
    #[must_use]
    pub fn fail(mut self, operation: Operation, error: ForgeError) -> Self {
        self.failures.push((operation, error));
        self
    }

    /// Every call recorded so far, in order.
    #[must_use]
    pub fn calls(&self) -> Vec<Call> {
        self.calls.borrow().clone()
    }

    /// The scripted failure for `operation`, if any.
    fn failure(&self, operation: Operation) -> Option<ForgeError> {
        self.failures
            .iter()
            .find(|(failed, _)| *failed == operation)
            .map(|(_, error)| error.clone())
    }
}

impl Default for FakeForge {
    fn default() -> Self {
        Self::new()
    }
}

impl Forge for FakeForge {
    fn create_branch(&self, branch: &NewBranch) -> Result<Branch, ForgeError> {
        self.calls
            .borrow_mut()
            .push(Call::CreateBranch(branch.clone()));
        if let Some(error) = self.failure(Operation::CreateBranch) {
            return Err(error);
        }
        Ok(Branch {
            name: branch.name.clone(),
            sha: branch.from_sha.clone(),
        })
    }

    fn open_pull_request(&self, request: &NewPullRequest) -> Result<PullRequestId, ForgeError> {
        self.calls
            .borrow_mut()
            .push(Call::OpenPullRequest(request.clone()));
        if let Some(error) = self.failure(Operation::OpenPullRequest) {
            return Err(error);
        }
        Ok(PullRequestId {
            repo: request.repo.clone(),
            pr: "1".to_string(),
        })
    }

    fn write_status_block(&self, pr: &PullRequestId, block: &str) -> Result<(), ForgeError> {
        self.calls.borrow_mut().push(Call::WriteStatusBlock {
            pr: pr.clone(),
            block: block.to_string(),
        });
        if let Some(error) = self.failure(Operation::WriteStatusBlock) {
            return Err(error);
        }
        Ok(())
    }

    fn post_review(&self, request: &NewReview) -> Result<PostedReview, ForgeError> {
        self.calls
            .borrow_mut()
            .push(Call::PostReview(request.clone()));
        if let Some(error) = self.failure(Operation::PostReview) {
            return Err(error);
        }
        Ok(PostedReview {
            verdict: request.verdict,
            advisory: false,
            inline: request.comments.len(),
            id: "1".to_string(),
            state: "COMMENTED".to_string(),
            url: "https://example.invalid/review/1".to_string(),
        })
    }

    fn read_check_status(&self, pr: &PullRequestId) -> ReadOutcome<CheckStatus> {
        self.calls
            .borrow_mut()
            .push(Call::ReadCheckStatus(pr.clone()));
        self.check_status.clone()
    }

    fn capabilities(&self, pr: &PullRequestId) -> Capabilities {
        self.calls.borrow_mut().push(Call::Capabilities(pr.clone()));
        self.capabilities.clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::forge::fake::{Call, FakeForge, Operation};
    use crate::forge::{
        Branch, Capabilities, Check, CheckStatus, Forge, ForgeError, NewBranch, NewPullRequest,
        NewReview, PostedReview, PullRequestId, ReadCapability, ReadOutcome, ReviewComment,
        Verdict, VerdictCapability,
    };

    fn branch() -> NewBranch {
        NewBranch {
            repo: "open-software-factory/demo".to_string(),
            name: "work/item-1".to_string(),
            from_sha: "cafe1234".to_string(),
        }
    }

    fn pull_request() -> NewPullRequest {
        NewPullRequest {
            repo: "open-software-factory/demo".to_string(),
            head: "work/item-1".to_string(),
            base: "main".to_string(),
            title: "A change".to_string(),
            body: "Body.".to_string(),
        }
    }

    fn pull_request_id() -> PullRequestId {
        PullRequestId {
            repo: "open-software-factory/demo".to_string(),
            pr: "1".to_string(),
        }
    }

    fn review() -> NewReview {
        NewReview {
            pull_request: pull_request_id(),
            head_sha: "cafe1234".to_string(),
            verdict: Verdict::RequestChanges,
            body: "Summary.".to_string(),
            comments: vec![ReviewComment {
                path: "src/lib.rs".to_string(),
                line: 7,
                body: "Fix this.".to_string(),
            }],
        }
    }

    #[test]
    fn create_branch_records_the_call_and_echoes_the_request() {
        let forge = FakeForge::new();
        let result = forge.create_branch(&branch()).expect("creates");
        assert_eq!(
            result,
            Branch {
                name: "work/item-1".to_string(),
                sha: "cafe1234".to_string(),
            }
        );
        assert_eq!(forge.calls(), vec![Call::CreateBranch(branch())]);
    }

    #[test]
    fn open_pull_request_records_the_call_and_returns_the_first_number() {
        let forge = FakeForge::new();
        let result = forge.open_pull_request(&pull_request()).expect("opens");
        assert_eq!(result, pull_request_id());
        assert_eq!(forge.calls(), vec![Call::OpenPullRequest(pull_request())]);
    }

    #[test]
    fn write_status_block_records_the_call_and_writes_nothing() {
        let forge = FakeForge::new();
        forge
            .write_status_block(&pull_request_id(), "the block")
            .expect("writes");
        assert_eq!(
            forge.calls(),
            vec![Call::WriteStatusBlock {
                pr: pull_request_id(),
                block: "the block".to_string(),
            }]
        );
    }

    #[test]
    fn post_review_records_the_call_and_returns_the_default() {
        let forge = FakeForge::new();
        let result = forge.post_review(&review()).expect("posts");
        assert_eq!(
            result,
            PostedReview {
                verdict: Verdict::RequestChanges,
                advisory: false,
                inline: 1,
                id: "1".to_string(),
                state: "COMMENTED".to_string(),
                url: "https://example.invalid/review/1".to_string(),
            }
        );
        assert_eq!(forge.calls(), vec![Call::PostReview(review())]);
    }

    #[test]
    fn read_check_status_records_the_call_and_defaults_to_empty() {
        let forge = FakeForge::new();
        assert_eq!(
            forge.read_check_status(&pull_request_id()),
            ReadOutcome::Empty
        );
        assert_eq!(
            forge.calls(),
            vec![Call::ReadCheckStatus(pull_request_id())]
        );
    }

    #[test]
    fn capabilities_records_the_call_and_defaults_to_known_verdicts() {
        let forge = FakeForge::new();
        assert_eq!(
            forge.capabilities(&pull_request_id()),
            Capabilities {
                verdicts: VerdictCapability::Known(
                    vec![Verdict::Approve, Verdict::RequestChanges,]
                ),
                check_status: ReadCapability::Readable,
            }
        );
        assert_eq!(forge.calls(), vec![Call::Capabilities(pull_request_id())]);
    }

    #[test]
    fn fail_create_branch_returns_the_error_after_recording() {
        let forge = FakeForge::new().fail(
            Operation::CreateBranch,
            ForgeError::Rejected("no".to_string()),
        );
        let error = forge.create_branch(&branch()).expect_err("fails");
        assert_eq!(error, ForgeError::Rejected("no".to_string()));
        assert_eq!(forge.calls(), vec![Call::CreateBranch(branch())]);
    }

    #[test]
    fn fail_open_pull_request_returns_the_error_after_recording() {
        let forge = FakeForge::new().fail(
            Operation::OpenPullRequest,
            ForgeError::Rejected("no".to_string()),
        );
        let error = forge.open_pull_request(&pull_request()).expect_err("fails");
        assert_eq!(error, ForgeError::Rejected("no".to_string()));
        assert_eq!(forge.calls(), vec![Call::OpenPullRequest(pull_request())]);
    }

    #[test]
    fn fail_write_status_block_returns_the_error_after_recording() {
        let forge = FakeForge::new().fail(
            Operation::WriteStatusBlock,
            ForgeError::Rejected("no".to_string()),
        );
        let error = forge
            .write_status_block(&pull_request_id(), "the block")
            .expect_err("fails");
        assert_eq!(error, ForgeError::Rejected("no".to_string()));
        assert_eq!(
            forge.calls(),
            vec![Call::WriteStatusBlock {
                pr: pull_request_id(),
                block: "the block".to_string(),
            }]
        );
    }

    #[test]
    fn fail_post_review_returns_the_error_after_recording() {
        let forge = FakeForge::new().fail(
            Operation::PostReview,
            ForgeError::Rejected("no".to_string()),
        );
        let error = forge.post_review(&review()).expect_err("fails");
        assert_eq!(error, ForgeError::Rejected("no".to_string()));
        assert_eq!(forge.calls(), vec![Call::PostReview(review())]);
    }

    #[test]
    fn the_check_status_answer_is_returned_as_scripted() {
        let status = CheckStatus {
            checks: vec![Check {
                name: "lint".to_string(),
                state: "SUCCESS".to_string(),
                bucket: "pass".to_string(),
            }],
        };
        let mut forge = FakeForge::new();
        forge.check_status = ReadOutcome::Found(status.clone());
        assert_eq!(
            forge.read_check_status(&pull_request_id()),
            ReadOutcome::Found(status)
        );
    }

    #[test]
    fn the_capabilities_answer_is_returned_as_scripted() {
        let capabilities = Capabilities {
            verdicts: VerdictCapability::Unknown("unknown".to_string()),
            check_status: ReadCapability::Unreadable("no access".to_string()),
        };
        let mut forge = FakeForge::new();
        forge.capabilities = capabilities.clone();
        assert_eq!(forge.capabilities(&pull_request_id()), capabilities);
    }

    #[test]
    fn the_module_text_names_no_provider_or_command() {
        let text = include_str!("fake.rs").to_lowercase();
        let banned = [["git", "hub"].concat(), ["g", "h"].concat()];
        let words: Vec<&str> = text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .collect();
        for word in banned {
            assert!(
                !words.contains(&word.as_str()),
                "the provider-neutral double must not name a provider or command"
            );
        }
    }
}
