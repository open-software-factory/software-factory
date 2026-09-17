//! `osf status`: the block at the top of a pull request description that
//! says whether the change is ready to merge. Rendering is a pure function
//! of its inputs, so the same inputs always give the same bytes; applying
//! replaces the block between two markers, or adds it at the top, without
//! touching anything else in the description.

use regex::Regex;
use serde_json::Value;
use std::fmt::{self, Write as _};
use std::process::Command;

const BEGIN: &str = "<!-- factory:status:begin -->";
const END: &str = "<!-- factory:status:end -->";

/// A `status` operation could not run: bad input, or a failed `gh` call.
/// Distinct from the operation running and finding nothing to do.
#[derive(Debug)]
pub struct StatusError(String);

impl StatusError {
    /// Builds an error carrying `message`. For a [`GhClient`] implementation
    /// reporting a failed `gh` call; every error this module raises itself
    /// is a specific, tested message already built with this same constructor.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        StatusError(message.into())
    }
}

impl fmt::Display for StatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StatusError {}

/// Everything [`render`] needs, as text: a file's worth of tier JSON, the
/// gate results, the two free-text fields, and the review JSON. Reading
/// files or calling `gh` is the caller's job, so this stays a pure function.
pub struct RenderInput<'a> {
    pub tier_json: &'a str,
    pub gates: &'a str,
    pub problem: &'a str,
    pub approach: &'a str,
    pub review_json: &'a str,
}

/// Renders the status block. Ready is `yes` only when every gate passed,
/// the verdict is APPROVE, and GitHub is not still requiring a human
/// review; otherwise it names the one blocking reason.
///
/// # Errors
/// Returns an error when `problem` or `approach` is empty, the tier JSON
/// does not have a string `tier` and a `reasons` array of strings, a gate
/// entry is not `name: passed` or `name: failed: <reason>`, or the review
/// JSON does not have the shape of `gh pr view --json
/// reviewDecision,reviews,comments`.
pub fn render(input: &RenderInput) -> Result<String, StatusError> {
    require_non_empty("problem", input.problem)?;
    require_non_empty("approach", input.approach)?;

    let (tier, reasons) = parse_tier(input.tier_json)?;
    let gates = parse_gates(input.gates)?;
    let review = parse_review(input.review_json)?;
    let (review_line, ready) = review_and_ready(&gates, &review);
    let verified = gates.verified_text();

    Ok(format!(
        "{BEGIN}\n\
         | | |\n\
         |---|---|\n\
         | **Ready** | {ready} |\n\
         | **Risk** | {tier}: {reasons} |\n\
         | **Verified** | {verified} |\n\
         | **Review** | {review_line} |\n\
         \n\
         **Problem**: {problem}\n\
         **Approach**: {approach}\n\
         {END}\n",
        problem = input.problem,
        approach = input.approach,
    ))
}

/// Puts `block` into `body`: between the markers when both are present,
/// replacing whatever was there; at the top when neither is present; an
/// error when only one is present, or the markers are in the wrong order.
/// Nothing outside the replaced or inserted span is changed. The line
/// ending already in use in `body` (`\n`, or `\r\n` if `body` has any) is
/// the one the result uses throughout, so the block never leaves a file
/// with mixed endings.
///
/// # Errors
/// Returns an error when `block` does not carry each marker exactly once
/// in the right order, or when `body` carries the markers in a shape apply
/// cannot resolve (one without the other, a repeat, or reversed).
pub fn apply(body: &str, block: &str) -> Result<String, StatusError> {
    validate_block(block)?;

    let style = newline_style(body);
    let sep = style.token();
    let (content, trail) = split_trailing_run(body, style);
    let (block_content, _) = split_trailing_run(block, newline_style(block));

    let body_lines = logical_lines(content);
    let block_lines = logical_lines(block_content);
    let begins = positions(&body_lines, BEGIN);
    let ends = positions(&body_lines, END);

    let new_lines = match (begins.as_slice(), ends.as_slice()) {
        ([], []) => {
            let mut lines = block_lines;
            lines.push(String::new());
            lines.extend(body_lines);
            lines
        }
        (&[begin], &[end]) => {
            if begin >= end {
                return Err(StatusError(
                    "the body's end marker comes before its begin marker".to_string(),
                ));
            }
            let mut lines: Vec<String> = body_lines.get(..begin).unwrap_or_default().to_vec();
            lines.extend(block_lines);
            lines.extend(body_lines.get(end + 1..).unwrap_or_default().to_vec());
            lines
        }
        (nb, ne) => {
            let (nb, ne) = (nb.len(), ne.len());
            return Err(StatusError(format!(
                "the body has {nb} begin marker(s) and {ne} end marker(s); it needs one of each, or neither. Nothing was changed."
            )));
        }
    };

    let mut result = new_lines.join(sep);
    result.push_str(trail);
    Ok(result)
}

fn require_non_empty(label: &str, value: &str) -> Result<(), StatusError> {
    if value.is_empty() {
        return Err(StatusError(format!("--{label} must not be empty")));
    }
    Ok(())
}

fn parse_tier(text: &str) -> Result<(String, String), StatusError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| StatusError(format!("--tier-json is not valid JSON: {e}")))?;
    let tier = value.get("tier").and_then(Value::as_str);
    let reasons = value.get("reasons").and_then(Value::as_array);
    let Some((tier, reasons)) = tier.zip(reasons) else {
        return Err(tier_shape_error());
    };
    if !reasons.iter().all(Value::is_string) {
        return Err(tier_shape_error());
    }
    let joined = reasons
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("; ");
    Ok((tier.to_string(), joined))
}

fn tier_shape_error() -> StatusError {
    StatusError(
        "--tier-json must be a JSON object with a string \"tier\" field and a \"reasons\" array of strings"
            .to_string(),
    )
}

struct ReviewFields {
    decision: String,
    advisory_verdict: Option<String>,
    round_line: Option<String>,
}

fn parse_review(text: &str) -> Result<ReviewFields, StatusError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| StatusError(format!("the review data is not valid JSON: {e}")))?;
    let Some(obj) = value.as_object() else {
        return Err(review_shape_error());
    };

    let decision = match obj.get("reviewDecision") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(_) => return Err(review_shape_error()),
    };
    let reviews = match obj.get("reviews") {
        None => Vec::new(),
        Some(Value::Array(items)) if items.iter().all(Value::is_object) => items.clone(),
        Some(_) => return Err(review_shape_error()),
    };
    let comments_ok = |items: &[Value]| {
        items
            .iter()
            .all(|c| c.is_object() && c.get("body").is_some_and(Value::is_string))
    };
    let comments = match obj.get("comments") {
        None => Vec::new(),
        Some(Value::Array(items)) if comments_ok(items) => items.clone(),
        Some(_) => return Err(review_shape_error()),
    };

    Ok(ReviewFields {
        decision,
        advisory_verdict: advisory_verdict(&reviews),
        round_line: round_line(&comments),
    })
}

fn review_shape_error() -> StatusError {
    StatusError(
        "the review data must have the shape of `gh pr view --json reviewDecision,reviews,comments`"
            .to_string(),
    )
}

/// The verdict named by the last review whose body opens with an advisory
/// verdict line, or none when no review carries one.
fn advisory_verdict(reviews: &[Value]) -> Option<String> {
    let pattern = Regex::new(r"^\*\*Verdict \(advisory\): ([A-Z_]+)").expect("fixed pattern");
    reviews
        .iter()
        .filter_map(|r| r.get("body").and_then(Value::as_str))
        .filter_map(|body| pattern.captures(body).map(|c| c[1].to_string()))
        .next_back()
}

/// The first line of the last comment whose body opens with "Review round ".
fn round_line(comments: &[Value]) -> Option<String> {
    comments
        .iter()
        .filter_map(|c| c.get("body").and_then(Value::as_str))
        .rfind(|body| body.starts_with("Review round "))
        .and_then(|body| body.lines().next().map(str::to_string))
}

#[derive(Default)]
struct GateSummary {
    total: usize,
    passed: usize,
    failed_names: Vec<String>,
    failed_desc: Vec<String>,
}

impl GateSummary {
    /// The checks tab already lists every check by name, so this names only
    /// the failing ones: `all N passed`, `no checks reported yet`, or `P of
    /// N passed, failed: name (reason), ...`.
    fn verified_text(&self) -> String {
        if self.total == 0 {
            return "no checks reported yet".to_string();
        }
        if self.failed_desc.is_empty() {
            return format!("all {} passed", self.total);
        }
        let mut text = format!("{} of {} passed", self.passed, self.total);
        write!(text, ", failed: {}", self.failed_desc.join(", "))
            .expect("writing to a string never fails");
        text
    }
}

/// Parses `"name: passed, name: failed: reason, ..."` into a [`GateSummary`].
///
/// # Errors
/// Returns an error when an entry has no colon (a comma inside a gate name
/// splits it into two entries with no colon in the first one), or when an
/// entry's result is neither `passed` nor `failed`.
fn parse_gates(spec: &str) -> Result<GateSummary, StatusError> {
    let mut summary = GateSummary::default();
    for raw_item in spec.split(',') {
        let item = raw_item.trim();
        if item.is_empty() {
            continue;
        }
        let Some(colon) = item.find(':') else {
            return Err(StatusError(format!(
                "gate entry '{item}' has no ': passed' or ': failed'; a comma inside a gate name splits it into two entries"
            )));
        };
        let name = item[..colon].trim().to_string();
        let result = item[colon + 1..].trim();
        summary.total += 1;
        if result.strip_prefix("passed").is_some() {
            summary.passed += 1;
        } else if let Some(rest) = result.strip_prefix("failed") {
            let reason = rest
                .trim()
                .strip_prefix(':')
                .unwrap_or_else(|| rest.trim())
                .trim();
            summary.failed_names.push(name.clone());
            if reason.is_empty() {
                summary.failed_desc.push(name);
            } else {
                summary.failed_desc.push(format!("{name} ({reason})"));
            }
        } else {
            return Err(StatusError(format!(
                "gate entry '{item}' must end ': passed' or ': failed: <reason>'"
            )));
        }
    }
    Ok(summary)
}

/// The native decision wins; otherwise the latest advisory verdict is used
/// and marked as advisory, and `REVIEW_REQUIRED` means a human is still
/// needed even when the advisory verdict is APPROVE.
fn verdict_and_flags(review: &ReviewFields) -> (String, bool, bool) {
    match review.decision.as_str() {
        "APPROVED" => ("APPROVE".to_string(), false, false),
        "CHANGES_REQUESTED" => ("REQUEST_CHANGES".to_string(), false, false),
        other => {
            let verdict = review.advisory_verdict.clone().unwrap_or_default();
            let advisory = !verdict.is_empty();
            (verdict, advisory, other == "REVIEW_REQUIRED")
        }
    }
}

/// Splits a "Review round N (tier): counts." line into the round number,
/// whether to say "round" or "rounds", and the counts with the prefix and
/// trailing period removed.
fn round_parts(line: &str) -> (String, &'static str, String) {
    let number = Regex::new(r"^Review round (\d+)").expect("fixed pattern");
    let k = number
        .captures(line)
        .map_or_else(|| line.to_string(), |c| c[1].to_string());
    let word = if k == "1" { "round" } else { "rounds" };

    let prefix = Regex::new(r"^Review round \d+ \([a-z]+\): ").expect("fixed pattern");
    let mut counts = prefix.replace(line, "").into_owned();
    if let Some(stripped) = counts.strip_suffix('.') {
        counts = stripped.to_string();
    }
    (k, word, counts)
}

fn review_and_ready(gates: &GateSummary, review: &ReviewFields) -> (String, String) {
    let (verdict, advisory, human_required) = verdict_and_flags(review);
    let verdict_display = if verdict.is_empty() {
        "pending"
    } else {
        verdict.as_str()
    };
    let advisory_suffix = if advisory { " (advisory)" } else { "" };

    let mut review_line = match &review.round_line {
        Some(line) => {
            let (k, word, counts) = round_parts(line);
            format!("{verdict_display}{advisory_suffix}: {k} {word}, {counts}")
        }
        None => format!("{verdict_display}{advisory_suffix}: no round yet"),
    };
    if human_required {
        review_line.push_str("; human approval required");
    }

    let ready = if let Some(first) = gates.failed_names.first() {
        format!("blocked by {first} failed")
    } else if verdict != "APPROVE" {
        format!("blocked by review: {verdict_display}")
    } else if human_required {
        "blocked by review: human approval required".to_string()
    } else {
        "yes".to_string()
    };

    (review_line, ready)
}

#[derive(Clone, Copy)]
enum Newline {
    Lf,
    CrLf,
}

impl Newline {
    const fn token(self) -> &'static str {
        match self {
            Newline::Lf => "\n",
            Newline::CrLf => "\r\n",
        }
    }
}

/// `\r\n` when `text` carries any, else plain `\n`. Whichever line ending
/// dominates a real file is the one used throughout apply's output.
fn newline_style(text: &str) -> Newline {
    if text.contains("\r\n") {
        Newline::CrLf
    } else {
        Newline::Lf
    }
}

/// Splits off the trailing run of `style`, so it can be put back on the end
/// of the result unchanged, however long it is.
fn split_trailing_run(text: &str, style: Newline) -> (&str, &str) {
    let token = style.token();
    let mut end = text.len();
    while end >= token.len() && &text[end - token.len()..end] == token {
        end -= token.len();
    }
    (&text[..end], &text[end..])
}

/// Splits on `\n` and drops a trailing `\r` from each line, so a marker
/// compares equal whether the line came from an `\n` or an `\r\n` file.
fn logical_lines(text: &str) -> Vec<String> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
        .collect()
}

fn positions(lines: &[String], marker: &str) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.as_str() == marker)
        .map(|(i, _)| i)
        .collect()
}

fn validate_block(block: &str) -> Result<(), StatusError> {
    let lines = logical_lines(block);
    let begins = positions(&lines, BEGIN);
    let ends = positions(&lines, END);
    let (&[begin], &[end]) = (begins.as_slice(), ends.as_slice()) else {
        return Err(StatusError(
            "the block must carry each marker exactly once, each on its own line".to_string(),
        ));
    };
    if begin >= end {
        return Err(StatusError(
            "the block's end marker comes before its begin marker".to_string(),
        ));
    }
    Ok(())
}

/// The block currently between the markers in `body`, rebuilt with `\n`
/// line endings the way [`render`] produces one, or `None` when `body`
/// carries no block at all.
///
/// # Errors
/// Returns an error when `body` carries the markers in a shape this cannot
/// resolve: one without the other, a repeat, or reversed.
pub fn current_block(body: &str) -> Result<Option<String>, StatusError> {
    let lines = logical_lines(body);
    let begins = positions(&lines, BEGIN);
    let ends = positions(&lines, END);
    match (begins.as_slice(), ends.as_slice()) {
        ([], []) => Ok(None),
        (&[begin], &[end]) => {
            if begin >= end {
                return Err(StatusError(
                    "the body's end marker comes before its begin marker".to_string(),
                ));
            }
            let slice = lines.get(begin..=end).unwrap_or_default();
            Ok(Some(format!("{}\n", slice.join("\n"))))
        }
        (nb, ne) => {
            let (nb, ne) = (nb.len(), ne.len());
            Err(StatusError(format!(
                "the body has {nb} begin marker(s) and {ne} end marker(s); it needs one of each, or neither."
            )))
        }
    }
}

/// Whether `rendered` already matches, byte for byte, the block sitting in
/// `body`. `false` both when the block in `body` differs, and when `body`
/// carries no block at all, since either way `body` needs `rendered` put
/// into it.
///
/// # Errors
/// Propagates a marker-shape error from [`current_block`].
pub fn is_unchanged(body: &str, rendered: &str) -> Result<bool, StatusError> {
    Ok(current_block(body)?.as_deref() == Some(rendered))
}

/// `Problem` and `Approach`, read back out of the block already in `body`.
/// `None` when `body` carries no block yet.
///
/// # Errors
/// Returns an error when `body` carries the markers in a shape
/// [`current_block`] cannot resolve, or when a block is there but is
/// missing a `**Problem**:` or `**Approach**:` line.
pub fn extract_problem_approach(body: &str) -> Result<Option<(String, String)>, StatusError> {
    let Some(block) = current_block(body)? else {
        return Ok(None);
    };
    let problem = block
        .lines()
        .find_map(|line| line.strip_prefix("**Problem**: "));
    let approach = block
        .lines()
        .find_map(|line| line.strip_prefix("**Approach**: "));
    match (problem, approach) {
        (Some(p), Some(a)) => Ok(Some((p.to_string(), a.to_string()))),
        _ => Err(StatusError(
            "the existing status block has no **Problem**: or **Approach**: line to reuse"
                .to_string(),
        )),
    }
}

/// Turns `gh pr checks --json name,state,bucket` output into the
/// comma-separated gate spec [`render`] understands: `name: passed` when
/// the bucket is `pass`, `name: failed: <state>` when it is `fail`, and
/// left out of the list entirely for any other bucket (pending, skipping,
/// or cancel). `skip_name` is left out too, so the check running this very
/// refresh never reports on itself.
///
/// # Errors
/// Returns an error when the JSON is not an array of objects each with a
/// string `name`, `state`, and `bucket`.
pub fn gates_from_checks_json(text: &str, skip_name: &str) -> Result<String, StatusError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| StatusError(format!("the checks data is not valid JSON: {e}")))?;
    let Some(items) = value.as_array() else {
        return Err(checks_shape_error());
    };
    let mut parts = Vec::new();
    for item in items {
        let obj = item.as_object().ok_or_else(checks_shape_error)?;
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(checks_shape_error)?;
        let state = obj
            .get("state")
            .and_then(Value::as_str)
            .ok_or_else(checks_shape_error)?;
        let bucket = obj
            .get("bucket")
            .and_then(Value::as_str)
            .ok_or_else(checks_shape_error)?;
        if name == skip_name {
            continue;
        }
        match bucket {
            "pass" => parts.push(format!("{name}: passed")),
            "fail" => parts.push(format!("{name}: failed: {state}")),
            _ => {}
        }
    }
    Ok(parts.join(", "))
}

fn checks_shape_error() -> StatusError {
    StatusError(
        "the checks data must have the shape of `gh pr checks --json name,state,bucket`"
            .to_string(),
    )
}

/// A pull request's description and the two branches it runs between, from
/// `gh pr view --json body,baseRefName,headRefName`.
pub struct PrInfo {
    pub body: String,
    pub base_ref: String,
    pub head_ref: String,
}

/// Parses [`PrInfo`] out of `gh pr view --json body,baseRefName,headRefName` output.
///
/// # Errors
/// Returns an error when the JSON does not have that shape.
pub fn parse_pr_info(text: &str) -> Result<PrInfo, StatusError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| StatusError(format!("the pull request data is not valid JSON: {e}")))?;
    let shape_error = || {
        StatusError(
            "the pull request data must have the shape of `gh pr view --json body,baseRefName,headRefName`"
                .to_string(),
        )
    };
    let body = value
        .get("body")
        .and_then(Value::as_str)
        .ok_or_else(shape_error)?;
    let base_ref = value
        .get("baseRefName")
        .and_then(Value::as_str)
        .ok_or_else(shape_error)?;
    let head_ref = value
        .get("headRefName")
        .and_then(Value::as_str)
        .ok_or_else(shape_error)?;
    Ok(PrInfo {
        body: body.to_string(),
        base_ref: base_ref.to_string(),
        head_ref: head_ref.to_string(),
    })
}

/// What [`apply`] and `osf status refresh` need from GitHub: reading a
/// pull request's description and metadata, its checks, its review state,
/// and writing a new description back. A trait so a test can supply a fake
/// instead of shelling out to `gh`.
pub trait GhClient {
    /// # Errors
    /// Returns an error when `gh` cannot run or exits non-zero.
    fn view_body(&self, repo: &str, pr: &str) -> Result<String, StatusError>;
    /// # Errors
    /// Returns an error when `gh` cannot run or exits non-zero.
    fn view_review(&self, repo: &str, pr: &str) -> Result<String, StatusError>;
    /// # Errors
    /// Returns an error when `gh` cannot run or exits non-zero.
    fn view_pr_info(&self, repo: &str, pr: &str) -> Result<String, StatusError>;
    /// # Errors
    /// Returns an error when `gh` cannot run or exits non-zero.
    fn view_checks(&self, repo: &str, pr: &str) -> Result<String, StatusError>;
    /// # Errors
    /// Returns an error when `gh` cannot run or exits non-zero.
    fn edit_body(&self, repo: &str, pr: &str, body: &str) -> Result<(), StatusError>;
}

/// Reads the description for `repo`#`pr`, applies `block`, and writes the
/// result back. A failed read never reaches the write.
///
/// # Errors
/// Returns an error when the description could not be read, `block` could
/// not be applied to it, or the result could not be written back.
pub fn apply_via_gh(
    client: &dyn GhClient,
    repo: &str,
    pr: &str,
    block: &str,
) -> Result<String, StatusError> {
    let body = client.view_body(repo, pr)?;
    let new_body = apply(&body, block)?;
    client.edit_body(repo, pr, &new_body)?;
    Ok(new_body)
}

/// Calls the real `gh` command line tool.
pub struct RealGh;

#[allow(clippy::unused_self)]
impl GhClient for RealGh {
    fn view_body(&self, repo: &str, pr: &str) -> Result<String, StatusError> {
        let output = Command::new("gh")
            .args([
                "pr", "view", pr, "--repo", repo, "--json", "body", "-q", ".body",
            ])
            .output()
            .map_err(|e| StatusError(format!("cannot run gh: {e}")))?;
        if !output.status.success() {
            return Err(StatusError(format!(
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
            .map_err(|e| StatusError(format!("cannot run gh: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(StatusError(format!(
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
            .map_err(|e| StatusError(format!("cannot run gh: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(StatusError(format!(
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
            .map_err(|e| StatusError(format!("cannot run gh: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.to_lowercase().contains("no checks reported") {
                return Ok("[]".to_string());
            }
            return Err(StatusError(format!(
                "gh pr checks failed for {repo}#{pr}: {}",
                stderr.trim()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn edit_body(&self, repo: &str, pr: &str, body: &str) -> Result<(), StatusError> {
        let tmp = std::env::temp_dir().join(format!("osf-status-{}.md", std::process::id()));
        std::fs::write(&tmp, body)
            .map_err(|e| StatusError(format!("cannot write a temporary file: {e}")))?;
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
        let output = run.map_err(|e| StatusError(format!("cannot run gh: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(StatusError(format!(
                "gh pr edit failed for {repo}#{pr}: {}",
                stderr.trim()
            )));
        }
        Ok(())
    }
}
