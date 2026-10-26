//! `osf pr status`: the block at the top of a pull request description that
//! says whether the change is ready to merge. Rendering is a pure function
//! of its inputs, so the same inputs always give the same bytes; applying
//! replaces the block between two markers, or adds it at the top, without
//! touching anything else in the description. The layout of the whole
//! description lives in `.github/PULL_REQUEST_TEMPLATE.md`; this module
//! writes only the `osf:status` block of it.

use crate::marker;
use regex::Regex;
use serde_json::Value;
use std::fmt::{self, Write as _};

const NAME: &str = "status";
const SHORT_SHA_LEN: usize = 7;

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
/// gate results, the review JSON, and the commit the block is built for.
/// Reading files or calling `gh` is the caller's job, so this stays a pure
/// function.
pub struct RenderInput<'a> {
    pub tier_json: &'a str,
    pub gates: &'a str,
    pub review_json: &'a str,
    /// The full commit hash of the pull request's head.
    pub head: &'a str,
    /// The Rust test summary, already rendered by
    /// [`crate::changeset_tests::render`], or `None` to leave it out: this
    /// module stays a pure function of its inputs and never builds one
    /// itself.
    pub tests: Option<&'a str>,
    /// One line for the Automated review row's Details, such as the model
    /// family, the rounds and the fixing commit. `None` or empty keeps the
    /// details computed from the review comments.
    pub automated_review: Option<&'a str>,
    /// One line for the Human review row's Details, such as reviewer display
    /// names. `None` or empty keeps the details computed from the decision.
    pub human_review: Option<&'a str>,
}

/// Renders the status block: a heading naming the head commit, a table of
/// checks with one row each, and a collapsed explanation of the risk. A
/// result is one icon and a few words: passed, waiting, failed, or not run
/// with the reason. A check that did not run is never shown as passed.
///
/// # Errors
/// Returns an error when `head` is empty, the tier JSON does not have a
/// string `tier` and a `reasons` array of strings, a gate entry is not
/// `name: passed` or `name: failed: <reason>`, or the review JSON does not
/// have the shape of `gh pr view --json reviewDecision,reviews,comments`.
pub fn render(input: &RenderInput) -> Result<String, StatusError> {
    require_non_empty("head", input.head)?;

    let (tier, reasons) = parse_tier(input.tier_json)?;
    let gates = parse_gates(input.gates)?;
    let review = parse_review(input.review_json)?;
    let short = input.head.get(..SHORT_SHA_LEN).unwrap_or(input.head);

    let (risk_result, level) = risk_label(&tier);
    let risk_details = reasons.first().map_or("no reason recorded", String::as_str);
    let (tests_result, tests_details, tests_rest) = tests_row(input.tests);
    let (ci_result, ci_details) = gates.ci_row();
    let (auto_result, auto_details) = automated_review_row(&review);
    let (human_result, human_details) = human_review_row(&review);
    let auto_details =
        review_override("--automated-review", input.automated_review)?.unwrap_or(auto_details);
    let human_details =
        review_override("--human-review", input.human_review)?.unwrap_or(human_details);

    let mut out = String::new();
    writeln!(out, "{}", marker::start(NAME, input.head)).expect("writing to a string never fails");
    write!(
        out,
        "### Status at `{short}`\n\
         \n\
         | Check | Result | Details |\n\
         |---|---|---|\n\
         | **Risk** | {risk_result} | {risk} |\n\
         | **Tests** | {tests_result} | {tests_details} |\n\
         | **CI** | {ci_result} | {ci_details} |\n\
         | **Commit messages** | ⏸ not run | no check lints commit messages yet |\n\
         | **Contributor agreement** | ⏸ not run | no check yet; a first-time author posts the CONTRIBUTING.md sentence |\n\
         | **Automated review** | {auto_result} | {auto_details} |\n\
         | **Human review** | {human_result} | {human_details} |\n",
        human_details = cell(&human_details),
        risk_result = cell(&risk_result),
        risk = cell(risk_details),
        tests_result = cell(&tests_result),
        ci_result = cell(&ci_result),
        ci_details = cell(&ci_details),
        auto_result = cell(&auto_result),
        auto_details = cell(&auto_details),
    )
    .expect("writing to a string never fails");

    if !reasons.is_empty() {
        write!(
            out,
            "\n<details>\n<summary><b>Why the risk is {level}</b></summary>\n\n"
        )
        .expect("writing to a string never fails");
        for reason in &reasons {
            writeln!(out, "- {reason}").expect("writing to a string never fails");
        }
        out.push_str("\n</details>\n");
    }
    if !review.rounds.is_empty() {
        out.push_str("\n<details>\n<summary><b>Automated review rounds</b></summary>\n\n");
        for round in &review.rounds {
            writeln!(out, "- {}", round_bullet(round)).expect("writing to a string never fails");
        }
        out.push_str("\n</details>\n");
    }
    if let Some(rest) = tests_rest {
        write!(
            out,
            "\n<details>\n<summary><b>Test changes</b></summary>\n\n{rest}\n\n</details>\n"
        )
        .expect("writing to a string never fails");
    }
    writeln!(out, "{}", marker::end(NAME)).expect("writing to a string never fails");
    Ok(out)
}

/// A table cell holds one line, and a bar would end it early.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

/// The Tests row from the rendered test summary: its first line becomes the
/// result, and the detail lines under it go in a collapsed section.
fn tests_row(tests: Option<&str>) -> (String, String, Option<String>) {
    let Some(text) = tests else {
        return (
            "⏸ not run".to_string(),
            "no base to compare against".to_string(),
            None,
        );
    };
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default();
    let summary = first.strip_prefix("**Tests**: ").unwrap_or(first);
    let rest: Vec<&str> = lines.collect();
    let rest = (!rest.is_empty()).then(|| rest.join("\n"));
    let result = if summary == "0 added, 0 changed, 0 removed" {
        "➖ none automated".to_string()
    } else {
        format!("✅ {summary}")
    };
    (result, "[Testing notes](#testing-notes)".to_string(), rest)
}

/// The Risk row's dot and level, and the level word for the collapsed
/// section: 🔴 High, 🟠 Medium (the `normal` tier), 🟢 Low. A tier this
/// build does not know keeps its own name behind a white dot.
fn risk_label(tier: &str) -> (String, String) {
    match tier {
        "high" => ("🔴 High".to_string(), "high".to_string()),
        "normal" | "medium" => ("🟠 Medium".to_string(), "medium".to_string()),
        "low" => ("🟢 Low".to_string(), "low".to_string()),
        other => (format!("⚪ {other}"), other.to_string()),
    }
}

/// One bullet for the rounds section: the round and its tier in bold, then
/// the counts. A line that does not have the usual shape is kept as written.
fn round_bullet(line: &str) -> String {
    let shape = Regex::new(r"^Review round (\d+) \(([a-z]+)\): (.*?)\.?$").expect("fixed pattern");
    match shape.captures(line) {
        Some(c) => format!("**Round {} ({}):** {}", &c[1], &c[2], &c[3]),
        None => line.to_string(),
    }
}

/// The Automated review row: the verdict of the last advisory review, and
/// the latest review round's counts.
fn automated_review_row(review: &ReviewFields) -> (String, String) {
    let result = match review.advisory_verdict.as_deref() {
        Some("APPROVE") => "✅ clean".to_string(),
        Some("REQUEST_CHANGES") => "❌ changes requested".to_string(),
        Some(other) => format!("⏳ {other}"),
        None => "⏳ waiting".to_string(),
    };
    let details = match &review.round_line {
        Some(line) => {
            let (k, word, counts) = round_parts(line);
            format!("{k} {word}, {counts}")
        }
        None => "no round yet".to_string(),
    };
    (result, details)
}

/// The Human review row, from the decision GitHub reports.
fn human_review_row(review: &ReviewFields) -> (&'static str, String) {
    let (result, details) = match review.decision.as_str() {
        "APPROVED" => ("✅ approved", "approved on GitHub"),
        "CHANGES_REQUESTED" => ("❌ changes requested", "changes requested on GitHub"),
        _ => ("⏳ waiting", "no approving review yet"),
    };
    (result, details.to_string())
}

/// The caller's one-line Details for a review row: `None` when absent or
/// empty, an error when it spans lines, since a table cell holds one line.
fn review_override(label: &str, value: Option<&str>) -> Result<Option<String>, StatusError> {
    let Some(text) = value.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.contains(['\n', '\r']) {
        return Err(StatusError(format!("{label} must be one line")));
    }
    Ok(Some(text.to_string()))
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
    let begins = start_positions(&body_lines);
    let ends = end_positions(&body_lines);

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

fn parse_tier(text: &str) -> Result<(String, Vec<String>), StatusError> {
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
    let reasons = reasons
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    Ok((tier.to_string(), reasons))
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
    /// The first line of every review-round comment, oldest first.
    rounds: Vec<String>,
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
        rounds: round_lines(&comments),
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
    round_lines(comments).pop()
}

/// The first line of every comment whose body opens with "Review round ".
fn round_lines(comments: &[Value]) -> Vec<String> {
    comments
        .iter()
        .filter_map(|c| c.get("body").and_then(Value::as_str))
        .filter(|body| body.starts_with("Review round "))
        .filter_map(|body| body.lines().next().map(str::to_string))
        .collect()
}

#[derive(Default)]
struct GateSummary {
    total: usize,
    passed: usize,
    failed_desc: Vec<String>,
}

impl GateSummary {
    /// The CI row. The checks tab already lists every check by name, so the
    /// details name only the failing ones: `all N passed`, `no checks
    /// reported yet`, or `P of N passed, failed: name (reason), ...`.
    fn ci_row(&self) -> (String, String) {
        if self.total == 0 {
            return (
                "⏳ waiting".to_string(),
                "no checks reported yet".to_string(),
            );
        }
        if self.failed_desc.is_empty() {
            return (
                "✅ passed".to_string(),
                format!("all {} passed", self.total),
            );
        }
        let details = format!(
            "{} of {} passed, failed: {}",
            self.passed,
            self.total,
            self.failed_desc.join(", ")
        );
        ("❌ failed".to_string(), details)
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

/// The lines that are a status start marker, in either the current or the
/// old style.
fn start_positions(lines: &[String]) -> Vec<usize> {
    positions(lines, |line| marker::is_start_line(line, NAME))
}

/// The lines that are a status end marker, in either the current or the old
/// style.
fn end_positions(lines: &[String]) -> Vec<usize> {
    positions(lines, |line| marker::is_end_line(line, NAME))
}

fn positions(lines: &[String], is_marker: impl Fn(&str) -> bool) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| is_marker(line.as_str()))
        .map(|(i, _)| i)
        .collect()
}

fn validate_block(block: &str) -> Result<(), StatusError> {
    let lines = logical_lines(block);
    let begins = start_positions(&lines);
    let ends = end_positions(&lines);
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
    let begins = start_positions(&lines);
    let ends = end_positions(&lines);
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

/// Reads `gh pr checks --json name,state,bucket` output into one
/// `(name, state, bucket)` triple per entry, in order.
///
/// # Errors
/// Returns an error when the JSON is not an array of objects each with a
/// string `name`, `state`, and `bucket`.
pub fn parse_checks(text: &str) -> Result<Vec<(String, String, String)>, StatusError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| StatusError(format!("the checks data is not valid JSON: {e}")))?;
    let Some(items) = value.as_array() else {
        return Err(checks_shape_error());
    };
    let mut checks = Vec::new();
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
        checks.push((name.to_string(), state.to_string(), bucket.to_string()));
    }
    Ok(checks)
}

/// The gate spec for [`render`] from `(name, state, bucket)` triples, skipping `skip_name`.
#[must_use]
pub fn gates_from_checks(checks: &[(String, String, String)], skip_name: &str) -> String {
    let mut parts = Vec::new();
    for (name, state, bucket) in checks {
        if name.as_str() == skip_name {
            continue;
        }
        match bucket.as_str() {
            "pass" => parts.push(format!("{name}: passed")),
            "fail" => parts.push(format!("{name}: failed: {state}")),
            _ => {}
        }
    }
    parts.join(", ")
}

/// Turns `gh pr checks --json name,state,bucket` output into the
/// comma-separated gate spec [`render`] understands: `name: passed` when
/// the bucket is `pass`, `name: failed: <state>` when it is `fail`, and
/// left out of the list entirely for any other bucket (pending, skipping,
/// or cancel). `skip_name` is left out too, so the check running this very
/// refresh never reports on itself.
///
/// # Errors
/// Returns an error when [`parse_checks`] cannot read the checks data.
pub fn gates_from_checks_json(text: &str, skip_name: &str) -> Result<String, StatusError> {
    Ok(gates_from_checks(&parse_checks(text)?, skip_name))
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

/// The raw result of one `gh` command that ran: exit success, status, stdout, stderr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhOutput {
    pub success: bool,
    pub status: String,
    pub stdout: String,
    pub stderr: String,
}

/// What [`apply`] and `osf pr status refresh` need from GitHub: reading a
/// pull request's description and metadata, its checks, its review state,
/// and writing a new description back. A trait so a test can supply a fake
/// instead of shelling out to `gh`.
pub trait GhClient {
    /// Runs one `gh` command, writes `stdin` to its standard input when given, and returns the raw result of a command that ran; the error is only for a command that could not start or could not be fed.
    ///
    /// # Errors
    /// Returns an error when `gh` cannot start, or its standard input cannot be written.
    fn run(&self, args: &[String], stdin: Option<&str>) -> Result<GhOutput, StatusError>;

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

#[cfg(test)]
mod tests {
    use super::*;

    const CHECKS: &str = r#"[
        {"name":"hygiene","state":"SUCCESS","bucket":"pass"},
        {"name":"rust","state":"FAILURE","bucket":"fail"},
        {"name":"slow-check","state":"PENDING","bucket":"pending"}
    ]"#;

    #[test]
    fn parse_checks_reads_every_entry_in_order() {
        let checks = parse_checks(CHECKS).expect("valid checks JSON");
        assert_eq!(
            checks,
            vec![
                (
                    "hygiene".to_string(),
                    "SUCCESS".to_string(),
                    "pass".to_string()
                ),
                (
                    "rust".to_string(),
                    "FAILURE".to_string(),
                    "fail".to_string()
                ),
                (
                    "slow-check".to_string(),
                    "PENDING".to_string(),
                    "pending".to_string()
                ),
            ]
        );
    }

    #[test]
    fn parse_checks_refuses_a_bad_shape_with_the_existing_text() {
        let err = parse_checks(r#"{"name":"only-one-object"}"#).expect_err("not an array");
        assert_eq!(
            err.to_string(),
            "the checks data must have the shape of `gh pr checks --json name,state,bucket`"
        );
    }

    #[test]
    fn parse_checks_refuses_a_missing_field_with_the_existing_text() {
        let err = parse_checks(r#"[{"name":"a","state":"SUCCESS"}]"#).expect_err("no bucket");
        assert_eq!(
            err.to_string(),
            "the checks data must have the shape of `gh pr checks --json name,state,bucket`"
        );
    }

    #[test]
    fn gates_from_checks_json_still_maps_buckets_and_skips_the_self_check() {
        let gates = gates_from_checks_json(CHECKS, "hygiene").expect("valid checks JSON");
        assert_eq!(gates, "rust: failed: FAILURE");
    }

    #[test]
    fn gates_from_checks_json_still_reads_an_empty_array_as_empty() {
        let gates = gates_from_checks_json("[]", "status block").expect("valid checks JSON");
        assert_eq!(gates, "");
    }

    #[test]
    fn the_module_text_names_no_process_command() {
        let text = include_str!("pr_status.rs");
        let word = ["Com", "mand"].concat();
        let words: Vec<&str> = text.split(|c: char| !c.is_alphanumeric()).collect();
        assert!(
            !words.contains(&word.as_str()),
            "the pure status module must hold no process-starting code"
        );
    }
}
