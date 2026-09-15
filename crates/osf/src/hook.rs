//! Hook handlers. Each reads one JSON event from standard input and answers
//! in the shape the calling harness expects.
//!
//! Stop: Claude Code, Codex and the dsh bridge send `last_assistant_message`
//! and honour exit code 2 with the reason on standard error. Copilot sends
//! `transcriptPath` in camel case and honours `{"decision":"block"}` on
//! standard output. `omp` and `OpenCode` cannot refuse a stop; their adapters
//! only print the findings.

use crate::lint;
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const MAX_LINES_IN_REASON: usize = 30;

enum Answer {
    ExitTwo,
    DecisionBlock,
}

pub fn stop(known_names: Option<&Path>, max_bounces: u32) -> ExitCode {
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
    let answer = if event.get("sessionId").is_some() {
        Answer::DecisionBlock
    } else {
        Answer::ExitTwo
    };
    let session =
        string_at(&event, &["session_id", "sessionId"]).unwrap_or_else(|| "unknown".to_string());
    let prompt = string_at(&event, &["prompt_id", "turn_id"]).unwrap_or_default();

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
    let errors: Vec<lint::Finding> = lint::lint_writing(&text, &known, lint::Kind::Message, true)
        .into_iter()
        .filter(|f| f.level == lint::Level::Error)
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
        Answer::ExitTwo => {
            eprintln!("{reason}");
            ExitCode::from(2)
        }
        Answer::DecisionBlock => {
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
}
