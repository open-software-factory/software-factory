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
        /// The issue number, without the repository.
        number: u64,
        /// The head commit the work item was found for.
        head: String,
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

/// `n` spaces, to blank a hidden region while keeping the line structure.
fn spaces(n: usize) -> String {
    " ".repeat(n)
}

/// Whether `line` starts an indented code block.
fn is_indented_code(line: &str) -> bool {
    line.starts_with('\t') || line.starts_with("    ")
}

/// `trimmed` with one leading list marker and the spaces after it removed.
fn strip_list_marker(trimmed: &str) -> &str {
    let marker_len = match trimmed.chars().next() {
        Some('-' | '*' | '+') => 1,
        Some(c) if c.is_ascii_digit() => {
            let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
            match trimmed.chars().nth(digits) {
                Some('.' | ')') => digits + 1,
                _ => return trimmed,
            }
        }
        _ => return trimmed,
    };
    let after = trimmed.get(marker_len..).unwrap_or("");
    let content = after.trim_start_matches([' ', '\t']);
    if content.len() < after.len() {
        content
    } else {
        trimmed
    }
}

/// The fence a line opens with, such as three backticks or three tildes.
/// A backtick run opens a fence only when no backtick follows it on the line.
fn fence_marker(trimmed: &str) -> Option<(char, usize)> {
    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let len = trimmed.chars().take_while(|c| *c == marker).count();
    if len < 3 {
        return None;
    }
    let after = trimmed.get(len..).unwrap_or("");
    if marker == '`' && after.contains('`') {
        return None;
    }
    Some((marker, len))
}

/// Whether `trimmed` closes a fence opened with `marker` repeated `len` times.
fn closes_fence(trimmed: &str, marker: char, len: usize) -> bool {
    let run = trimmed.chars().take_while(|c| *c == marker).count();
    run >= len && trimmed.chars().skip(run).all(|c| c == ' ' || c == '\t')
}

/// The byte offset of the next backtick run of exactly `run` in `text`.
fn closing_ticks(text: &str, run: usize) -> Option<usize> {
    let mut base = 0;
    let mut search = text;
    loop {
        let at = search.find('`')?;
        let (_, from_tick) = search.split_at(at);
        let len = from_tick.chars().take_while(|c| *c == '`').count();
        if len == run {
            return Some(base + at);
        }
        let (_, after_run) = from_tick.split_at(len);
        base += at + len;
        search = after_run;
    }
}

/// The byte range of a destination title in `inner`, quotes included.
fn title_span(inner: &str) -> Option<(usize, usize)> {
    let body = inner.trim_end();
    let end = body.len();
    let last = body.chars().last()?;
    let start = match last {
        '"' | '\'' => body.get(..end - 1)?.rfind(last)?,
        ')' => {
            let mut depth = 0usize;
            let mut open = None;
            for (at, c) in body.char_indices().rev().skip(1) {
                match c {
                    ')' => depth += 1,
                    '(' if depth == 0 => {
                        open = Some(at);
                        break;
                    }
                    '(' => depth -= 1,
                    _ => {}
                }
            }
            open?
        }
        _ => return None,
    };
    let before = body.get(..start)?;
    before
        .chars()
        .last()
        .is_some_and(char::is_whitespace)
        .then_some((start, end))
}

/// The byte offset of the `)` that closes the `(` at `open`.
fn matching_paren(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, c) in text.get(open..)?.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return None,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Blank each image's alt text between `![` and the matching `]`.
fn blank_image_alt(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find("![") {
        let Some(close) = rest.get(at + 2..).and_then(|after| after.find(']')) else {
            break;
        };
        let start = at + 2;
        let end = start + close;
        out.push_str(rest.get(..start).unwrap_or(""));
        out.push_str(&spaces(
            rest.get(start..end).map_or(0, |s| s.chars().count()),
        ));
        rest = rest.get(end..).unwrap_or("");
    }
    out.push_str(rest);
    out
}

/// Blank a link or image title inside `](...)`, quotes included.
fn blank_link_title(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find("](") {
        let open = at + 1;
        let Some(close) = matching_paren(rest, open) else {
            break;
        };
        let inner = rest.get(open + 1..close).unwrap_or("");
        out.push_str(rest.get(..open + 1).unwrap_or(""));
        match title_span(inner) {
            Some((start, end)) => {
                out.push_str(inner.get(..start).unwrap_or(""));
                out.push_str(&spaces(
                    inner.get(start..end).map_or(0, |s| s.chars().count()),
                ));
                out.push_str(inner.get(end..).unwrap_or(""));
            }
            None => out.push_str(inner),
        }
        out.push(')');
        rest = rest.get(close + 1..).unwrap_or("");
    }
    out.push_str(rest);
    out
}

/// Blank a link reference definition's title on the line, quotes included.
fn blank_reference_definition(line: &str) -> String {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 || !trimmed.starts_with('[') {
        return line.to_string();
    }
    let Some(colon) = trimmed.find("]:") else {
        return line.to_string();
    };
    let after = trimmed.get(colon + 2..).unwrap_or("");
    let Some((start, end)) = title_span(after) else {
        return line.to_string();
    };
    let prefix = line.len() - after.len();
    format!(
        "{}{}{}",
        line.get(..prefix + start).unwrap_or(""),
        spaces(after.get(start..end).map_or(0, |s| s.chars().count())),
        after.get(end..).unwrap_or("")
    )
}

/// Blank an HTML tag that holds whitespace, up to the next `>` on the line.
fn blank_html_tag(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    loop {
        let Some(at) = rest.find('<') else {
            out.push_str(rest);
            return out;
        };
        let (before, from_lt) = rest.split_at(at);
        out.push_str(before);
        let after_lt = from_lt.get(1..).unwrap_or("");
        let opens_tag = after_lt
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '/');
        if let (true, Some(end)) = (opens_tag, after_lt.find('>')) {
            let tag = from_lt.get(..end + 2).unwrap_or(from_lt);
            if tag.chars().any(char::is_whitespace) {
                out.push_str(&spaces(tag.chars().count()));
            } else {
                out.push_str(tag);
            }
            rest = from_lt.get(end + 2..).unwrap_or("");
        } else {
            out.push('<');
            rest = after_lt;
        }
    }
}

/// Blank the hidden parts of links, images, reference titles and HTML tags.
fn blank_inline(line: &str) -> String {
    let line = blank_reference_definition(line);
    let line = blank_image_alt(&line);
    let line = blank_link_title(&line);
    blank_html_tag(&line)
}

/// `line` with code spans, comments, links, images and tags blanked; whether a comment is still open.
fn inline_prose(line: &str, in_comment: bool) -> (String, bool) {
    let (text, still) = inline_code(line, in_comment);
    (blank_inline(&text), still)
}

/// `line` with inline code spans and HTML comments blanked; whether a comment is still open.
fn inline_code(line: &str, mut in_comment: bool) -> (String, bool) {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    loop {
        if in_comment {
            if let Some((before, after)) = rest.split_once("-->") {
                out.push_str(&spaces(before.chars().count() + 3));
                rest = after;
                in_comment = false;
                continue;
            }
            out.push_str(&spaces(rest.chars().count()));
            return (out, true);
        }
        let comment_at = rest.find("<!--");
        let tick_at = rest.find('`');
        if let Some(at) = comment_at.filter(|at| tick_at.is_none_or(|tick| *at < tick)) {
            let (before, after) = rest.split_at(at);
            out.push_str(before);
            out.push_str("    ");
            rest = after.strip_prefix("<!--").unwrap_or(after);
            in_comment = true;
            continue;
        }
        if let Some(tick) = tick_at {
            let (before, from_tick) = rest.split_at(tick);
            let run = from_tick.chars().take_while(|c| *c == '`').count();
            let after_open = from_tick.strip_prefix(&"`".repeat(run)).unwrap_or("");
            out.push_str(before);
            if let Some(close) = closing_ticks(after_open, run) {
                out.push_str(&spaces(2 * run + close));
                rest = after_open.get(close + run..).unwrap_or("");
            } else {
                // Unmatched backticks are literal text.
                out.push_str(&"`".repeat(run));
                rest = after_open;
            }
            continue;
        }
        out.push_str(rest);
        return (out, false);
    }
}

/// Whether `line` is a link reference definition whose destination has no title.
fn reference_title_missing(line: &str) -> bool {
    let trimmed = line.trim_start_matches(' ');
    if line.len() - trimmed.len() > 3 || !trimmed.starts_with('[') {
        return false;
    }
    let Some(colon) = trimmed.find("]:") else {
        return false;
    };
    let after = trimmed.get(colon + 2..).unwrap_or("");
    !after.trim().is_empty() && title_span(after).is_none()
}

/// `line` with a title that fills it blanked, for a reference title on the next line.
fn blank_standalone_title(line: &str) -> String {
    let trimmed = line.trim();
    let Some(first) = trimmed.chars().next() else {
        return line.to_string();
    };
    let closed = match first {
        '"' | '\'' => trimmed.len() > 1 && trimmed.ends_with(first),
        '(' => trimmed.ends_with(')'),
        _ => return line.to_string(),
    };
    if !closed {
        return line.to_string();
    }
    let start = line.len() - line.trim_start().len();
    let end = start + trimmed.len();
    format!(
        "{}{}{}",
        line.get(..start).unwrap_or(""),
        spaces(trimmed.chars().count()),
        line.get(end..).unwrap_or("")
    )
}

/// `text` with CRLF and then a lone CR rewritten as LF.
pub(crate) fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// `body` with code blocks, code spans, HTML comments and block quotes blanked out, reading CRLF and lone CR as line ends.
pub(crate) fn prose(body: &str) -> String {
    let body = normalize_newlines(body);
    let mut out = String::with_capacity(body.len());
    let mut fence: Option<(char, usize)> = None;
    let mut in_comment = false;
    let mut reference_title_next = false;
    for piece in body.split_inclusive('\n') {
        let (content, newline) = match piece.strip_suffix('\n') {
            Some(content) => (content, "\n"),
            None => (piece, ""),
        };
        let reference_title_here = reference_title_next;
        reference_title_next = false;
        if in_comment {
            let (processed, still) = inline_prose(content, true);
            out.push_str(&processed);
            out.push_str(newline);
            in_comment = still;
            continue;
        }
        if let Some((marker, len)) = fence {
            if closes_fence(content.trim_start_matches([' ', '\t']), marker, len) {
                fence = None;
            }
            out.push_str(&spaces(content.chars().count()));
            out.push_str(newline);
            continue;
        }
        let trimmed = content.trim_start_matches([' ', '\t']);
        if is_indented_code(content) {
            out.push_str(&spaces(content.chars().count()));
            out.push_str(newline);
            continue;
        }
        if let Some(marker) = fence_marker(strip_list_marker(trimmed)) {
            fence = Some(marker);
            out.push_str(&spaces(content.chars().count()));
            out.push_str(newline);
            continue;
        }
        if trimmed.starts_with('>') {
            out.push_str(&spaces(content.chars().count()));
            out.push_str(newline);
            continue;
        }
        let visible = if reference_title_here {
            blank_standalone_title(content)
        } else {
            content.to_string()
        };
        reference_title_next = reference_title_missing(&visible);
        let (processed, still) = inline_prose(&visible, false);
        out.push_str(&processed);
        out.push_str(newline);
        in_comment = still;
    }
    out
}

/// The words before a closing keyword searched for a negation.
const NEGATION_WINDOW: usize = 6;

/// Whether a negation word sits before a closing keyword in the same sentence.
fn negated_before(before: &str) -> bool {
    let start = before.rfind(['.', '!', '?', '\n']).map_or(0, |at| at + 1);
    let words: Vec<String> = before
        .get(start..)
        .unwrap_or("")
        .split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')
                .to_ascii_lowercase()
        })
        .collect();
    let window = words
        .get(words.len().saturating_sub(NEGATION_WINDOW)..)
        .unwrap_or(&[]);
    window.iter().enumerate().any(|(index, word)| {
        word == "not"
            || word == "never"
            || word == "without"
            || word == "cannot"
            || word.contains("n't")
            || (word == "no" && window.get(index + 1).is_some_and(|next| next == "longer"))
            || (word == "longer"
                && index > 0
                && window.get(index - 1).is_some_and(|prev| prev == "no"))
    })
}

/// The issues `body` names in prose, in order: first on its `Issue:` lines, then after a closing keyword.
fn candidates(body: &str) -> Vec<Reference> {
    let text = prose(body);
    let mut found: Vec<Reference> = Vec::new();
    let mut add = |items: Vec<Reference>| {
        for item in items {
            if !found.contains(&item) {
                found.push(item);
            }
        }
    };
    for line in text.lines() {
        let line = line.trim_start_matches(['-', '*', ' ', '\t']);
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("issue:") || lower.starts_with("issues:") {
            add(references(line));
        }
    }
    for caps in closing_pattern().captures_iter(&text) {
        let Some(whole) = caps.get(0) else {
            continue;
        };
        if negated_before(text.get(..whole.start()).unwrap_or("")) {
            continue;
        }
        if let Some(token) = caps.get(1) {
            add(references(token.as_str()));
        }
    }
    found
}

/// The first candidate in `repository`.
fn own_issue(repository: &str, all: &[Reference]) -> Option<u64> {
    all.iter().find_map(|(repo, number)| match repo {
        Some(named) if !named.eq_ignore_ascii_case(repository) => None,
        _ => Some(*number),
    })
}

/// The issue number the body of a pull request names for `repository`: the
/// first prose candidate in that repository, or `None` when it names none there.
#[must_use]
pub fn named_issue(repository: &str, pr_body: &str) -> Option<u64> {
    own_issue(repository, &candidates(pr_body))
}

/// The work item for the pull request whose body is `pr_body` and whose head
/// commit is `head`, in `repository`.
///
/// # Errors
/// Returns the source's own error when it fails. A pull request with no
/// readable linked issue is [`WorkItem::Missing`], not an error.
pub fn find(
    source: &dyn IssueSource,
    repository: &str,
    pr_body: &str,
    head: &str,
) -> Result<WorkItem, String> {
    let Some(number) = named_issue(repository, pr_body) else {
        let all = candidates(pr_body);
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
            number,
            head: head.to_string(),
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
                number,
                head,
                title,
                body,
            } => serde_json::json!({
                "issue": reference,
                "number": number,
                "head": head,
                "title": title,
                "body": body,
            }),
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

/// Checks that a saved work item belongs to `repository`, `head` and, when
/// `pr_body` is given, the issue that body names.
///
/// # Errors
/// Names the mismatch: a saved item with no number or head, a head or issue
/// that does not match, or text that is not the saved JSON form.
pub fn check_binding(
    saved_json: &str,
    repository: &str,
    head: &str,
    pr_body: Option<&str>,
) -> Result<(), String> {
    let value: serde_json::Value = serde_json::from_str(saved_json)
        .map_err(|e| format!("the saved work item is not valid JSON: {e}"))?;
    if value.get("missing").is_some_and(|m| !m.is_null()) {
        // A pull request with no readable issue has nothing to bind.
        return Ok(());
    }
    let number = value.get("number").and_then(serde_json::Value::as_u64);
    let saved_head = value.get("head").and_then(serde_json::Value::as_str);
    let (Some(number), Some(saved_head)) = (number, saved_head) else {
        return Err("the saved work item is not bound to a commit and an issue".to_string());
    };
    if !saved_head.eq_ignore_ascii_case(head) {
        return Err(format!(
            "the saved work item is for commit {saved_head}, not {head}"
        ));
    }
    if let Some(body) = pr_body {
        match named_issue(repository, body) {
            Some(named) if named == number => {}
            Some(named) => {
                return Err(format!(
                    "the saved work item names issue #{number}, but the pull request text names #{named}"
                ));
            }
            None => {
                return Err(format!(
                    "the saved work item names issue #{number}, but the pull request text names no issue"
                ));
            }
        }
    }
    Ok(())
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
    const HEAD: &str = "0123456789abcdef0123456789abcdef01234567";

    /// `body` with every LF rewritten as the CRLF a web editor saves.
    fn crlf(body: &str) -> String {
        body.replace('\n', "\r\n")
    }

    /// The candidates for `body` as written and for its CRLF form, which must match.
    fn candidates_both(body: &str) -> Vec<Reference> {
        let as_written = candidates(body);
        let as_crlf = candidates(&crlf(body));
        assert_eq!(as_written, as_crlf, "{body}");
        as_written
    }

    #[test]
    fn the_issue_line_names_the_work_item() {
        let body = "Issue: [open-software-factory/software-factory#137 (Build the check)](https://github.com/open-software-factory/software-factory/issues/137)\n\n## What and why\nText.";
        assert_eq!(candidates_both(body), vec![(Some(REPO.to_string()), 137)]);
    }

    #[test]
    fn an_issue_line_with_a_bare_number_or_a_link_alone_is_read() {
        assert_eq!(candidates_both("Issue: #12"), vec![(None, 12)]);
        assert_eq!(
            candidates_both(
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
        assert_eq!(candidates_both("Closes #44 and more"), vec![(None, 44)]);
        assert_eq!(
            candidates_both("This fixes: open-software-factory/software-factory#5."),
            vec![(
                Some("open-software-factory/software-factory".to_string()),
                5
            )]
        );
        assert_eq!(candidates_both("Resolved #7"), vec![(None, 7)]);
    }

    #[test]
    fn the_issue_line_comes_before_a_closing_keyword() {
        let found = candidates_both("Closes #44\n\nIssue: #12");
        assert_eq!(found, vec![(None, 12), (None, 44)]);
    }

    #[test]
    fn a_number_in_prose_with_no_keyword_names_nothing() {
        assert!(candidates_both(
            "See #5 and also open-software-factory/software-factory#6 for background."
        )
        .is_empty());
        assert!(candidates_both("An entity &#123; is not a reference.").is_empty());
    }

    #[test]
    fn the_issue_the_pull_request_names_is_fetched_from_its_own_repository() {
        let source = Fake::issue("## Done when\n- it works");
        let item = find(&source, REPO, "Issue: #137\n", HEAD).expect("finds");
        assert_eq!(
            item,
            WorkItem::Found {
                reference: format!("{REPO}#137"),
                number: 137,
                head: HEAD.to_string(),
                title: "Build the check".to_string(),
                body: "## Done when\n- it works".to_string(),
            }
        );
        assert_eq!(*source.asked.borrow(), vec![(REPO.to_string(), 137)]);
    }

    #[test]
    fn with_no_linked_issue_the_reason_says_to_link_one() {
        let source = Fake::issue("unused");
        let item = find(&source, REPO, "A body that names no issue.", HEAD).expect("finds");
        let WorkItem::Missing(reason) = item else {
            panic!("expected Missing");
        };
        assert!(reason.contains("Link one"), "{reason}");
        assert!(source.asked.borrow().is_empty(), "nothing was fetched");
    }

    #[test]
    fn an_issue_in_another_repository_is_not_read() {
        let source = Fake::issue("unused");
        let item = find(
            &source,
            REPO,
            "Issue: open-software-factory/other-repo#3",
            HEAD,
        )
        .expect("finds");
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
            let item = find(&source, REPO, "Issue: #3", HEAD).expect("finds");
            let WorkItem::Missing(reason) = item else {
                panic!("expected Missing");
            };
            assert!(reason.contains(expected), "{reason}");
        }
    }

    #[test]
    fn a_failing_source_stops_instead_of_guessing() {
        let source = Fake::answering(Err("network down".to_string()));
        let e = find(&source, REPO, "Issue: #3", HEAD).expect_err("stops");
        assert!(e.contains("network down"), "{e}");
    }

    #[test]
    fn a_very_long_body_is_cut_with_a_note() {
        let source = Fake::issue(&"x".repeat(MAX_BODY_CHARS + 10));
        let WorkItem::Found { body, .. } = find(&source, REPO, "Issue: #3", HEAD).expect("finds")
        else {
            panic!("expected Found");
        };
        assert!(body.contains("[cut: the issue body is longer"), "{body}");
        assert!(body.chars().count() < MAX_BODY_CHARS + 100);
    }

    #[test]
    fn the_saved_form_reads_back_as_the_work_item_text_or_the_reason() {
        let found = WorkItem::Found {
            reference: "open-software-factory/software-factory#3".to_string(),
            number: 3,
            head: HEAD.to_string(),
            title: "A title".to_string(),
            body: "## Done when\n- yes".to_string(),
        };
        let json = found.to_json();
        assert!(json.contains("\"number\":3"), "{json}");
        assert!(json.contains(&format!("\"head\":\"{HEAD}\"")), "{json}");
        let text = read_saved(&json).expect("reads");
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

    #[test]
    fn named_issue_is_the_one_the_body_names_for_the_repository() {
        assert_eq!(named_issue(REPO, "Closes #44"), Some(44));
        assert_eq!(named_issue(REPO, "Issue: other-repo#3"), None);
        assert!(named_issue(REPO, "Only prose, no issue named.").is_none());
    }

    #[test]
    fn find_records_the_issue_number_and_the_head_commit() {
        let source = Fake::issue("unused");
        let WorkItem::Found { number, head, .. } =
            find(&source, REPO, "Issue: #137", HEAD).expect("finds")
        else {
            panic!("expected Found");
        };
        assert_eq!(number, 137);
        assert_eq!(head, HEAD);
    }

    #[test]
    fn check_binding_accepts_a_saved_work_item_that_matches() {
        let json = WorkItem::Found {
            reference: format!("{REPO}#137"),
            number: 137,
            head: HEAD.to_string(),
            title: "A title".to_string(),
            body: "body".to_string(),
        }
        .to_json();
        assert!(check_binding(&json, REPO, HEAD, Some("Issue: #137")).is_ok());
        assert!(check_binding(&json, REPO, HEAD, None).is_ok());
    }

    #[test]
    fn check_binding_refuses_a_saved_work_item_for_another_commit() {
        let json = WorkItem::Found {
            reference: format!("{REPO}#137"),
            number: 137,
            head: HEAD.to_string(),
            title: "A title".to_string(),
            body: "body".to_string(),
        }
        .to_json();
        let other = "ffffffffffffffffffffffffffffffffffffffff";
        let e = check_binding(&json, REPO, other, Some("Issue: #137")).expect_err("refused");
        assert!(e.contains(HEAD) && e.contains(other), "{e}");
    }

    #[test]
    fn check_binding_refuses_a_saved_work_item_for_another_issue() {
        let json = WorkItem::Found {
            reference: format!("{REPO}#137"),
            number: 137,
            head: HEAD.to_string(),
            title: "A title".to_string(),
            body: "body".to_string(),
        }
        .to_json();
        let e = check_binding(&json, REPO, HEAD, Some("Issue: #44")).expect_err("refused");
        assert!(e.contains("#137") && e.contains("#44"), "{e}");
    }

    #[test]
    fn check_binding_refuses_a_saved_work_item_with_no_number_or_head() {
        let json = serde_json::json!({
            "issue": format!("{REPO}#137"),
            "title": "A title",
            "body": "body",
        })
        .to_string();
        let e = check_binding(&json, REPO, HEAD, Some("Issue: #137")).expect_err("refused");
        assert!(e.contains("not bound"), "{e}");
    }

    #[test]
    fn check_binding_passes_a_missing_work_item() {
        let json = WorkItem::Missing("none linked".to_string()).to_json();
        assert!(check_binding(&json, REPO, HEAD, Some("Only prose.")).is_ok());
    }

    #[test]
    fn an_issue_line_in_a_fenced_block_is_hidden_and_a_visible_closing_keyword_wins() {
        let body = "```\nIssue: #123\n```\n\nCloses #44";
        assert_eq!(candidates_both(body), vec![(None, 44)]);
    }

    #[test]
    fn a_crlf_fenced_block_hides_an_issue_line_and_the_closing_keyword_wins() {
        let body = "```\r\nIssue: #1\r\n```\r\nCloses #44";
        assert_eq!(candidates(body), vec![(None, 44)]);
    }

    #[test]
    fn a_lone_carriage_return_is_a_line_ending() {
        let body = "```\rIssue: #1\r```\rCloses #44";
        assert_eq!(candidates(body), vec![(None, 44)]);
    }

    #[test]
    fn a_tilde_fenced_block_hides_an_issue_line() {
        let body = "~~~\nIssue: #123\n~~~";
        assert!(candidates_both(body).is_empty());
    }

    #[test]
    fn a_backtick_line_with_more_backticks_is_an_inline_span_not_a_fence() {
        assert_eq!(
            candidates_both("```x``` note\n\nCloses #44"),
            vec![(None, 44)]
        );
    }

    #[test]
    fn a_fence_after_a_bullet_list_marker_hides_an_issue_line() {
        assert_eq!(
            candidates_both("- ```\n  Issue: #1\n  ```\nCloses #44"),
            vec![(None, 44)]
        );
    }

    #[test]
    fn a_fence_after_a_numbered_list_marker_hides_an_issue_line() {
        assert_eq!(
            candidates_both("1. ```\n   Issue: #1\n   ```\nCloses #44"),
            vec![(None, 44)]
        );
    }

    #[test]
    fn every_list_marker_form_opens_a_fence() {
        for body in [
            "* ```\n  Issue: #1\n  ```\nCloses #44",
            "+ ```\n  Issue: #1\n  ```\nCloses #44",
            "1) ```\n   Issue: #1\n   ```\nCloses #44",
        ] {
            assert_eq!(candidates_both(body), vec![(None, 44)], "{body}");
        }
    }

    #[test]
    fn a_list_fence_closes_without_indent() {
        assert_eq!(
            candidates_both("- ```\n  Issue: #1\n```\nCloses #44"),
            vec![(None, 44)]
        );
    }

    #[test]
    fn a_list_fence_with_an_info_string_hides_an_issue_line() {
        assert_eq!(
            candidates_both("- ```text\n  Issue: #1\n  ```\nCloses #44"),
            vec![(None, 44)]
        );
    }

    #[test]
    fn inline_backticks_on_a_list_item_are_not_a_fence() {
        assert_eq!(
            candidates_both("- ```x``` note\n\nCloses #44"),
            vec![(None, 44)]
        );
    }

    #[test]
    fn a_plain_list_item_still_names_the_issue() {
        assert_eq!(candidates_both("- Issue: #12"), vec![(None, 12)]);
    }

    #[test]
    fn an_unclosed_fenced_block_hides_every_line_after_it() {
        let body = "Closes #44\n\n```\nIssue: #123\nCloses #99";
        assert_eq!(candidates_both(body), vec![(None, 44)]);
    }

    #[test]
    fn an_indented_code_block_hides_an_issue_line() {
        let body = "    Issue: #123\n\nCloses #44";
        assert_eq!(candidates_both(body), vec![(None, 44)]);
    }

    #[test]
    fn an_inline_code_span_hides_a_closing_keyword() {
        assert!(candidates_both("Run `Closes #5` now.").is_empty());
    }

    #[test]
    fn a_link_title_is_hidden_from_the_closing_keyword_scan() {
        assert_eq!(
            candidates_both("[a](https://example.com \"Closes #99\")\n\nCloses #44"),
            vec![(None, 44)]
        );
    }

    #[test]
    fn a_link_reference_definition_title_is_hidden() {
        assert_eq!(
            candidates_both("[x]: https://example.com \"Closes #99\"\n\nCloses #44"),
            vec![(None, 44)]
        );
    }

    #[test]
    fn an_image_alt_text_is_hidden() {
        assert_eq!(
            candidates_both("![Closes #99](u)\n\nCloses #44"),
            vec![(None, 44)]
        );
    }

    #[test]
    fn an_html_attribute_is_hidden() {
        assert_eq!(
            candidates_both("<a title=\"Closes #99\">\n\nCloses #44"),
            vec![(None, 44)]
        );
    }

    #[test]
    fn visible_link_text_still_names_the_issue() {
        assert_eq!(candidates_both("[Closes #5](u)"), vec![(None, 5)]);
    }

    #[test]
    fn an_autolink_with_a_scheme_is_not_an_html_tag() {
        assert_eq!(
            candidates_both(
                "Closes <https://github.com/open-software-factory/software-factory/issues/7>"
            ),
            vec![(Some(REPO.to_string()), 7)]
        );
    }

    #[test]
    fn a_single_quoted_or_parenthesized_title_is_hidden() {
        for body in [
            "[a](https://example.com 'Closes #99')\n\nCloses #44",
            "[a](https://example.com (Closes #99))\n\nCloses #44",
            "[x]: https://example.com 'Closes #99'\n\nCloses #44",
            "[x]: https://example.com (Closes #99)\n\nCloses #44",
        ] {
            assert_eq!(candidates_both(body), vec![(None, 44)], "{body}");
        }
    }

    #[test]
    fn a_reference_definition_title_on_the_next_line_is_hidden() {
        let body = "[x]: https://example.com\n\"Closes #99\"\n\nCloses #44";
        assert_eq!(candidates_both(body), vec![(None, 44)]);
    }

    #[test]
    fn an_html_comment_on_one_line_hides_an_issue_line() {
        assert!(candidates_both("<!-- Issue: #123 -->").is_empty());
    }

    #[test]
    fn an_html_comment_over_several_lines_hides_an_issue_line() {
        assert!(candidates_both("<!--\nIssue: #123\n-->").is_empty());
    }

    #[test]
    fn a_block_quote_line_is_ignored() {
        assert!(candidates_both("> Issue: #123").is_empty());
        assert!(candidates_both("  > Closes #5").is_empty());
    }

    #[test]
    fn a_closing_keyword_after_a_negation_names_nothing() {
        for body in [
            "This does not fix #77",
            "This doesn't close #5",
            "This will not resolve #9",
            "This never fixes #1",
            "This no longer closes #2",
            "This is without fix #3",
            "This without fixing #3",
            "This cannot close #4",
            "This can't fix #6",
        ] {
            assert!(candidates_both(body).is_empty(), "{body}");
        }
    }

    #[test]
    fn a_negation_only_hides_its_own_sentence() {
        assert_eq!(
            candidates_both("This does not fix #77. It closes #44."),
            vec![(None, 44)]
        );
    }

    #[test]
    fn ordinary_prose_still_names_the_issue_line_and_a_closing_keyword() {
        assert_eq!(
            candidates_both("## Why\n\nIssue: #12\n\nCloses #44"),
            vec![(None, 12), (None, 44)]
        );
    }

    #[test]
    fn with_only_hidden_mentions_find_reports_no_issue_linked() {
        let source = Fake::issue("unused");
        for body in [
            "```\nIssue: #123\n```",
            "~~~ Closes #5 ~~~",
            "    Issue: #9",
            "`Closes #5`",
            "<!-- Issue: #123 -->",
            "<!--\nIssue: #123\n-->",
            "> Issue: #123",
            "This does not fix #77",
        ] {
            for fixture in [body.to_string(), crlf(body)] {
                let item = find(&source, REPO, &fixture, HEAD).expect("finds");
                assert_eq!(item, WorkItem::Missing(no_issue_linked(REPO)), "{fixture}");
            }
        }
        assert!(source.asked.borrow().is_empty(), "nothing was fetched");
    }
}
