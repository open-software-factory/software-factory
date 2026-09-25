//! Hook handlers. Each reads one JSON event from standard input and answers
//! in the shape the calling harness expects.
//!
//! # The harnesses do not agree on key names
//!
//! There is no shared schema for a stop event. Each harness spells its keys
//! its own way, and an event carries nothing that names the harness that sent
//! it. Every key this module reads is therefore looked up under each spelling
//! that is known to occur.
//!
//! | Harness | Session key | Assistant text | How it is refused |
//! |---|---|---|---|
//! | Claude Code | `session_id` | `last_assistant_message` | exit code 2 |
//! | Codex | `session_id` | `last_assistant_message` | exit code 2 |
//! | dsh bridge | `session_id` | none: it sends an empty `transcript_path` | exit code 2 |
//! | Copilot CLI | `sessionId` | `transcriptPath` | `{"decision":"block"}` |
//! | omp | `session_id` | supplied by its adapter | `{"decision":"block"}` |
//! | `OpenCode` | `sessionID` | supplied by its adapter | it cannot be refused |
//!
//! The dsh bridge sends no assistant text at all, which is why this project
//! ships its own dsh plugin rather than relying on that bridge.

use crate::config::WritingConfig;
use crate::journal::CheckResult;
use crate::lints::{self, Remediation};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

const MAX_LINES_IN_REASON: usize = 30;

/// How many advice lines one turn may leave for the next prompt. A session
/// that never read its advice once accumulated sixty kilobytes of it.
const MAX_ADVICE_LINES: usize = 20;

/// Printed on every prompt, before any stored advice. One line, so it costs
/// almost nothing, naming the shapes a model falls into most often. The
/// stored advice only covers a slip that already happened; this covers the
/// first message of a session too.
pub const STANDING_REMINDER: &str =
    "osf writing-lint reminder for this reply. State the point and stop. \
    Do not end a sentence with a `, not X` or `, never X` tail. \
    Write a reference as owner/repo#N (what it is). \
    Name a thing by what it is rather than by its place in a list. \
    No sweeps such as `nobody` or `everyone`.";

/// Every spelling of the session key, most common first.
const SESSION_KEYS: &[&str] = &["session_id", "sessionId", "sessionID"];

/// Every spelling of the key naming the turn within a session.
const PROMPT_KEYS: &[&str] = &["prompt_id", "turn_id", "promptId", "turnId"];

/// Camel-case keys that mark an event as coming from a harness that reads its
/// answer as JSON on standard output rather than from the exit code.
const CAMEL_CASE_KEYS: &[&str] = &[
    "sessionId",
    "transcriptPath",
    "lastAssistantMessage",
    "hookEventName",
];

/// How a harness learns that the check refused.
///
/// The two are not interchangeable. A harness reading the exit code ignores
/// standard output, and a harness reading standard output treats exit code 2
/// as the check itself crashing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Answer {
    /// Exit code 2, reason on standard error. Claude Code, Codex, dsh.
    ExitCode,
    /// `{"decision":"block","reason":...}` on standard output. Copilot, omp.
    DecisionJson,
}

/// Which answer an event's sender will understand.
///
/// Nothing in an event names its harness, so the key spelling is the only
/// signal available. A camel-case key means Copilot CLI, the one harness that
/// both spells keys that way and reads its answer from standard output.
///
/// This is a guess, and a caller that knows better overrides it with
/// `--answer`. An adapter that builds the event itself, such as the one for
/// omp, should always pass the flag rather than rely on the spelling.
fn answer_for(event: &Value) -> Answer {
    if CAMEL_CASE_KEYS.iter().any(|k| event.get(k).is_some()) {
        Answer::DecisionJson
    } else {
        Answer::ExitCode
    }
}

/// Reads a coding agent's Stop event from standard input, lints the final
/// message, and refuses the turn on errors.
///
/// Four things can stop this from checking anything at all: standard input
/// cannot be read, the input is not JSON, the event carries no assistant
/// message, or the known-names list cannot be loaded. Each of those is a
/// failure to run, not a clean message, and
/// [0003](../../../docs/architecture/decisions/0003-deterministic-verification-is-authoritative.md)
/// requires that a failure to run never reads as a pass. So every one of
/// them refuses the turn too, worded to say the check itself did not run,
/// through the same [`refuse`] a genuine finding uses.
pub fn stop(
    known_names: Option<&Path>,
    max_bounces: u32,
    cfg: &WritingConfig,
    answer: Option<Answer>,
) -> ExitCode {
    let mut raw = String::new();
    let read = std::io::stdin().read_to_string(&mut raw).map(|_| raw);
    stop_with_input(read, known_names, max_bounces, cfg, answer)
}

/// The body of [`stop`], taking the already-attempted standard input read
/// as a parameter instead of performing it, so every failure-to-run path
/// can be driven by a test without a real process or a real stream.
fn stop_with_input(
    raw: std::io::Result<String>,
    known_names: Option<&Path>,
    max_bounces: u32,
    cfg: &WritingConfig,
    answer: Option<Answer>,
) -> ExitCode {
    let raw = match raw {
        Ok(r) => r,
        Err(e) => {
            return refuse_could_not_run(
                answer.unwrap_or(Answer::ExitCode),
                "unknown",
                "",
                max_bounces,
                &format!("cannot read standard input: {e}"),
            );
        }
    };
    let event: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            return refuse_could_not_run(
                answer.unwrap_or(Answer::ExitCode),
                "unknown",
                "",
                max_bounces,
                &format!("input is not JSON: {e}"),
            );
        }
    };
    let answer = answer.unwrap_or_else(|| answer_for(&event));
    let session = string_at(&event, SESSION_KEYS).unwrap_or_else(|| "unknown".to_string());
    let prompt = string_at(&event, PROMPT_KEYS).unwrap_or_default();

    let Some(text) = message_text(&event) else {
        return refuse_could_not_run(
            answer,
            &session,
            &prompt,
            max_bounces,
            "no assistant message in the event",
        );
    };
    let known = match lints::load_known_names(&cfg.known_names, known_names) {
        Ok(k) => k,
        Err(e) => {
            return refuse_could_not_run(answer, &session, &prompt, max_bounces, &e);
        }
    };
    let findings = checked_findings(&text, &known, cfg);

    let advise: Vec<&lints::Finding> = findings
        .iter()
        .filter(|f| f.remediation == Remediation::Advise)
        .collect();
    if !advise.is_empty() {
        store_advice(&session, &advise);
    }

    let counter = counter_path(&session, &prompt);
    let Some((blocking, verb, instruction)) = blocking_set(&findings) else {
        let _ = std::fs::remove_file(&counter);
        return ExitCode::SUCCESS;
    };

    let bounces = read_counter(&counter);
    if bounces >= max_bounces {
        eprintln!(
            "osf hook stop: {} {verb}(s) remain after {bounces} attempt(s); letting the message through",
            blocking.len()
        );
        let _ = std::fs::remove_file(&counter);
        return ExitCode::SUCCESS;
    }
    write_counter(&counter, bounces + 1);

    let reason = build_reason(&blocking, verb, instruction, bounces + 1, max_bounces);
    refuse(answer, &reason)
}

/// How a harness learns that the stop check refused the turn, whichever
/// reason it refused for. A finding worth blocking and a check that could
/// not run at all both answer through here, so neither path invents a
/// second way to refuse.
fn refuse(answer: Answer, reason: &str) -> ExitCode {
    match answer {
        Answer::ExitCode => {
            eprintln!("{reason}");
            ExitCode::from(2)
        }
        Answer::DecisionJson => {
            println!(
                "{}",
                serde_json::json!({ "decision": "block", "reason": reason })
            );
            ExitCode::SUCCESS
        }
    }
}

/// Refuses the turn because the writing check could not run at all: no
/// input, no JSON, no assistant message, or no known-names list to check
/// against. Worded to say plainly that the check did not run, unlike
/// [`build_reason`]'s wording for a check that ran and found errors, since
/// a reader needs to tell the two apart.
///
/// A harness that retries a refusal could bounce forever against a defect
/// that never clears, such as a known-names file that stays unreadable.
/// This is counted against the same `max_bounces` budget, the same way a
/// genuine finding is: after that many attempts the turn is let through,
/// but loudly, on standard error, saying it went through unchecked rather
/// than saying it passed. `session` and `prompt` fall back to fixed keys
/// when the event could not be read far enough to name either.
fn refuse_could_not_run(
    answer: Answer,
    session: &str,
    prompt: &str,
    max_bounces: u32,
    detail: &str,
) -> ExitCode {
    let counter = counter_path(session, prompt);
    let bounces = read_counter(&counter);
    if bounces >= max_bounces {
        eprintln!(
            "osf hook stop: the writing check could not run after {bounces} attempt(s) ({detail}); letting the message through unchecked"
        );
        let _ = std::fs::remove_file(&counter);
        return ExitCode::SUCCESS;
    }
    write_counter(&counter, bounces + 1);
    let reason = format!(
        "osf hook stop: the writing check could not run, so the turn is refused rather than treated as a pass (attempt {} of {max_bounces}): {detail}",
        bounces + 1
    );
    refuse(answer, &reason)
}

/// Lints `text` as a transcript, and applies the config's level overrides,
/// dropping every suppressed finding. The stop check runs on every turn
/// end, so it stays on the fast tier only.
fn checked_findings(
    text: &str,
    known: &lints::KnownNames,
    cfg: &WritingConfig,
) -> Vec<lints::Finding> {
    let findings =
        lints::writing::lint_writing(text, known, cfg, lints::Context::Transcript, true, false);
    osf_lint_core::apply_level_overrides(findings, &cfg.levels)
        .into_iter()
        .filter(|f| f.suppressed.is_none())
        .collect()
}

/// The findings that block the stop, the opening line, and the
/// instruction to give. `None` when nothing blocks.
///
/// A message checked here has already been sent, so a rewrite corrects
/// nothing: it puts a second copy under the first. [`resolve`] therefore
/// never produces a rewrite for a transcript, and this asks for an
/// addition instead. A rewrite finding is still handled, because this
/// function takes findings from anywhere, but it says the same thing: add
/// to what you sent, do not send it again.
///
/// [`resolve`]: osf_lint_core::resolve
fn blocking_set(
    findings: &[lints::Finding],
) -> Option<(Vec<&lints::Finding>, &'static str, &'static str)> {
    const OPENING: &str = "your message has been sent and cannot be changed";
    const INSTRUCTION: &str = "Do not send that message again. Reply with a short follow-up \
        that answers only the point(s) below, one sentence each, and nothing else.";

    let blocking: Vec<&lints::Finding> = findings
        .iter()
        .filter(|f| matches!(f.remediation, Remediation::Rewrite | Remediation::Clarify))
        .collect();
    if blocking.is_empty() {
        return None;
    }
    Some((blocking, OPENING, INSTRUCTION))
}

fn build_reason(
    blocking: &[&lints::Finding],
    opening: &str,
    instruction: &str,
    attempt: u32,
    max_bounces: u32,
) -> String {
    let mut lines: Vec<String> = blocking
        .iter()
        .take(MAX_LINES_IN_REASON)
        .map(|f| f.render("message", f.level))
        .collect();
    if blocking.len() > MAX_LINES_IN_REASON {
        lines.push(format!(
            "...and {} more",
            blocking.len() - MAX_LINES_IN_REASON
        ));
    }
    format!(
        "osf writing-lint: {opening}. {} point(s) need a follow-up (attempt {attempt} of {max_bounces}). {instruction}\n{}",
        blocking.len(),
        lines.join("\n")
    )
}

/// Reads a prompt-submitted hook payload from standard input and prints the
/// context for the new turn: the standing reminder, then any advice stored
/// for that session, which it clears. This is how an `advise` finding from
/// the previous turn's stop check reaches the agent without costing a
/// rewrite. The reminder prints whatever the payload holds, so a broken
/// payload still gets it.
pub fn prompt() -> ExitCode {
    let mut raw = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut raw) {
        eprintln!("osf hook prompt: cannot read standard input: {e}");
        println!("{STANDING_REMINDER}");
        return ExitCode::SUCCESS;
    }
    let event: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("osf hook prompt: input is not JSON: {e}");
            println!("{STANDING_REMINDER}");
            return ExitCode::SUCCESS;
        }
    };
    let session =
        string_at(&event, &["session_id", "sessionId"]).unwrap_or_else(|| "unknown".to_string());
    println!("{}", prompt_text(&session));
    ExitCode::SUCCESS
}

/// The reminder, followed by the session's stored advice when there is any.
fn prompt_text(session: &str) -> String {
    match take_advice(session) {
        Some(advice) => format!(
            "{STANDING_REMINDER}\nosf writing-lint has style advice from your last turn, worth a look this time:\n{advice}"
        ),
        None => STANDING_REMINDER.to_string(),
    }
}

/// Reads and clears the session's advice file, returning its trimmed
/// content when it holds anything.
fn take_advice(session: &str) -> Option<String> {
    let path = advice_path(session);
    let advice = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let trimmed = advice.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The keys a `PostToolUse` event's `tool_input` names the written path
/// under, checked in this order.
const WRITTEN_PATH_KEYS: &[&str] = &["file_path", "path", "notebook_path"];

/// Runs the hook checkpoint on the file a `PostToolUse` event names, and
/// refuses the tool call through [`refuse`] on an error finding.
pub fn post_tool(timeout: Duration, answer: Option<Answer>) -> ExitCode {
    let mut raw = String::new();
    let read = std::io::stdin().read_to_string(&mut raw).map(|_| raw);
    post_tool_with_input(read, timeout, answer)
}

/// Says the hook checkpoint itself could not run, as `refuse_could_not_run`
/// does for `stop` in this same file. A finding worth blocking and a check
/// that could not run at all must not read the same either way, so this
/// never claims a pass.
fn refuse_post_tool_could_not_run(answer: Answer, detail: &str) -> ExitCode {
    refuse(
        answer,
        &format!(
            "osf hook post-tool: the hook checkpoint could not run, so the write is refused \
             rather than treated as a pass: {detail}"
        ),
    )
}

/// The body of [`post_tool`], taking the standard input read as a
/// parameter so every path can be driven by a test.
fn post_tool_with_input(
    raw: std::io::Result<String>,
    timeout: Duration,
    answer: Option<Answer>,
) -> ExitCode {
    let raw = match raw {
        Ok(r) => r,
        Err(e) => {
            return refuse_post_tool_could_not_run(
                answer.unwrap_or(Answer::ExitCode),
                &format!("cannot read standard input: {e}"),
            );
        }
    };
    let event: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            return refuse_post_tool_could_not_run(
                answer.unwrap_or(Answer::ExitCode),
                &format!("input is not JSON: {e}"),
            );
        }
    };
    let Some(raw_path) = written_path(&event) else {
        return ExitCode::SUCCESS;
    };
    let answer = answer.unwrap_or_else(|| answer_for(&event));
    let absolute_path = absolute_written_path(&raw_path);
    let anchor = written_file_parent(&absolute_path);
    let root = match crate::checkpoint::detect_adoption(&anchor) {
        crate::checkpoint::Adoption::Adopted(root) => root,
        crate::checkpoint::Adoption::NotAdopted(path, reason) => {
            eprintln!(
                "osf hook post-tool: not adopted at {}: {reason}",
                path.display()
            );
            return ExitCode::SUCCESS;
        }
        crate::checkpoint::Adoption::AdoptedButBroken(root, missing) => {
            return refuse_post_tool_could_not_run(
                answer,
                &format!(
                    "osf.toml at {} asks for osf, but {missing} is missing",
                    root.display()
                ),
            );
        }
        crate::checkpoint::Adoption::CouldNotRun(reason) => {
            return refuse_post_tool_could_not_run(answer, &reason);
        }
    };
    let Some(rel) = repo_relative(&root, &absolute_path) else {
        eprintln!(
            "osf hook post-tool: {raw_path} is outside the repository root {}; nothing to check",
            root.display()
        );
        return ExitCode::SUCCESS;
    };
    let state_dir = match crate::journal::state_dir() {
        Ok(d) => d,
        Err(e) => {
            return refuse_post_tool_could_not_run(answer, &e);
        }
    };
    let req = crate::checkpoint::Request {
        root: &root,
        checkpoint: crate::checkpoint::Checkpoint::Hook,
        base: None,
        files: Some(vec![rel]),
        timeout: Some(timeout),
    };
    let summary = crate::checkpoint::run(&req, &state_dir);
    if let Some(err) = &summary.journal_error {
        eprintln!("osf hook post-tool: {err}");
    }
    match summary.result {
        CheckResult::Skipped => {
            eprintln!(
                "osf hook post-tool: skipped (hook time limit {}s)",
                timeout.as_secs()
            );
            ExitCode::SUCCESS
        }
        CheckResult::Failed | CheckResult::CouldNotRun => {
            let mut lines = vec![format!(
                "osf hook post-tool: the written file did not pass the hook checkpoint in {}",
                root.display()
            )];
            // The journal error, if any, was already printed above; do not
            // let the refusal body repeat the same line.
            lines.extend(
                summary
                    .error_findings
                    .iter()
                    .filter(|f| Some(f.as_str()) != summary.journal_error.as_deref())
                    .cloned(),
            );
            refuse(answer, &lines.join("\n"))
        }
        CheckResult::Passed | CheckResult::NothingToCheck => ExitCode::SUCCESS,
    }
}

/// The written path an event names, from the first of [`WRITTEN_PATH_KEYS`]
/// its `tool_input` carries.
fn written_path(event: &Value) -> Option<String> {
    let input = event.get("tool_input")?;
    WRITTEN_PATH_KEYS
        .iter()
        .find_map(|k| input.get(k).and_then(Value::as_str).map(str::to_string))
}

/// `raw` resolved once to an absolute, lexically normalised path, against
/// the current directory when it is not already absolute. Both the
/// adoption anchor and the repository-relative path are derived from this
/// one value, so they can never disagree on where the file actually is.
fn absolute_written_path(raw: &str) -> PathBuf {
    let normalized = raw.replace('\\', "/");
    let candidate = PathBuf::from(&normalized);
    let absolute = if candidate.is_absolute() {
        candidate
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(&candidate)
    };
    lexically_normalize(&absolute)
}

/// The written path's own parent directory: the adoption anchor is the repository holding the file, not the process's working directory.
fn written_file_parent(absolute: &Path) -> PathBuf {
    absolute
        .parent()
        .map_or_else(|| absolute.to_path_buf(), Path::to_path_buf)
}

/// `absolute` made repository-relative to `root`, with forward slashes.
/// `None` when it leaves `root` entirely: a path outside the repository
/// the adoption check found must never read back as one inside it.
fn repo_relative(root: &Path, absolute: &Path) -> Option<String> {
    if let Ok(rel) = absolute.strip_prefix(root) {
        return Some(rel.to_string_lossy().replace('\\', "/"));
    }
    let root_canon = std::fs::canonicalize(root).ok()?;
    let candidate_canon = std::fs::canonicalize(absolute).ok()?;
    let rel = candidate_canon.strip_prefix(&root_canon).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}

/// `path`'s `.` and `..` components collapsed left to right, without
/// touching the filesystem: a containment check must work even when
/// nothing exists at `path` yet, such as a rejected `../outside.md`.
fn lexically_normalize(path: &Path) -> PathBuf {
    let mut out: Vec<std::path::Component> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out.into_iter().collect()
}

fn string_at(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| v.get(k).and_then(Value::as_str).map(str::to_string))
}

fn message_text(event: &Value) -> Option<String> {
    if let Some(t) = string_at(event, &["last_assistant_message", "lastAssistantMessage"]) {
        return Some(t);
    }
    let path = string_at(event, &["transcriptPath", "transcript_path"])?;
    let body = std::fs::read_to_string(&path).ok()?;
    last_assistant_text(&body)
}

/// Last assistant text in a JSON-lines transcript, across the common shapes.
fn last_assistant_text(body: &str) -> Option<String> {
    body.lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find_map(|v| {
            let msg = v.get("message").unwrap_or(&v);
            let role = msg
                .get("role")
                .or_else(|| v.get("role"))
                .and_then(Value::as_str)?;
            if role != "assistant" {
                return None;
            }
            let content = msg.get("content").or_else(|| msg.get("text"))?;
            match content {
                Value::String(s) => Some(s.clone()),
                Value::Array(parts) => {
                    let texts: Vec<&str> = parts
                        .iter()
                        .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
                        .filter_map(|p| p.get("text").and_then(Value::as_str))
                        .collect();
                    (!texts.is_empty()).then(|| texts.join("\n"))
                }
                _ => None,
            }
        })
}

/// Strips a session or prompt id down to characters safe for a file name.
fn safe_id(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

fn counter_path(session: &str, prompt: &str) -> PathBuf {
    let base = std::env::temp_dir(); // osf: temp-dir allowed, shared across hook invocations
    let dir = base.join("osf-stop");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{}-{}", safe_id(session), safe_id(prompt)))
}

fn read_counter(p: &Path) -> u32 {
    std::fs::read_to_string(p)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn write_counter(p: &Path, n: u32) {
    let _ = std::fs::write(p, n.to_string());
}

/// Where an `advise` finding waits for the next turn's prompt hook,
/// keyed by session id, under the system temporary directory.
fn advice_path(session: &str) -> PathBuf {
    let base = std::env::temp_dir(); // osf: temp-dir allowed, shared across hook invocations
    let dir = base.join("osf-advice");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{}.txt", safe_id(session)))
}

/// Writes the advise findings of this turn to the session's advice file,
/// replacing whatever the last turn left, for `osf hook prompt` to deliver
/// on the next turn. Advice is about the message just sent, so an older
/// turn's lines are stale by the time anyone reads them. Duplicate lines
/// collapse to one, and the file holds at most [`MAX_ADVICE_LINES`].
fn store_advice(session: &str, findings: &[&lints::Finding]) {
    let mut lines: Vec<String> = Vec::new();
    for f in findings {
        let line = f.render("message", f.level);
        if !lines.contains(&line) {
            lines.push(line);
        }
    }
    let total = lines.len();
    lines.truncate(MAX_ADVICE_LINES);
    if total > MAX_ADVICE_LINES {
        lines.push(format!("...and {} more", total - MAX_ADVICE_LINES));
    }
    let path = advice_path(session);
    let _ = std::fs::write(&path, lines.join("\n") + "\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_claude_transcript_shape() {
        let t = r#"{"type":"user","message":{"role":"user","content":"hi"}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Done."}]}}
"#;
        assert_eq!(last_assistant_text(t).as_deref(), Some("Done."));
    }

    #[test]
    fn reads_flat_transcript_shape() {
        let t = "{\"role\":\"assistant\",\"content\":\"One.\"}\n{\"role\":\"user\",\"content\":\"x\"}\n";
        assert_eq!(last_assistant_text(t).as_deref(), Some("One."));
    }

    /// One real event per harness. The point of the table is that no two
    /// senders agree, so a change that assumes one spelling fails here.
    fn events() -> Vec<(&'static str, Value, Answer, &'static str)> {
        vec![
            (
                "claude code",
                serde_json::json!({
                    "session_id": "s-claude",
                    "transcript_path": "/tmp/t.jsonl",
                    "hook_event_name": "Stop",
                    "stop_hook_active": false,
                    "last_assistant_message": "hello"
                }),
                Answer::ExitCode,
                "s-claude",
            ),
            (
                "codex",
                serde_json::json!({
                    "session_id": "s-codex",
                    "hook_event_name": "Stop",
                    "last_assistant_message": "hello"
                }),
                Answer::ExitCode,
                "s-codex",
            ),
            (
                "dsh bridge",
                serde_json::json!({
                    "session_id": "s-dsh",
                    "transcript_path": "",
                    "cwd": "/w",
                    "hook_event_name": "Stop"
                }),
                Answer::ExitCode,
                "s-dsh",
            ),
            (
                "copilot cli",
                serde_json::json!({
                    "sessionId": "s-copilot",
                    "transcriptPath": "/tmp/t.jsonl",
                    "hookEventName": "agentStop"
                }),
                Answer::DecisionJson,
                "s-copilot",
            ),
            (
                "opencode",
                serde_json::json!({
                    "sessionID": "s-opencode",
                    "last_assistant_message": "hello"
                }),
                Answer::ExitCode,
                "s-opencode",
            ),
        ]
    }

    #[test]
    fn every_harness_session_key_is_read() {
        for (harness, event, _, session) in events() {
            assert_eq!(
                string_at(&event, SESSION_KEYS).as_deref(),
                Some(session),
                "{harness}"
            );
        }
    }

    /// The answer shape is guessed from the key spelling, so the guess is
    /// pinned here. Reading the exit code and reading standard output are not
    /// interchangeable: each harness ignores the other's answer.
    #[test]
    fn the_answer_shape_matches_the_harness() {
        for (harness, event, want, _) in events() {
            assert_eq!(answer_for(&event), want, "{harness}");
        }
    }

    /// A camel-case key other than the session id must still pick the JSON
    /// answer. Keying this off `sessionId` alone was the earlier bug.
    #[test]
    fn any_camel_case_key_picks_the_json_answer() {
        for key in [
            "sessionId",
            "transcriptPath",
            "lastAssistantMessage",
            "hookEventName",
        ] {
            let event = serde_json::json!({ key: "x" });
            assert_eq!(answer_for(&event), Answer::DecisionJson, "{key}");
        }
    }

    /// An event nothing recognises still gets an answer, and it is the one
    /// most harnesses read.
    #[test]
    fn an_unknown_event_falls_back_to_the_exit_code() {
        assert_eq!(answer_for(&serde_json::json!({})), Answer::ExitCode);
        assert_eq!(string_at(&serde_json::json!({}), SESSION_KEYS), None);
    }

    #[test]
    fn every_spelling_of_the_turn_key_is_read() {
        for key in PROMPT_KEYS {
            let event = serde_json::json!({ *key: "t-1" });
            assert_eq!(
                string_at(&event, PROMPT_KEYS).as_deref(),
                Some("t-1"),
                "{key}"
            );
        }
    }

    fn finding_with(remediation: Remediation) -> lints::Finding {
        let mut f = lints::Finding::new(
            "probe-rule",
            lints::Level::Warning,
            1,
            "m".to_string(),
            "x".to_string(),
        );
        f.remediation = remediation;
        f
    }

    /// Change 3: an advise finding must not block the stop hook.
    #[test]
    fn an_advise_only_batch_does_not_block() {
        let findings = vec![
            finding_with(Remediation::Advise),
            finding_with(Remediation::Advise),
        ];
        assert!(blocking_set(&findings).is_none());
    }

    /// The message is already sent, so the only correction it can take is
    /// an addition. Nothing here may ask for it to be sent again.
    #[test]
    fn a_clarify_finding_asks_for_a_follow_up_not_another_copy() {
        let findings = vec![finding_with(Remediation::Clarify)];
        let (blocking, opening, instruction) = blocking_set(&findings).expect("clarify blocks");
        assert_eq!(blocking.len(), 1);
        assert!(opening.contains("cannot be changed"), "{opening}");
        assert!(
            instruction.contains("Do not send that message again"),
            "{instruction}"
        );
        assert!(
            !instruction.to_lowercase().contains("rewrite"),
            "a sent message cannot be rewritten: {instruction}"
        );
    }

    /// A rewrite finding cannot reach this from a transcript, since
    /// `resolve` never produces one there. If it arrives from elsewhere it
    /// gets the same answer: add to what was sent.
    #[test]
    fn a_rewrite_finding_is_answered_the_same_way_as_a_clarify() {
        let findings = vec![
            finding_with(Remediation::Rewrite),
            finding_with(Remediation::Clarify),
        ];
        let (blocking, _, instruction) = blocking_set(&findings).expect("rewrite blocks");
        assert_eq!(blocking.len(), 2);
        assert!(
            instruction.contains("Do not send that message again"),
            "{instruction}"
        );
    }

    /// Change 3: an advise finding's advice survives to `osf hook prompt`.
    #[test]
    fn advice_survives_to_the_next_prompt() {
        let session = "test-session-advice-survives";
        let _ = std::fs::remove_file(advice_path(session));
        let finding = finding_with(Remediation::Advise);
        store_advice(session, &[&finding]);
        let advice = take_advice(session).expect("advice was stored");
        assert!(advice.contains("probe-rule"), "{advice}");
        assert!(
            take_advice(session).is_none(),
            "advice is cleared once read"
        );
    }

    /// A session whose prompt hook was never wired once piled up sixty
    /// kilobytes of advice. Each turn now replaces the last turn's lines.
    #[test]
    fn advice_holds_the_last_turn_only() {
        let session = "test-session-advice-last-turn";
        let _ = std::fs::remove_file(advice_path(session));
        let mut first = finding_with(Remediation::Advise);
        first.excerpt = "first-turn".to_string();
        let mut second = finding_with(Remediation::Advise);
        second.excerpt = "second-turn".to_string();
        store_advice(session, &[&first]);
        store_advice(session, &[&second]);
        let advice = take_advice(session).expect("advice was stored");
        assert!(advice.contains("second-turn"), "{advice}");
        assert!(!advice.contains("first-turn"), "{advice}");
    }

    #[test]
    fn advice_is_capped_and_deduplicated() {
        let session = "test-session-advice-cap";
        let _ = std::fs::remove_file(advice_path(session));
        let same = finding_with(Remediation::Advise);
        let mut distinct: Vec<lints::Finding> = Vec::new();
        for i in 0..(MAX_ADVICE_LINES + 5) {
            let mut f = finding_with(Remediation::Advise);
            f.excerpt = format!("line-{i}");
            distinct.push(f);
        }
        let mut all: Vec<&lints::Finding> = vec![&same, &same, &same];
        all.extend(distinct.iter());
        store_advice(session, &all);
        let advice = take_advice(session).expect("advice was stored");
        let lines: Vec<&str> = advice.lines().collect();
        assert_eq!(lines.len(), MAX_ADVICE_LINES + 1, "{advice}");
        assert_eq!(
            lines.iter().filter(|l| l.contains("\"x\"")).count(),
            1,
            "{advice}"
        );
        assert!(
            lines.last().is_some_and(|l| l.starts_with("...and ")),
            "{advice}"
        );
    }

    /// The reminder is what every prompt pays for, so it stays one line and
    /// it must not itself carry the shapes it warns against.
    #[test]
    fn the_standing_reminder_is_one_line_and_leads_the_prompt_text() {
        assert_eq!(STANDING_REMINDER.lines().count(), 1);
        assert!(STANDING_REMINDER.len() < 400, "{}", STANDING_REMINDER.len());
        let session = "test-session-reminder-order";
        let _ = std::fs::remove_file(advice_path(session));
        assert_eq!(prompt_text(session), STANDING_REMINDER);
        let finding = finding_with(Remediation::Advise);
        store_advice(session, &[&finding]);
        let text = prompt_text(session);
        assert!(text.starts_with(STANDING_REMINDER), "{text}");
        assert!(text.contains("probe-rule"), "{text}");
    }

    /// `ExitCode` exposes nothing else to a caller in the same process, but
    /// its debug form embeds the value: 0 for `SUCCESS`, 2 for the exit-code
    /// refusal `refuse` sends. Good enough to tell a refusal from a pass
    /// here, where nothing else can.
    fn is_refusal(code: ExitCode) -> bool {
        let text = format!("{code:?}");
        assert!(
            text.contains('0') || text.contains('2'),
            "unexpected exit code shape: {text}"
        );
        text.contains('2')
    }

    /// A check that could not run must never look like a check that ran and
    /// found nothing: [0003](../../../docs/architecture/decisions/0003-deterministic-verification-is-authoritative.md)
    /// forbids exactly that, and the four tests below cover the four ways
    /// `stop` can fail to run at all.
    #[test]
    fn a_standard_input_read_failure_refuses_rather_than_passes() {
        let _ = std::fs::remove_file(counter_path("unknown", ""));
        let err = std::io::Error::other("device is busy");
        let code = stop_with_input(Err(err), None, 2, &WritingConfig::default(), None);
        assert!(is_refusal(code));
    }

    #[test]
    fn input_that_is_not_json_refuses_rather_than_passes() {
        let _ = std::fs::remove_file(counter_path("unknown", ""));
        let code = stop_with_input(
            Ok("not json at all".to_string()),
            None,
            2,
            &WritingConfig::default(),
            None,
        );
        assert!(is_refusal(code));
    }

    #[test]
    fn an_event_with_no_assistant_message_refuses_rather_than_passes() {
        let session = "test-session-no-assistant-message";
        let _ = std::fs::remove_file(counter_path(session, ""));
        let raw = serde_json::json!({ "session_id": session }).to_string();
        let code = stop_with_input(Ok(raw), None, 2, &WritingConfig::default(), None);
        assert!(is_refusal(code));
    }

    #[test]
    fn an_unreadable_known_names_file_refuses_rather_than_passes() {
        let session = "test-session-unreadable-known-names";
        let _ = std::fs::remove_file(counter_path(session, ""));
        let raw = serde_json::json!({
            "session_id": session,
            "last_assistant_message": "hello"
        })
        .to_string();
        let missing = Path::new("osf-hook-test-missing-known-names-file.txt");
        let code = stop_with_input(Ok(raw), Some(missing), 2, &WritingConfig::default(), None);
        assert!(is_refusal(code));
    }

    /// The bounce budget applies to a could-not-run refusal the same way it
    /// applies to a genuine finding, so a harness that retries forever
    /// against a defect that never clears (here: a known-names file that
    /// stays missing) still gets let through eventually, loudly, rather
    /// than hanging the turn forever.
    #[test]
    fn a_could_not_run_refusal_is_let_through_after_max_bounces() {
        let session = "test-session-could-not-run-bounce-limit";
        let counter = counter_path(session, "");
        let _ = std::fs::remove_file(&counter);
        let raw = serde_json::json!({
            "session_id": session,
            "last_assistant_message": "hello"
        })
        .to_string();
        let missing = Path::new("osf-hook-test-missing-known-names-file.txt");
        let cfg = WritingConfig::default();

        let first = stop_with_input(Ok(raw.clone()), Some(missing), 1, &cfg, None);
        assert!(is_refusal(first), "attempt 1 of 1 still refuses");
        let second = stop_with_input(Ok(raw), Some(missing), 1, &cfg, None);
        assert!(
            !is_refusal(second),
            "the bounce budget is spent, so the turn goes through"
        );
        assert!(
            !counter.exists(),
            "the counter is cleared once the turn is let through"
        );
    }
}
