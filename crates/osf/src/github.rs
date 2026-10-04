//! The first [`Forge`] adapter: it drives the installed command-line tool
//! over the pull request, status-block, review and check-status work that
//! already exists. Nothing above this module names the provider.

use crate::forge::{
    Branch, Capabilities, Check, CheckStatus, Forge, ForgeError, NewBranch, NewPullRequest,
    NewReview, PostedReview, PullRequestId, ReadCapability, ReadOutcome, ReviewComment, Verdict,
    VerdictCapability,
};
use crate::pr_status::{self, GhClient, GhOutput, StatusError};
use crate::review;
use serde_json::Value;
use std::io::Write as _;
use std::process::{Command, Stdio};

/// Wraps a [`GhClient`] as a [`Forge`].
pub struct GitHub<C: GhClient> {
    client: C,
}

impl<C: GhClient> GitHub<C> {
    /// Wraps `client` as a forge.
    #[must_use]
    pub fn new(client: C) -> Self {
        Self { client }
    }

    /// The wrapped client.
    #[must_use]
    pub fn client(&self) -> &C {
        &self.client
    }

    /// The head commit of a pull request's branch, the one a review anchors
    /// its comments to.
    ///
    /// # Errors
    /// Returns an error if the command cannot run, fails, or its answer
    /// holds no head commit.
    pub fn head_commit(&self, repo: &str, pr: u64) -> Result<String, String> {
        let argv = vec![
            "pr".to_string(),
            "view".to_string(),
            pr.to_string(),
            "--repo".to_string(),
            repo.to_string(),
            "--json".to_string(),
            "headRefOid".to_string(),
        ];
        let out = self.client.run(&argv, None).map_err(|e| e.to_string())?;
        if !out.success {
            return Err(format!(
                "could not read the head commit of {repo}#{pr}: {}",
                out.stderr.trim()
            ));
        }
        review::parse_head_sha_json(&out.stdout).map_err(|e| format!("{e} ({repo}#{pr})"))
    }
}

impl GitHub<RealGh> {
    /// The adapter over the real command-line tool.
    #[must_use]
    pub fn real() -> Self {
        Self::new(RealGh)
    }
}

#[allow(clippy::unused_self)]
impl<C: GhClient> Forge for GitHub<C> {
    fn create_branch(&self, branch: &NewBranch) -> Result<Branch, ForgeError> {
        let argv = create_branch_argv(&branch.repo);
        let payload = create_branch_payload(branch);
        let text = run_gh_json(&self.client, &argv, &payload)?;
        parse_branch_response(branch, &text)
    }

    fn open_pull_request(&self, request: &NewPullRequest) -> Result<PullRequestId, ForgeError> {
        let argv = open_pull_request_argv(&request.repo);
        let payload = open_pull_request_payload(request);
        let text = run_gh_json(&self.client, &argv, &payload)?;
        parse_opened_pull_request(&request.repo, &text)
    }

    fn write_status_block(&self, pr: &PullRequestId, block: &str) -> Result<(), ForgeError> {
        let body = self
            .client
            .view_body(&pr.repo, &pr.pr)
            .map_err(|e| ForgeError::Failed(e.to_string()))?;
        let new_body =
            pr_status::apply(&body, block).map_err(|e| ForgeError::Failed(e.to_string()))?;
        self.client
            .edit_body(&pr.repo, &pr.pr, &new_body)
            .map_err(|e| ForgeError::Failed(e.to_string()))
    }

    fn post_review(&self, request: &NewReview) -> Result<PostedReview, ForgeError> {
        let selector = &request.pull_request.pr;
        let number: u64 = selector.parse().map_err(|_| {
            ForgeError::Failed(format!(
                "the pull request selector '{selector}' is not a number"
            ))
        })?;
        let plan = review::Plan {
            verdict: to_review_verdict(request.verdict),
            body: request.body.clone(),
            comments: request.comments.iter().map(to_review_comment).collect(),
        };
        let repo = request.pull_request.repo.as_str();
        let poster = |payload: &Value| post_review_attempt(&self.client, repo, number, payload);
        let mut warn = |text: &str| eprintln!("{text}");
        match post_with_fallback(&poster, &plan, &request.head_sha, &mut warn) {
            review::Outcome::Reviewed {
                verdict,
                n_inline,
                id,
                state,
                url,
            } => Ok(PostedReview {
                verdict: from_review_verdict(verdict),
                advisory: false,
                inline: n_inline,
                id,
                state,
                url,
            }),
            review::Outcome::FallbackComment {
                verdict,
                n_inline,
                id,
                state,
                url,
            } => Ok(PostedReview {
                verdict: from_review_verdict(verdict),
                advisory: true,
                inline: n_inline,
                id,
                state,
                url,
            }),
            review::Outcome::Rejected(e) => Err(ForgeError::Rejected(e.to_string())),
            review::Outcome::PostFailed(e) => Err(ForgeError::Failed(e)),
        }
    }

    fn read_check_status(&self, pr: &PullRequestId) -> ReadOutcome<CheckStatus> {
        let text = match self.client.view_checks(&pr.repo, &pr.pr) {
            Ok(text) => text,
            Err(e) => return ReadOutcome::Unknown(e.to_string()),
        };
        let entries = match pr_status::parse_checks(&text) {
            Ok(entries) => entries,
            Err(e) => return ReadOutcome::Unknown(e.to_string()),
        };
        if entries.is_empty() {
            return ReadOutcome::Empty;
        }
        let checks = entries
            .into_iter()
            .map(|(name, state, bucket)| Check {
                name,
                state,
                bucket,
            })
            .collect();
        ReadOutcome::Found(CheckStatus { checks })
    }

    fn capabilities(&self, pr: &PullRequestId) -> Capabilities {
        let viewer = read_login(
            &self.client,
            &[
                "api".to_string(),
                "user".to_string(),
                "--jq".to_string(),
                ".login".to_string(),
            ],
        );
        let author = read_login(
            &self.client,
            &[
                "pr".to_string(),
                "view".to_string(),
                pr.pr.clone(),
                "--repo".to_string(),
                pr.repo.clone(),
                "--json".to_string(),
                "author".to_string(),
                "--jq".to_string(),
                ".author.login".to_string(),
            ],
        );
        let (success, stderr) = read_checks_status(&self.client, &pr.repo, &pr.pr);
        Capabilities {
            verdicts: verdict_capability(author.as_deref(), viewer.as_deref()),
            check_status: check_status_capability(success, &stderr),
        }
    }
}

/// The argv that creates a branch ref through the API.
#[must_use]
pub fn create_branch_argv(repo: &str) -> Vec<String> {
    vec![
        "api".to_string(),
        "--method".to_string(),
        "POST".to_string(),
        format!("repos/{repo}/git/refs"),
        "--input".to_string(),
        "-".to_string(),
    ]
}

/// The request body that creates a branch ref.
#[must_use]
pub fn create_branch_payload(branch: &NewBranch) -> Value {
    serde_json::json!({
        "ref": format!("refs/heads/{}", branch.name),
        "sha": branch.from_sha,
    })
}

/// Reads the created branch's name and commit out of the API response.
///
/// # Errors
/// Returns an error when the response is not JSON, or has no string `ref`
/// or `object.sha`.
pub fn parse_branch_response(request: &NewBranch, text: &str) -> Result<Branch, ForgeError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| ForgeError::Failed(format!("cannot read the branch response: {e}")))?;
    let reference = value
        .get("ref")
        .and_then(Value::as_str)
        .ok_or_else(|| branch_field_error(request, "ref"))?;
    let name = reference
        .strip_prefix("refs/heads/")
        .unwrap_or(reference)
        .to_string();
    let sha = value
        .get("object")
        .and_then(|object| object.get("sha"))
        .and_then(Value::as_str)
        .ok_or_else(|| branch_field_error(request, "object.sha"))?;
    Ok(Branch {
        name,
        sha: sha.to_string(),
    })
}

fn branch_field_error(request: &NewBranch, field: &str) -> ForgeError {
    let name = &request.name;
    ForgeError::Failed(format!("the branch response for '{name}' has no `{field}`"))
}

/// The argv that opens a pull request through the API.
#[must_use]
pub fn open_pull_request_argv(repo: &str) -> Vec<String> {
    vec![
        "api".to_string(),
        "--method".to_string(),
        "POST".to_string(),
        format!("repos/{repo}/pulls"),
        "--input".to_string(),
        "-".to_string(),
    ]
}

/// The request body that opens a pull request.
#[must_use]
pub fn open_pull_request_payload(request: &NewPullRequest) -> Value {
    serde_json::json!({
        "title": request.title,
        "head": request.head,
        "base": request.base,
        "body": request.body,
    })
}

/// Reads the new pull request's number out of the API response.
///
/// # Errors
/// Returns an error when the response is not JSON, or has no numeric
/// `number`.
pub fn parse_opened_pull_request(repo: &str, text: &str) -> Result<PullRequestId, ForgeError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| ForgeError::Failed(format!("cannot read the pull request response: {e}")))?;
    let number = value
        .get("number")
        .and_then(Value::as_u64)
        .ok_or_else(|| ForgeError::Failed("the pull request response has no number".to_string()))?;
    Ok(PullRequestId {
        repo: repo.to_string(),
        pr: number.to_string(),
    })
}

/// Runs one command-line POST and returns stdout, or a never-empty failure text.
fn run_gh_json(
    client: &dyn GhClient,
    argv: &[String],
    payload: &Value,
) -> Result<String, ForgeError> {
    let stdin = payload.to_string();
    let out = client
        .run(argv, Some(&stdin))
        .map_err(|e| ForgeError::Failed(e.to_string()))?;
    if !out.success {
        return Err(ForgeError::Failed(gh_failure_text(
            &out.status,
            &out.stdout,
            &out.stderr,
        )));
    }
    Ok(out.stdout)
}

/// The never-empty failure text for one failed command.
fn gh_failure_text(status: &str, stdout: &str, stderr: &str) -> String {
    let stderr = stderr.trim();
    let message = serde_json::from_str::<Value>(stdout.trim())
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let message = message.as_deref().unwrap_or("").trim();
    match (message.is_empty(), stderr.is_empty()) {
        (false, false) if message == stderr => message.to_string(),
        (false, false) => format!("{message}\n{stderr}"),
        (false, true) => message.to_string(),
        (true, false) => stderr.to_string(),
        (true, true) => {
            let status = status.trim();
            if status.is_empty() {
                "gh failed with no output".to_string()
            } else {
                status.to_string()
            }
        }
    }
}

/// Posts one review payload through `client` and classifies the raw result.
fn post_review_attempt(
    client: &dyn GhClient,
    repo: &str,
    pr: u64,
    payload: &Value,
) -> review::PostAttempt {
    let argv = vec![
        "api".to_string(),
        "-X".to_string(),
        "POST".to_string(),
        format!("repos/{repo}/pulls/{pr}/reviews"),
        "--input".to_string(),
        "-".to_string(),
    ];
    let stdin = payload.to_string();
    let out = match client.run(&argv, Some(&stdin)) {
        Ok(out) => out,
        Err(e) => return review::PostAttempt::Error(e.to_string()),
    };
    if out.success {
        review::classify_post_result(true, &out.stdout)
    } else {
        review::classify_post_result(false, &format!("{}{}", out.stdout, out.stderr))
    }
}

/// Runs one read-only command and returns its standard output, or trimmed
/// standard error on a non-zero exit.
fn run_read(client: &dyn GhClient, argv: &[String]) -> Result<String, String> {
    let out = client.run(argv, None).map_err(|e| e.to_string())?;
    if !out.success {
        return Err(out.stderr.trim().to_string());
    }
    Ok(out.stdout)
}

/// One login command's answer, or none when it could not run or failed.
fn read_login(client: &dyn GhClient, argv: &[String]) -> Option<String> {
    run_read(client, argv)
        .ok()
        .and_then(|text| parse_login(&text))
}

/// Whether the check-status command succeeded, and its standard error when
/// it did not.
fn read_checks_status(client: &dyn GhClient, repo: &str, pr: &str) -> (bool, String) {
    let argv = vec![
        "pr".to_string(),
        "checks".to_string(),
        pr.to_string(),
        "--repo".to_string(),
        repo.to_string(),
        "--json".to_string(),
        "name,state,bucket".to_string(),
    ];
    match run_read(client, &argv) {
        Ok(_) => (true, String::new()),
        Err(text) => (false, text),
    }
}

/// Reads a login name: trimmed, and absent when empty.
#[must_use]
pub fn parse_login(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// What one identity may do about verdicts, given the pull request's author
/// and the current viewer.
#[must_use]
pub fn verdict_capability(author: Option<&str>, viewer: Option<&str>) -> VerdictCapability {
    match (author, viewer) {
        (Some(a), Some(v)) if a == v => VerdictCapability::Known(Vec::new()),
        (Some(_), Some(_)) => {
            VerdictCapability::Known(vec![Verdict::Approve, Verdict::RequestChanges])
        }
        (None, None) => {
            VerdictCapability::Unknown("cannot read the author or viewer login".to_string())
        }
        (None, Some(_)) => VerdictCapability::Unknown("cannot read the author login".to_string()),
        (Some(_), None) => VerdictCapability::Unknown("cannot read the viewer login".to_string()),
    }
}

/// Whether the check status is readable, unreadable, or unknown, from the
/// command's success and its standard error.
#[must_use]
pub fn check_status_capability(success: bool, stderr: &str) -> ReadCapability {
    let lower = stderr.to_lowercase();
    if success || lower.contains("no checks reported") {
        return ReadCapability::Readable;
    }
    if lower.contains("403") || lower.contains("not accessible") {
        return ReadCapability::Unreadable(stderr.trim().to_string());
    }
    ReadCapability::Unknown(stderr.trim().to_string())
}

fn to_review_verdict(verdict: Verdict) -> review::Verdict {
    match verdict {
        Verdict::Approve => review::Verdict::Approve,
        Verdict::RequestChanges => review::Verdict::RequestChanges,
    }
}

fn from_review_verdict(verdict: review::Verdict) -> Verdict {
    match verdict {
        review::Verdict::Approve => Verdict::Approve,
        review::Verdict::RequestChanges => Verdict::RequestChanges,
    }
}

fn to_review_comment(comment: &ReviewComment) -> review::Comment {
    review::Comment {
        path: comment.path.clone(),
        line: comment.line,
        body: comment.body.clone(),
    }
}

/// The warning printed when a self-review refusal forces an advisory
/// comment, byte for byte the text the command line printed before.
fn self_review_warning(event: &str) -> String {
    format!(
        "osf review post: GitHub refuses {event} from the pull request's own author; posting \
         as COMMENT with the verdict marked advisory. reviewDecision stays empty until \
         the reviewer has its own identity."
    )
}

/// Posts `plan` once, then again as an advisory comment after a self-review
/// refusal. Pure-ish: the two side effects are injected.
fn post_with_fallback(
    poster: &dyn Fn(&Value) -> review::PostAttempt,
    plan: &review::Plan,
    head_sha: &str,
    warn: &mut dyn FnMut(&str),
) -> review::Outcome {
    let payload = review::payload(
        head_sha,
        &plan.body,
        plan.verdict.as_event(),
        &plan.comments,
    );
    match review::after_first_attempt(poster(&payload), plan, head_sha) {
        review::NextStep::Done(outcome) => outcome,
        review::NextStep::RetryAsComment(advisory) => {
            warn(&self_review_warning(plan.verdict.as_event()));
            review::after_fallback_attempt(poster(&advisory), plan)
        }
    }
}

/// Calls the real `gh` command line tool.
pub struct RealGh;

#[allow(clippy::unused_self)]
impl GhClient for RealGh {
    fn run(&self, args: &[String], stdin: Option<&str>) -> Result<GhOutput, StatusError> {
        let mut command = Command::new("gh");
        command
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if stdin.is_some() {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        let mut child = command
            .spawn()
            .map_err(|e| StatusError::new(format!("cannot run gh: {e}")))?;
        if let Some(text) = stdin {
            if let Some(mut pipe) = child.stdin.take() {
                pipe.write_all(text.as_bytes())
                    .map_err(|e| StatusError::new(format!("cannot write to gh: {e}")))?;
            }
        }
        let output = child
            .wait_with_output()
            .map_err(|e| StatusError::new(format!("cannot run gh: {e}")))?;
        Ok(GhOutput {
            success: output.status.success(),
            status: output.status.to_string(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn view_body(&self, repo: &str, pr: &str) -> Result<String, StatusError> {
        let output = Command::new("gh")
            .args([
                "pr", "view", pr, "--repo", repo, "--json", "body", "-q", ".body",
            ])
            .output()
            .map_err(|e| StatusError::new(format!("cannot run gh: {e}")))?;
        if !output.status.success() {
            return Err(StatusError::new(format!(
                "gh pr view failed for {repo}#{pr}; nothing was changed"
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn view_review(&self, repo: &str, pr: &str) -> Result<String, StatusError> {
        let output = Command::new("gh")
            .args([
                "pr",
                "view",
                pr,
                "--repo",
                repo,
                "--json",
                "reviewDecision,reviews,comments",
            ])
            .output()
            .map_err(|e| StatusError::new(format!("cannot run gh: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(StatusError::new(format!(
                "gh pr view failed for {repo}#{pr}: {}",
                stderr.trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn view_pr_info(&self, repo: &str, pr: &str) -> Result<String, StatusError> {
        let output = Command::new("gh")
            .args([
                "pr",
                "view",
                pr,
                "--repo",
                repo,
                "--json",
                "body,baseRefName,headRefName",
            ])
            .output()
            .map_err(|e| StatusError::new(format!("cannot run gh: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(StatusError::new(format!(
                "gh pr view failed for {repo}#{pr}: {}",
                stderr.trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn view_checks(&self, repo: &str, pr: &str) -> Result<String, StatusError> {
        let output = Command::new("gh")
            .args([
                "pr",
                "checks",
                pr,
                "--repo",
                repo,
                "--json",
                "name,state,bucket",
            ])
            .output()
            .map_err(|e| StatusError::new(format!("cannot run gh: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.to_lowercase().contains("no checks reported") {
                return Ok("[]".to_string());
            }
            return Err(StatusError::new(format!(
                "gh pr checks failed for {repo}#{pr}: {}",
                stderr.trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn edit_body(&self, repo: &str, pr: &str, body: &str) -> Result<(), StatusError> {
        let base = std::env::temp_dir(); // osf: temp-dir allowed, gh needs a real file path
        let tmp = base.join(format!("osf-status-{}.md", std::process::id()));
        std::fs::write(&tmp, body)
            .map_err(|e| StatusError::new(format!("cannot write a temporary file: {e}")))?;
        let run = Command::new("gh")
            .arg("pr")
            .arg("edit")
            .arg(pr)
            .arg("--repo")
            .arg(repo)
            .arg("--body-file")
            .arg(&tmp)
            .output();
        let _ = std::fs::remove_file(&tmp);
        let output = run.map_err(|e| StatusError::new(format!("cannot run gh: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(StatusError::new(format!(
                "gh pr edit failed for {repo}#{pr}: {}",
                stderr.trim()
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pr_status::{GhOutput, StatusError};
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    struct FakeGh {
        calls: RefCell<Vec<String>>,
        body: String,
        body_error: Option<String>,
        checks: String,
        checks_error: Option<String>,
        edited: RefCell<Option<String>>,
        runs: RefCell<VecDeque<Result<GhOutput, StatusError>>>,
        run_calls: RefCell<Vec<(Vec<String>, Option<String>)>>,
    }

    fn fake(body: &str, checks: &str) -> FakeGh {
        FakeGh {
            calls: RefCell::new(Vec::new()),
            body: body.to_string(),
            body_error: None,
            checks: checks.to_string(),
            checks_error: None,
            edited: RefCell::new(None),
            runs: RefCell::new(VecDeque::new()),
            run_calls: RefCell::new(Vec::new()),
        }
    }

    /// A fake whose `run` answers with `answers`, in order.
    fn fake_with_runs(answers: Vec<Result<GhOutput, StatusError>>) -> FakeGh {
        let mut client = fake("", "[]");
        client.runs = RefCell::new(VecDeque::from(answers));
        client
    }

    /// A successful [`GhOutput`] carrying `stdout`.
    fn gh_ok(stdout: &str) -> GhOutput {
        GhOutput {
            success: true,
            status: "exit status: 0".to_string(),
            stdout: stdout.to_string(),
            stderr: String::new(),
        }
    }

    /// A failed [`GhOutput`] carrying `status`, `stdout` and `stderr`.
    fn gh_fail(status: &str, stdout: &str, stderr: &str) -> GhOutput {
        GhOutput {
            success: false,
            status: status.to_string(),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }
    }

    /// `parts` as the argv Vec the client records.
    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(ToString::to_string).collect()
    }

    #[allow(clippy::unused_self)]
    impl GhClient for FakeGh {
        fn run(&self, args: &[String], stdin: Option<&str>) -> Result<GhOutput, StatusError> {
            self.run_calls
                .borrow_mut()
                .push((args.to_vec(), stdin.map(str::to_string)));
            self.runs
                .borrow_mut()
                .pop_front()
                .expect("FakeGh ran out of scripted answers")
        }

        fn view_body(&self, repo: &str, pr: &str) -> Result<String, StatusError> {
            self.calls
                .borrow_mut()
                .push(format!("view_body {repo}#{pr}"));
            match &self.body_error {
                Some(e) => Err(StatusError::new(e.clone())),
                None => Ok(self.body.clone()),
            }
        }

        fn view_review(&self, _repo: &str, _pr: &str) -> Result<String, StatusError> {
            unreachable!("write_status_block never reads the review")
        }

        fn view_pr_info(&self, _repo: &str, _pr: &str) -> Result<String, StatusError> {
            unreachable!("write_status_block never reads the pull request info")
        }

        fn view_checks(&self, repo: &str, pr: &str) -> Result<String, StatusError> {
            self.calls
                .borrow_mut()
                .push(format!("view_checks {repo}#{pr}"));
            match &self.checks_error {
                Some(e) => Err(StatusError::new(e.clone())),
                None => Ok(self.checks.clone()),
            }
        }

        fn edit_body(&self, repo: &str, pr: &str, body: &str) -> Result<(), StatusError> {
            self.calls
                .borrow_mut()
                .push(format!("edit_body {repo}#{pr}"));
            *self.edited.borrow_mut() = Some(body.to_string());
            Ok(())
        }
    }

    fn pr() -> PullRequestId {
        PullRequestId {
            repo: "open-software-factory/demo".to_string(),
            pr: "7".to_string(),
        }
    }

    fn status_block() -> String {
        format!(
            "{}\nhello\n{}\n",
            crate::marker::start("status", "abc123"),
            crate::marker::end("status")
        )
    }

    fn sample_plan() -> review::Plan {
        review::Plan {
            verdict: review::Verdict::RequestChanges,
            body: "Summary.\n".to_string(),
            comments: vec![review::Comment {
                path: "README.md".to_string(),
                line: 1,
                body: "fix this".to_string(),
            }],
        }
    }

    #[test]
    fn write_status_block_writes_the_apply_result_after_reading() {
        let github = GitHub::new(fake("old body\n", "[]"));
        let block = status_block();
        github.write_status_block(&pr(), &block).expect("writes");
        let expected = pr_status::apply("old body\n", &block).expect("applies");
        assert_eq!(
            github.client.edited.borrow().as_deref(),
            Some(expected.as_str())
        );
        assert_eq!(
            *github.client.calls.borrow(),
            vec![
                "view_body open-software-factory/demo#7".to_string(),
                "edit_body open-software-factory/demo#7".to_string()
            ]
        );
    }

    #[test]
    fn write_status_block_never_edits_when_the_read_fails() {
        let mut client = fake("old body\n", "[]");
        client.body_error = Some("gh pr view failed for open-software-factory/demo#7".to_string());
        let github = GitHub::new(client);
        let err = github
            .write_status_block(&pr(), &status_block())
            .expect_err("read fails");
        assert_eq!(
            err.to_string(),
            "gh pr view failed for open-software-factory/demo#7"
        );
        assert!(github.client.edited.borrow().is_none());
        assert_eq!(
            *github.client.calls.borrow(),
            vec!["view_body open-software-factory/demo#7".to_string()]
        );
    }

    #[test]
    fn read_check_status_reports_unknown_on_a_failed_read() {
        let mut client = fake("", "[]");
        client.checks_error =
            Some("gh pr checks failed for open-software-factory/demo#7: boom".to_string());
        let github = GitHub::new(client);
        assert_eq!(
            github.read_check_status(&pr()),
            ReadOutcome::Unknown(
                "gh pr checks failed for open-software-factory/demo#7: boom".to_string()
            )
        );
    }

    #[test]
    fn read_check_status_reads_an_empty_array_as_empty() {
        let github = GitHub::new(fake("", "[]"));
        assert_eq!(github.read_check_status(&pr()), ReadOutcome::Empty);
    }

    #[test]
    fn read_check_status_keeps_check_order() {
        let text = r#"[
            {"name":"first","state":"SUCCESS","bucket":"pass"},
            {"name":"second","state":"FAILURE","bucket":"fail"}
        ]"#;
        let github = GitHub::new(fake("", text));
        assert_eq!(
            github.read_check_status(&pr()),
            ReadOutcome::Found(CheckStatus {
                checks: vec![
                    Check {
                        name: "first".to_string(),
                        state: "SUCCESS".to_string(),
                        bucket: "pass".to_string(),
                    },
                    Check {
                        name: "second".to_string(),
                        state: "FAILURE".to_string(),
                        bucket: "fail".to_string(),
                    },
                ],
            })
        );
    }

    #[test]
    fn read_check_status_reports_unknown_on_unparseable_text() {
        let github = GitHub::new(fake("", r#"{"not":"an array"}"#));
        assert_eq!(
            github.read_check_status(&pr()),
            ReadOutcome::Unknown(
                "the checks data must have the shape of `gh pr checks --json name,state,bucket`"
                    .to_string()
            )
        );
    }

    #[test]
    fn post_with_fallback_reports_a_successful_first_attempt() {
        let plan = sample_plan();
        let sent: RefCell<Vec<Value>> = RefCell::new(Vec::new());
        let poster = |payload: &Value| {
            sent.borrow_mut().push(payload.clone());
            review::PostAttempt::Success {
                id: "1".to_string(),
                state: "APPROVED".to_string(),
                url: "u".to_string(),
            }
        };
        let mut messages: Vec<String> = Vec::new();
        let mut warn = |text: &str| messages.push(text.to_string());
        let outcome = post_with_fallback(&poster, &plan, "sha", &mut warn);
        assert_eq!(
            outcome,
            review::Outcome::Reviewed {
                verdict: review::Verdict::RequestChanges,
                n_inline: 1,
                id: "1".to_string(),
                state: "APPROVED".to_string(),
                url: "u".to_string(),
            }
        );
        assert_eq!(sent.borrow().len(), 1);
        assert!(messages.is_empty());
    }

    #[test]
    fn post_with_fallback_warns_and_retries_as_an_advisory_comment() {
        let plan = sample_plan();
        let sent: RefCell<Vec<Value>> = RefCell::new(Vec::new());
        let attempts = Cell::new(0u32);
        let poster = |payload: &Value| {
            sent.borrow_mut().push(payload.clone());
            let n = attempts.get();
            attempts.set(n + 1);
            if n == 0 {
                review::PostAttempt::RefusedSelfReview
            } else {
                review::PostAttempt::Success {
                    id: "2".to_string(),
                    state: "COMMENTED".to_string(),
                    url: "u2".to_string(),
                }
            }
        };
        let mut messages: Vec<String> = Vec::new();
        let mut warn = |text: &str| messages.push(text.to_string());
        let outcome = post_with_fallback(&poster, &plan, "sha123", &mut warn);
        assert_eq!(
            outcome,
            review::Outcome::FallbackComment {
                verdict: review::Verdict::RequestChanges,
                n_inline: 1,
                id: "2".to_string(),
                state: "COMMENTED".to_string(),
                url: "u2".to_string(),
            }
        );
        assert_eq!(
            messages,
            vec![
                "osf review post: GitHub refuses REQUEST_CHANGES from the pull request's own \
                 author; posting as COMMENT with the verdict marked advisory. reviewDecision \
                 stays empty until the reviewer has its own identity."
                    .to_string()
            ]
        );
        let expected =
            review::build_advisory_payload(&plan, plan.verdict, "sha123").expect("advisory");
        assert_eq!(sent.borrow().get(1), Some(&expected));
    }

    #[test]
    fn post_with_fallback_reports_a_second_refusal() {
        let plan = sample_plan();
        let poster = |_payload: &Value| review::PostAttempt::RefusedSelfReview;
        let mut messages: Vec<String> = Vec::new();
        let mut warn = |text: &str| messages.push(text.to_string());
        let outcome = post_with_fallback(&poster, &plan, "sha", &mut warn);
        assert_eq!(
            outcome,
            review::Outcome::PostFailed("gh refused the fallback comment as well".to_string())
        );
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn post_with_fallback_does_not_retry_a_plain_error() {
        let plan = sample_plan();
        let calls = Cell::new(0u32);
        let poster = |_payload: &Value| {
            calls.set(calls.get() + 1);
            review::PostAttempt::Error("gh: rate limited".to_string())
        };
        let mut messages: Vec<String> = Vec::new();
        let mut warn = |text: &str| messages.push(text.to_string());
        let outcome = post_with_fallback(&poster, &plan, "sha", &mut warn);
        assert_eq!(
            outcome,
            review::Outcome::PostFailed("gh: rate limited".to_string())
        );
        assert_eq!(calls.get(), 1);
        assert!(messages.is_empty());
    }

    #[test]
    fn create_branch_argv_posts_to_the_refs_endpoint() {
        assert_eq!(
            create_branch_argv("open-software-factory/demo"),
            vec![
                "api",
                "--method",
                "POST",
                "repos/open-software-factory/demo/git/refs",
                "--input",
                "-"
            ]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn create_branch_payload_carries_the_ref_and_sha() {
        let branch = NewBranch {
            repo: "open-software-factory/demo".to_string(),
            name: "topic".to_string(),
            from_sha: "abc".to_string(),
        };
        assert_eq!(
            create_branch_payload(&branch),
            serde_json::json!({"ref": "refs/heads/topic", "sha": "abc"})
        );
    }

    #[test]
    fn parse_branch_response_reads_the_name_and_sha() {
        let branch = NewBranch {
            repo: "open-software-factory/demo".to_string(),
            name: "topic".to_string(),
            from_sha: "abc".to_string(),
        };
        let text = r#"{"ref":"refs/heads/topic","object":{"sha":"def"}}"#;
        assert_eq!(
            parse_branch_response(&branch, text),
            Ok(Branch {
                name: "topic".to_string(),
                sha: "def".to_string(),
            })
        );
    }

    #[test]
    fn parse_branch_response_refuses_a_missing_ref() {
        let branch = NewBranch {
            repo: "open-software-factory/demo".to_string(),
            name: "topic".to_string(),
            from_sha: "abc".to_string(),
        };
        let err =
            parse_branch_response(&branch, r#"{"object":{"sha":"def"}}"#).expect_err("no ref");
        assert_eq!(
            err.to_string(),
            "the branch response for 'topic' has no `ref`"
        );
    }

    #[test]
    fn parse_branch_response_refuses_a_missing_sha() {
        let branch = NewBranch {
            repo: "open-software-factory/demo".to_string(),
            name: "topic".to_string(),
            from_sha: "abc".to_string(),
        };
        let err = parse_branch_response(&branch, r#"{"ref":"refs/heads/topic","object":{}}"#)
            .expect_err("no sha");
        assert_eq!(
            err.to_string(),
            "the branch response for 'topic' has no `object.sha`"
        );
    }

    #[test]
    fn open_pull_request_argv_posts_to_the_pulls_endpoint() {
        assert_eq!(
            open_pull_request_argv("open-software-factory/demo"),
            vec![
                "api",
                "--method",
                "POST",
                "repos/open-software-factory/demo/pulls",
                "--input",
                "-"
            ]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn open_pull_request_payload_carries_the_fields() {
        let request = NewPullRequest {
            repo: "open-software-factory/demo".to_string(),
            head: "topic".to_string(),
            base: "main".to_string(),
            title: "Title".to_string(),
            body: "Body".to_string(),
        };
        assert_eq!(
            open_pull_request_payload(&request),
            serde_json::json!({
                "title": "Title",
                "head": "topic",
                "base": "main",
                "body": "Body",
            })
        );
    }

    #[test]
    fn parse_opened_pull_request_reads_the_number() {
        assert_eq!(
            parse_opened_pull_request("open-software-factory/demo", r#"{"number":42}"#),
            Ok(PullRequestId {
                repo: "open-software-factory/demo".to_string(),
                pr: "42".to_string(),
            })
        );
    }

    #[test]
    fn parse_opened_pull_request_refuses_a_missing_number() {
        let err = parse_opened_pull_request("open-software-factory/demo", r#"{"title":"x"}"#)
            .expect_err("no number");
        assert_eq!(err.to_string(), "the pull request response has no number");
    }

    #[test]
    fn parse_login_trims_and_rejects_empty() {
        assert_eq!(parse_login("  octocat \n"), Some("octocat".to_string()));
        assert_eq!(parse_login("   \n"), None);
    }

    #[test]
    fn verdict_capability_covers_every_branch() {
        assert_eq!(
            verdict_capability(Some("me"), Some("me")),
            VerdictCapability::Known(Vec::new())
        );
        assert_eq!(
            verdict_capability(Some("me"), Some("you")),
            VerdictCapability::Known(vec![Verdict::Approve, Verdict::RequestChanges])
        );
        assert_eq!(
            verdict_capability(None, Some("me")),
            VerdictCapability::Unknown("cannot read the author login".to_string())
        );
        assert_eq!(
            verdict_capability(Some("me"), None),
            VerdictCapability::Unknown("cannot read the viewer login".to_string())
        );
        assert_eq!(
            verdict_capability(None, None),
            VerdictCapability::Unknown("cannot read the author or viewer login".to_string())
        );
    }

    #[test]
    fn check_status_capability_covers_every_branch() {
        assert_eq!(check_status_capability(true, ""), ReadCapability::Readable);
        assert_eq!(
            check_status_capability(false, "No checks reported on the branch"),
            ReadCapability::Readable
        );
        assert_eq!(
            check_status_capability(false, "HTTP 403: Forbidden\n"),
            ReadCapability::Unreadable("HTTP 403: Forbidden".to_string())
        );
        assert_eq!(
            check_status_capability(false, "repository not accessible"),
            ReadCapability::Unreadable("repository not accessible".to_string())
        );
        assert_eq!(
            check_status_capability(false, "some other failure"),
            ReadCapability::Unknown("some other failure".to_string())
        );
    }

    #[test]
    fn verdict_conversions_round_trip() {
        assert_eq!(
            to_review_verdict(Verdict::Approve),
            review::Verdict::Approve
        );
        assert_eq!(
            to_review_verdict(Verdict::RequestChanges),
            review::Verdict::RequestChanges
        );
        assert_eq!(
            from_review_verdict(review::Verdict::Approve),
            Verdict::Approve
        );
        assert_eq!(
            from_review_verdict(review::Verdict::RequestChanges),
            Verdict::RequestChanges
        );
    }

    #[test]
    fn gh_failure_text_uses_the_stdout_message() {
        assert_eq!(
            gh_failure_text("exit status: 1", r#"{"message":"Validation Failed"}"#, ""),
            "Validation Failed"
        );
    }

    #[test]
    fn gh_failure_text_uses_the_stderr_when_stdout_has_no_message() {
        assert_eq!(
            gh_failure_text("exit status: 1", "", "gh: Validation Failed (HTTP 422)\n"),
            "gh: Validation Failed (HTTP 422)"
        );
    }

    #[test]
    fn gh_failure_text_joins_the_message_and_stderr() {
        assert_eq!(
            gh_failure_text(
                "exit status: 1",
                r#"{"message":"Validation Failed"}"#,
                "gh: Validation Failed (HTTP 422)"
            ),
            "Validation Failed\ngh: Validation Failed (HTTP 422)"
        );
    }

    #[test]
    fn gh_failure_text_falls_back_to_the_status() {
        assert_eq!(gh_failure_text("exit status: 1", "", ""), "exit status: 1");
    }

    #[test]
    fn gh_failure_text_falls_back_when_status_is_empty() {
        assert_eq!(gh_failure_text("", "   ", "\n"), "gh failed with no output");
    }

    #[test]
    fn gh_failure_text_ignores_stdout_that_is_not_json() {
        assert_eq!(
            gh_failure_text("exit status: 1", "not json", "boom"),
            "boom"
        );
    }

    #[test]
    fn gh_failure_text_does_not_repeat_equal_parts() {
        assert_eq!(
            gh_failure_text("exit status: 1", r#"{"message":"boom"}"#, "boom"),
            "boom"
        );
    }

    fn new_branch() -> NewBranch {
        NewBranch {
            repo: "open-software-factory/demo".to_string(),
            name: "topic".to_string(),
            from_sha: "abc".to_string(),
        }
    }

    fn new_pull_request() -> NewPullRequest {
        NewPullRequest {
            repo: "open-software-factory/demo".to_string(),
            head: "topic".to_string(),
            base: "main".to_string(),
            title: "Title".to_string(),
            body: "Body".to_string(),
        }
    }

    #[test]
    fn create_branch_runs_the_api_command_through_the_client() {
        let branch = new_branch();
        let github = GitHub::new(fake_with_runs(vec![Ok(gh_ok(
            r#"{"ref":"refs/heads/topic","object":{"sha":"def"}}"#,
        ))]));
        let created = github.create_branch(&branch).expect("creates the branch");
        assert_eq!(
            created,
            Branch {
                name: "topic".to_string(),
                sha: "def".to_string(),
            }
        );
        let calls = github.client.run_calls.borrow();
        assert_eq!(calls.len(), 1, "one run call: {calls:?}");
        let (args, stdin) = calls.first().expect("one run call");
        assert_eq!(args, &create_branch_argv(&branch.repo));
        let expected = create_branch_payload(&branch).to_string();
        assert_eq!(stdin.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn create_branch_reports_the_validation_message() {
        let github = GitHub::new(fake_with_runs(vec![Ok(gh_fail(
            "exit status: 1",
            r#"{"message":"Validation Failed"}"#,
            "",
        ))]));
        let err = github.create_branch(&new_branch()).expect_err("refused");
        assert_eq!(err, ForgeError::Failed("Validation Failed".to_string()));
    }

    #[test]
    fn create_branch_falls_back_to_the_exit_status() {
        let github = GitHub::new(fake_with_runs(vec![Ok(gh_fail("exit status: 1", "", ""))]));
        let err = github.create_branch(&new_branch()).expect_err("refused");
        assert_eq!(err, ForgeError::Failed("exit status: 1".to_string()));
    }

    #[test]
    fn create_branch_reports_a_command_that_could_not_start() {
        let github = GitHub::new(fake_with_runs(vec![Err(StatusError::new(
            "cannot run gh: no such file",
        ))]));
        let err = github.create_branch(&new_branch()).expect_err("cannot run");
        assert_eq!(
            err,
            ForgeError::Failed("cannot run gh: no such file".to_string())
        );
    }

    #[test]
    fn open_pull_request_runs_the_api_command_through_the_client() {
        let request = new_pull_request();
        let github = GitHub::new(fake_with_runs(vec![Ok(gh_ok(r#"{"number":42}"#))]));
        let opened = github.open_pull_request(&request).expect("opens");
        assert_eq!(
            opened,
            PullRequestId {
                repo: "open-software-factory/demo".to_string(),
                pr: "42".to_string(),
            }
        );
        let calls = github.client.run_calls.borrow();
        assert_eq!(calls.len(), 1, "one run call: {calls:?}");
        let (args, stdin) = calls.first().expect("one run call");
        assert_eq!(args, &open_pull_request_argv(&request.repo));
        let expected = open_pull_request_payload(&request).to_string();
        assert_eq!(stdin.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn open_pull_request_reports_the_validation_message() {
        let github = GitHub::new(fake_with_runs(vec![Ok(gh_fail(
            "exit status: 1",
            r#"{"message":"Validation Failed"}"#,
            "",
        ))]));
        let err = github
            .open_pull_request(&new_pull_request())
            .expect_err("refused");
        assert_eq!(err, ForgeError::Failed("Validation Failed".to_string()));
    }

    #[test]
    fn open_pull_request_falls_back_to_the_exit_status() {
        let github = GitHub::new(fake_with_runs(vec![Ok(gh_fail("exit status: 1", "", ""))]));
        let err = github
            .open_pull_request(&new_pull_request())
            .expect_err("refused");
        assert_eq!(err, ForgeError::Failed("exit status: 1".to_string()));
    }

    #[test]
    fn open_pull_request_reports_a_command_that_could_not_start() {
        let github = GitHub::new(fake_with_runs(vec![Err(StatusError::new(
            "cannot run gh: no such file",
        ))]));
        let err = github
            .open_pull_request(&new_pull_request())
            .expect_err("cannot run");
        assert_eq!(
            err,
            ForgeError::Failed("cannot run gh: no such file".to_string())
        );
    }

    #[test]
    fn capabilities_reads_every_source_through_the_client_in_order() {
        let github = GitHub::new(fake_with_runs(vec![
            Ok(gh_ok("reviewer\n")),
            Ok(gh_ok("builder\n")),
            Ok(gh_ok("[]")),
        ]));
        let capabilities = github.capabilities(&pr());
        assert_eq!(
            capabilities,
            Capabilities {
                verdicts: VerdictCapability::Known(vec![Verdict::Approve, Verdict::RequestChanges]),
                check_status: ReadCapability::Readable,
            }
        );
        assert_eq!(
            *github.client.run_calls.borrow(),
            vec![
                (argv(&["api", "user", "--jq", ".login"]), None),
                (
                    argv(&[
                        "pr",
                        "view",
                        "7",
                        "--repo",
                        "open-software-factory/demo",
                        "--json",
                        "author",
                        "--jq",
                        ".author.login"
                    ]),
                    None
                ),
                (
                    argv(&[
                        "pr",
                        "checks",
                        "7",
                        "--repo",
                        "open-software-factory/demo",
                        "--json",
                        "name,state,bucket"
                    ]),
                    None
                ),
            ]
        );
    }

    #[test]
    fn capabilities_reports_an_unreadable_viewer_login() {
        let github = GitHub::new(fake_with_runs(vec![
            Ok(gh_fail("exit status: 1", "", "boom")),
            Ok(gh_ok("builder\n")),
            Ok(gh_ok("[]")),
        ]));
        let capabilities = github.capabilities(&pr());
        assert_eq!(
            capabilities.verdicts,
            VerdictCapability::Unknown("cannot read the viewer login".to_string())
        );
        assert_eq!(capabilities.check_status, ReadCapability::Readable);
    }

    #[test]
    fn capabilities_reports_an_unreadable_check_status_on_403() {
        let github = GitHub::new(fake_with_runs(vec![
            Ok(gh_ok("reviewer\n")),
            Ok(gh_ok("builder\n")),
            Ok(gh_fail("exit status: 1", "", "HTTP 403: Forbidden")),
        ]));
        let capabilities = github.capabilities(&pr());
        assert_eq!(
            capabilities.check_status,
            ReadCapability::Unreadable("HTTP 403: Forbidden".to_string())
        );
    }

    #[test]
    fn capabilities_reports_an_unknown_check_status_on_another_error() {
        let github = GitHub::new(fake_with_runs(vec![
            Ok(gh_ok("reviewer\n")),
            Ok(gh_ok("builder\n")),
            Ok(gh_fail("exit status: 1", "", "HTTP 502: Bad Gateway")),
        ]));
        let capabilities = github.capabilities(&pr());
        assert_eq!(
            capabilities.check_status,
            ReadCapability::Unknown("HTTP 502: Bad Gateway".to_string())
        );
    }

    #[test]
    fn capabilities_reports_an_unknown_check_status_when_the_command_cannot_start() {
        let github = GitHub::new(fake_with_runs(vec![
            Ok(gh_ok("reviewer\n")),
            Ok(gh_ok("builder\n")),
            Err(StatusError::new("cannot run gh: no such file")),
        ]));
        let capabilities = github.capabilities(&pr());
        assert_eq!(
            capabilities.check_status,
            ReadCapability::Unknown("cannot run gh: no such file".to_string())
        );
    }

    /// A review request with one inline comment, for the post path.
    fn new_review() -> NewReview {
        NewReview {
            pull_request: pr(),
            head_sha: "abc123".to_string(),
            verdict: Verdict::RequestChanges,
            body: "Summary.\n".to_string(),
            comments: vec![ReviewComment {
                path: "README.md".to_string(),
                line: 1,
                body: "fix this".to_string(),
            }],
        }
    }

    /// The argv every review POST sends.
    fn review_argv() -> Vec<String> {
        argv(&[
            "api",
            "-X",
            "POST",
            "repos/open-software-factory/demo/pulls/7/reviews",
            "--input",
            "-",
        ])
    }

    #[test]
    fn post_review_attempt_posts_the_payload_and_reads_the_result() {
        let payload = serde_json::json!({"event": "APPROVE"});
        let github = GitHub::new(fake_with_runs(vec![Ok(gh_ok(
            r#"{"id":42,"state":"APPROVED","html_url":"https://example.invalid/42"}"#,
        ))]));
        assert_eq!(
            post_review_attempt(&github.client, "open-software-factory/demo", 7, &payload),
            review::PostAttempt::Success {
                id: "42".to_string(),
                state: "APPROVED".to_string(),
                url: "https://example.invalid/42".to_string(),
            }
        );
        let calls = github.client.run_calls.borrow();
        assert_eq!(calls.len(), 1, "one run call: {calls:?}");
        let (args, stdin) = calls.first().expect("one run call");
        assert_eq!(args, &review_argv());
        assert_eq!(stdin.as_deref(), Some(payload.to_string().as_str()));
    }

    #[test]
    fn post_review_attempt_recognizes_the_self_review_refusal() {
        let github = GitHub::new(fake_with_runs(vec![Ok(gh_fail(
            "exit status: 1",
            "gh: Review cannot be requested",
            " on your own pull request (HTTP 422)",
        ))]));
        assert_eq!(
            post_review_attempt(
                &github.client,
                "open-software-factory/demo",
                7,
                &serde_json::json!({})
            ),
            review::PostAttempt::RefusedSelfReview
        );
    }

    #[test]
    fn post_review_attempt_reports_another_failure() {
        let github = GitHub::new(fake_with_runs(vec![Ok(gh_fail(
            "exit status: 1",
            "gh: rate limited",
            " (HTTP 403)",
        ))]));
        assert_eq!(
            post_review_attempt(
                &github.client,
                "open-software-factory/demo",
                7,
                &serde_json::json!({})
            ),
            review::PostAttempt::Error("gh: rate limited (HTTP 403)".to_string())
        );
    }

    #[test]
    fn post_review_attempt_reports_a_command_that_could_not_start() {
        let github = GitHub::new(fake_with_runs(vec![Err(StatusError::new(
            "cannot run gh: no such file",
        ))]));
        assert_eq!(
            post_review_attempt(
                &github.client,
                "open-software-factory/demo",
                7,
                &serde_json::json!({})
            ),
            review::PostAttempt::Error("cannot run gh: no such file".to_string())
        );
    }

    #[test]
    fn post_review_posts_a_native_review_through_the_client() {
        let github = GitHub::new(fake_with_runs(vec![Ok(gh_ok(
            r#"{"id":42,"state":"CHANGES_REQUESTED","html_url":"https://example.invalid/r/42"}"#,
        ))]));
        let posted = github.post_review(&new_review()).expect("posts");
        assert_eq!(
            posted,
            PostedReview {
                verdict: Verdict::RequestChanges,
                advisory: false,
                inline: 1,
                id: "42".to_string(),
                state: "CHANGES_REQUESTED".to_string(),
                url: "https://example.invalid/r/42".to_string(),
            }
        );
        let calls = github.client.run_calls.borrow();
        assert_eq!(calls.len(), 1, "one run call: {calls:?}");
        let (args, _stdin) = calls.first().expect("one run call");
        assert_eq!(args, &review_argv());
    }

    #[test]
    fn post_review_falls_back_to_an_advisory_comment_through_the_client() {
        let github = GitHub::new(fake_with_runs(vec![
            Ok(gh_fail(
                "exit status: 1",
                "",
                "gh: Review cannot be requested on your own pull request (HTTP 422)",
            )),
            Ok(gh_ok(
                r#"{"id":43,"state":"COMMENTED","html_url":"https://example.invalid/r/43"}"#,
            )),
        ]));
        let posted = github.post_review(&new_review()).expect("posts");
        assert_eq!(
            posted,
            PostedReview {
                verdict: Verdict::RequestChanges,
                advisory: true,
                inline: 1,
                id: "43".to_string(),
                state: "COMMENTED".to_string(),
                url: "https://example.invalid/r/43".to_string(),
            }
        );
        let calls = github.client.run_calls.borrow();
        assert_eq!(calls.len(), 2, "two run calls: {calls:?}");
        for (args, _stdin) in calls.iter() {
            assert_eq!(args, &review_argv());
        }
    }

    #[test]
    fn head_commit_reads_the_sha_through_the_client() {
        let github = GitHub::new(fake_with_runs(vec![Ok(gh_ok(
            r#"{"headRefOid":"abc123"}"#,
        ))]));
        assert_eq!(
            github.head_commit("open-software-factory/demo", 7),
            Ok("abc123".to_string())
        );
        let calls = github.client.run_calls.borrow();
        assert_eq!(calls.len(), 1, "one run call: {calls:?}");
        let (args, stdin) = calls.first().expect("one run call");
        assert_eq!(
            args,
            &argv(&[
                "pr",
                "view",
                "7",
                "--repo",
                "open-software-factory/demo",
                "--json",
                "headRefOid"
            ])
        );
        assert!(stdin.is_none());
    }

    #[test]
    fn head_commit_reports_a_failed_command_with_the_repo_number_and_stderr() {
        let github = GitHub::new(fake_with_runs(vec![Ok(gh_fail(
            "exit status: 1",
            "",
            "no such pull request\n",
        ))]));
        assert_eq!(
            github.head_commit("open-software-factory/demo", 7),
            Err(
                "could not read the head commit of open-software-factory/demo#7: no such pull \
                 request"
                    .to_string()
            )
        );
    }

    #[test]
    fn head_commit_reports_bad_json_with_the_repo_and_number() {
        let github = GitHub::new(fake_with_runs(vec![Ok(gh_ok("not json"))]));
        let err = github
            .head_commit("open-software-factory/demo", 7)
            .expect_err("bad json");
        assert!(err.ends_with(" (open-software-factory/demo#7)"), "{err}");
    }

    #[test]
    fn head_commit_reports_a_command_that_could_not_start() {
        let github = GitHub::new(fake_with_runs(vec![Err(StatusError::new(
            "cannot run gh: no such file",
        ))]));
        assert_eq!(
            github.head_commit("open-software-factory/demo", 7),
            Err("cannot run gh: no such file".to_string())
        );
    }
}
