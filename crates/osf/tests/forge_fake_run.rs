//! A full run driven only by the in-memory forge double: no process, file,
//! network or clock is reached anywhere in this test.

use osf::forge::fake::{Call, FakeForge, Operation};
use osf::forge::report::{report_change, ReportRequest, ReviewDraft, Step};
use osf::forge::{
    Branch, Forge, ForgeError, NewBranch, NewPullRequest, NewReview, PostedReview, PullRequestId,
    ReadOutcome, ReviewComment, Verdict,
};

const REPO: &str = "open-software-factory/demo";

fn work_branch() -> NewBranch {
    NewBranch {
        repo: REPO.to_string(),
        name: "work/item-1".to_string(),
        from_sha: "base1234".to_string(),
    }
}

fn opened() -> PullRequestId {
    PullRequestId {
        repo: REPO.to_string(),
        pr: "1".to_string(),
    }
}

fn report_request(branch: &Branch) -> ReportRequest {
    ReportRequest {
        pull_request: NewPullRequest {
            repo: REPO.to_string(),
            head: branch.name.clone(),
            base: "main".to_string(),
            title: "A change".to_string(),
            body: "Body.".to_string(),
        },
        status_block: "the block".to_string(),
        review: ReviewDraft {
            head_sha: branch.sha.clone(),
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

fn review_to_post(branch: &Branch) -> NewReview {
    NewReview {
        pull_request: opened(),
        head_sha: branch.sha.clone(),
        verdict: Verdict::RequestChanges,
        body: "Summary.".to_string(),
        comments: vec![ReviewComment {
            path: "src/lib.rs".to_string(),
            line: 7,
            body: "Fix this.".to_string(),
        }],
    }
}

fn full_call_list(branch: &Branch) -> Vec<Call> {
    vec![
        Call::CreateBranch(work_branch()),
        Call::OpenPullRequest(NewPullRequest {
            repo: REPO.to_string(),
            head: branch.name.clone(),
            base: "main".to_string(),
            title: "A change".to_string(),
            body: "Body.".to_string(),
        }),
        Call::WriteStatusBlock {
            pr: opened(),
            block: "the block".to_string(),
        },
        Call::Capabilities(opened()),
        Call::PostReview(review_to_post(branch)),
        Call::ReadCheckStatus(opened()),
    ]
}

#[test]
fn a_full_run_drives_every_call_in_order() {
    let forge = FakeForge::new();
    let branch = forge
        .create_branch(&work_branch())
        .expect("creates the branch");
    assert_eq!(branch.name, "work/item-1");
    assert_eq!(branch.sha, "base1234");
    let request = report_request(&branch);
    assert_eq!(request.pull_request.head, branch.name);
    let report = report_change(&forge, &request).expect("reports");
    assert_eq!(report.pull_request, opened());
    assert_eq!(
        report.review,
        PostedReview {
            verdict: Verdict::RequestChanges,
            advisory: false,
            inline: 1,
            id: "1".to_string(),
            state: "COMMENTED".to_string(),
            url: "https://example.invalid/review/1".to_string(),
        }
    );
    assert_eq!(report.checks, ReadOutcome::Empty);
    assert_eq!(forge.calls(), full_call_list(&branch));
}

#[test]
fn a_failed_status_block_stops_the_run_and_names_the_step() {
    let forge = FakeForge::new().fail(
        Operation::WriteStatusBlock,
        ForgeError::Rejected("refused".to_string()),
    );
    let branch = forge
        .create_branch(&work_branch())
        .expect("creates the branch");
    let request = report_request(&branch);
    let error = report_change(&forge, &request).expect_err("stops at the status block");
    assert_eq!(error.step, Step::WriteStatusBlock);
    assert_eq!(error.to_string(), "write status block: refused");
    let calls = forge.calls();
    assert_eq!(
        calls,
        vec![
            Call::CreateBranch(work_branch()),
            Call::OpenPullRequest(request.pull_request.clone()),
            Call::WriteStatusBlock {
                pr: opened(),
                block: "the block".to_string(),
            },
        ]
    );
    assert!(
        !calls.iter().any(|call| matches!(call, Call::PostReview(_))),
        "a stopped run records no review"
    );
}
