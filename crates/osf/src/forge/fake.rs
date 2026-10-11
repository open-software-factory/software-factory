//! An in-memory [`Forge`] test double: it records every call as plain data and
//! performs no process, file, network or clock work of any kind.

use crate::forge::{
    Branch, Capabilities, CheckStatus, Forge, ForgeError, NewBranch, NewPullRequest, NewReview,
    PostedReview, PullRequestId, ReadCapability, ReadOutcome, Verdict, VerdictCapability,
};
use std::sync::Mutex;

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

/// A sequence of scripted answers and the cursor of the one to take next.
struct Script<T: Clone> {
    answers: Vec<T>,
    cursor: Mutex<usize>,
}

impl<T: Clone> Script<T> {
    /// A script over `answers`; an empty script yields nothing.
    fn new(answers: Vec<T>) -> Self {
        Self {
            answers,
            cursor: Mutex::new(0),
        }
    }

    /// The next answer, repeating the last one once the script runs out.
    fn take_next(&self) -> Option<T> {
        let last = self.answers.len().checked_sub(1)?;
        let mut cursor = self
            .cursor
            .lock()
            .expect("the script cursor is never poisoned");
        let index = (*cursor).min(last);
        *cursor = index.saturating_add(1);
        self.answers.get(index).cloned()
    }
}

/// A [`Forge`] that answers from memory, records every call, and can script a
/// sequence of answers per operation.
pub struct FakeForge {
    calls: Mutex<Vec<Call>>,
    failures: Vec<(Operation, ForgeError)>,
    script_create_branch: Script<Result<Branch, ForgeError>>,
    script_open_pull_request: Script<Result<PullRequestId, ForgeError>>,
    script_write_status_block: Script<Result<(), ForgeError>>,
    script_post_review: Script<Result<PostedReview, ForgeError>>,
    script_check_status: Script<ReadOutcome<CheckStatus>>,
    script_capabilities: Script<Capabilities>,
    /// The answer [`Forge::read_check_status`] returns when no script is set.
    pub check_status: ReadOutcome<CheckStatus>,
    /// The answer [`Forge::capabilities`] returns when no script is set.
    pub capabilities: Capabilities,
}

impl FakeForge {
    /// A double with the documented defaults and no scripted failure.
    #[must_use]
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            failures: Vec::new(),
            script_create_branch: Script::new(Vec::new()),
            script_open_pull_request: Script::new(Vec::new()),
            script_write_status_block: Script::new(Vec::new()),
            script_post_review: Script::new(Vec::new()),
            script_check_status: Script::new(Vec::new()),
            script_capabilities: Script::new(Vec::new()),
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

    /// Scripts the answers [`Forge::create_branch`] returns, in order.
    #[must_use]
    pub fn script_create_branch(mut self, answers: Vec<Result<Branch, ForgeError>>) -> Self {
        self.script_create_branch = Script::new(answers);
        self
    }

    /// Scripts the answers [`Forge::open_pull_request`] returns, in order.
    #[must_use]
    pub fn script_open_pull_request(
        mut self,
        answers: Vec<Result<PullRequestId, ForgeError>>,
    ) -> Self {
        self.script_open_pull_request = Script::new(answers);
        self
    }

    /// Scripts the answers [`Forge::write_status_block`] returns, in order.
    #[must_use]
    pub fn script_write_status_block(mut self, answers: Vec<Result<(), ForgeError>>) -> Self {
        self.script_write_status_block = Script::new(answers);
        self
    }

    /// Scripts the answers [`Forge::post_review`] returns, in order.
    #[must_use]
    pub fn script_post_review(mut self, answers: Vec<Result<PostedReview, ForgeError>>) -> Self {
        self.script_post_review = Script::new(answers);
        self
    }

    /// Scripts the answers [`Forge::read_check_status`] returns, in order.
    #[must_use]
    pub fn script_check_status(mut self, answers: Vec<ReadOutcome<CheckStatus>>) -> Self {
        self.script_check_status = Script::new(answers);
        self
    }

    /// Scripts the answers [`Forge::capabilities`] returns, in order.
    #[must_use]
    pub fn script_capabilities(mut self, answers: Vec<Capabilities>) -> Self {
        self.script_capabilities = Script::new(answers);
        self
    }

    /// Every call recorded so far, in order.
    #[must_use]
    pub fn calls(&self) -> Vec<Call> {
        self.call_log().clone()
    }

    /// Records `call` before the operation answers.
    fn record(&self, call: Call) {
        self.call_log().push(call);
    }

    /// The call log, recovering from a poisoned lock rather than panicking.
    fn call_log(&self) -> std::sync::MutexGuard<'_, Vec<Call>> {
        self.calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
        self.record(Call::CreateBranch(branch.clone()));
        if let Some(answer) = self.script_create_branch.take_next() {
            return answer;
        }
        if let Some(error) = self.failure(Operation::CreateBranch) {
            return Err(error);
        }
        Ok(Branch {
            name: branch.name.clone(),
            sha: branch.from_sha.clone(),
        })
    }

    fn open_pull_request(&self, request: &NewPullRequest) -> Result<PullRequestId, ForgeError> {
        self.record(Call::OpenPullRequest(request.clone()));
        if let Some(answer) = self.script_open_pull_request.take_next() {
            return answer;
        }
        if let Some(error) = self.failure(Operation::OpenPullRequest) {
            return Err(error);
        }
        Ok(PullRequestId {
            repo: request.repo.clone(),
            pr: "1".to_string(),
        })
    }

    fn write_status_block(&self, pr: &PullRequestId, block: &str) -> Result<(), ForgeError> {
        self.record(Call::WriteStatusBlock {
            pr: pr.clone(),
            block: block.to_string(),
        });
        if let Some(answer) = self.script_write_status_block.take_next() {
            return answer;
        }
        if let Some(error) = self.failure(Operation::WriteStatusBlock) {
            return Err(error);
        }
        Ok(())
    }

    fn post_review(&self, request: &NewReview) -> Result<PostedReview, ForgeError> {
        self.record(Call::PostReview(request.clone()));
        if let Some(answer) = self.script_post_review.take_next() {
            return answer;
        }
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
        self.record(Call::ReadCheckStatus(pr.clone()));
        self.script_check_status
            .take_next()
            .unwrap_or_else(|| self.check_status.clone())
    }

    fn capabilities(&self, pr: &PullRequestId) -> Capabilities {
        self.record(Call::Capabilities(pr.clone()));
        self.script_capabilities
            .take_next()
            .unwrap_or_else(|| self.capabilities.clone())
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

    /// A one-check status carrying `state` and `bucket`.
    fn check_status(state: &str, bucket: &str) -> CheckStatus {
        CheckStatus {
            checks: vec![Check {
                name: "ci".to_string(),
                state: state.to_string(),
                bucket: bucket.to_string(),
            }],
        }
    }

    #[test]
    fn a_check_status_script_answers_in_order_then_repeats_the_last() {
        let pending = ReadOutcome::Found(check_status("PENDING", "pending"));
        let pass = ReadOutcome::Found(check_status("SUCCESS", "pass"));
        let forge = FakeForge::new().script_check_status(vec![pending.clone(), pass.clone()]);
        assert_eq!(forge.read_check_status(&pull_request_id()), pending);
        assert_eq!(forge.read_check_status(&pull_request_id()), pass);
        assert_eq!(forge.read_check_status(&pull_request_id()), pass);
        assert_eq!(
            forge.calls(),
            vec![
                Call::ReadCheckStatus(pull_request_id()),
                Call::ReadCheckStatus(pull_request_id()),
                Call::ReadCheckStatus(pull_request_id()),
            ]
        );
    }

    #[test]
    fn a_capabilities_script_answers_in_order() {
        let first = Capabilities {
            verdicts: VerdictCapability::Known(Vec::new()),
            check_status: ReadCapability::Readable,
        };
        let second = Capabilities {
            verdicts: VerdictCapability::Unknown("unknown".to_string()),
            check_status: ReadCapability::Unreadable("no access".to_string()),
        };
        let forge = FakeForge::new().script_capabilities(vec![first.clone(), second.clone()]);
        assert_eq!(forge.capabilities(&pull_request_id()), first);
        assert_eq!(forge.capabilities(&pull_request_id()), second);
    }

    #[test]
    fn an_open_pull_request_script_fails_then_succeeds_then_repeats() {
        let custom = PullRequestId {
            repo: "open-software-factory/demo".to_string(),
            pr: "9".to_string(),
        };
        let forge = FakeForge::new().script_open_pull_request(vec![
            Err(ForgeError::Failed("boom".to_string())),
            Ok(custom.clone()),
        ]);
        assert_eq!(
            forge.open_pull_request(&pull_request()),
            Err(ForgeError::Failed("boom".to_string()))
        );
        assert_eq!(forge.open_pull_request(&pull_request()), Ok(custom.clone()));
        assert_eq!(forge.open_pull_request(&pull_request()), Ok(custom));
    }

    #[test]
    fn a_post_review_script_fails_then_succeeds() {
        let posted = PostedReview {
            verdict: Verdict::RequestChanges,
            advisory: true,
            inline: 1,
            id: "9".to_string(),
            state: "COMMENTED".to_string(),
            url: "https://example.invalid/review/9".to_string(),
        };
        let forge = FakeForge::new().script_post_review(vec![
            Err(ForgeError::Rejected("no".to_string())),
            Ok(posted.clone()),
        ]);
        assert_eq!(
            forge.post_review(&review()),
            Err(ForgeError::Rejected("no".to_string()))
        );
        assert_eq!(forge.post_review(&review()), Ok(posted.clone()));
        assert_eq!(forge.post_review(&review()), Ok(posted));
    }

    #[test]
    fn a_write_status_block_script_fails_then_succeeds() {
        let forge = FakeForge::new()
            .script_write_status_block(vec![Err(ForgeError::Failed("boom".to_string())), Ok(())]);
        assert_eq!(
            forge.write_status_block(&pull_request_id(), "the block"),
            Err(ForgeError::Failed("boom".to_string()))
        );
        assert_eq!(
            forge.write_status_block(&pull_request_id(), "the block"),
            Ok(())
        );
    }

    #[test]
    fn a_create_branch_script_returns_the_custom_branch() {
        let custom = Branch {
            name: "scripted".to_string(),
            sha: "deadbeef".to_string(),
        };
        let forge = FakeForge::new().script_create_branch(vec![Ok(custom.clone())]);
        assert_eq!(forge.create_branch(&branch()), Ok(custom));
    }

    #[test]
    fn a_scripted_answer_wins_over_a_failure() {
        let custom = Branch {
            name: "scripted".to_string(),
            sha: "deadbeef".to_string(),
        };
        let forge = FakeForge::new()
            .fail(
                Operation::CreateBranch,
                ForgeError::Rejected("no".to_string()),
            )
            .script_create_branch(vec![Ok(custom.clone())]);
        assert_eq!(forge.create_branch(&branch()), Ok(custom));
    }

    #[test]
    fn an_empty_script_keeps_the_default_answer() {
        let forge = FakeForge::new().script_create_branch(Vec::new());
        assert_eq!(
            forge.create_branch(&branch()),
            Ok(Branch {
                name: "work/item-1".to_string(),
                sha: "cafe1234".to_string(),
            })
        );
    }

    fn assert_send_sync<T: Send + Sync + ?Sized>() {}

    #[test]
    fn the_fake_forge_is_send_and_sync() {
        assert_send_sync::<FakeForge>();
        assert_send_sync::<dyn Forge>();
    }

    #[test]
    fn the_fake_forge_can_be_shared_by_two_threads() {
        let forge = FakeForge::new();
        std::thread::scope(|scope| {
            let forge = &forge;
            scope.spawn(move || {
                forge.read_check_status(&pull_request_id());
                forge.capabilities(&pull_request_id());
            });
            scope.spawn(move || {
                forge.read_check_status(&pull_request_id());
                forge.capabilities(&pull_request_id());
            });
        });
        let calls = forge.calls();
        assert_eq!(calls.len(), 4, "four calls were recorded: {calls:?}");
        assert_eq!(
            calls
                .iter()
                .filter(|call| matches!(call, Call::ReadCheckStatus(_)))
                .count(),
            2
        );
        assert_eq!(
            calls
                .iter()
                .filter(|call| matches!(call, Call::Capabilities(_)))
                .count(),
            2
        );
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
