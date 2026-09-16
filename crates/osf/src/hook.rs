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

use crate::lint;
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const MAX_LINES_IN_REASON: usize = 30;

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

pub fn stop(known_names: Option<&Path>, max_bounces: u32, answer: Option<Answer>) -> ExitCode {
    let mut raw = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut raw) {
        eprintln!("osf hook stop: cannot read standard input: {e}");
        return ExitCode::SUCCESS;
    }
    let event: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("osf hook stop: input is not JSON, message not checked: {e}");
            return ExitCode::SUCCESS;
        }
    };
    let answer = answer.unwrap_or_else(|| answer_for(&event));
    let session = string_at(&event, SESSION_KEYS).unwrap_or_else(|| "unknown".to_string());
    let prompt = string_at(&event, PROMPT_KEYS).unwrap_or_default();

    let Some(text) = message_text(&event) else {
        eprintln!("osf hook stop: no assistant message in the event, nothing checked");
        return ExitCode::SUCCESS;
    };
    let known = match lint::load_known_names(known_names) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("osf hook stop: {e}; message not checked");
            return ExitCode::SUCCESS;
        }
    };
    // A stop check runs on every turn end, so it stays on the fast tier only.
    let errors: Vec<lint::Finding> =
        lint::lint_writing(&text, &known, lint::Kind::Message, true, false)
            .into_iter()
            .filter(|f| f.level == lint::Level::Error && f.suppressed.is_none())
            .collect();

    let counter = counter_path(&session, &prompt);
    if errors.is_empty() {
        let _ = std::fs::remove_file(&counter);
        return ExitCode::SUCCESS;
    }
    let bounces = read_counter(&counter);
    if bounces >= max_bounces {
        eprintln!(
            "osf hook stop: {} error(s) remain after {bounces} rewrite(s); letting the message through",
            errors.len()
        );
        let _ = std::fs::remove_file(&counter);
        return ExitCode::SUCCESS;
    }
    write_counter(&counter, bounces + 1);

    let mut lines: Vec<String> = errors
        .iter()
        .take(MAX_LINES_IN_REASON)
        .map(|f| f.render("message", f.level))
        .collect();
    if errors.len() > MAX_LINES_IN_REASON {
        lines.push(format!(
            "...and {} more",
            errors.len() - MAX_LINES_IN_REASON
        ));
    }
    let reason = format!(
        "osf writing-lint refused this message ({} error(s), rewrite {} of {}). Fix every line, then answer again.\n{}",
        errors.len(),
        bounces + 1,
        max_bounces,
        lines.join("\n")
    );
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

fn counter_path(session: &str, prompt: &str) -> PathBuf {
    let safe = |s: &str| {
        s.chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect::<String>()
    };
    let dir = std::env::temp_dir().join("osf-stop");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{}-{}", safe(session), safe(prompt)))
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
}
