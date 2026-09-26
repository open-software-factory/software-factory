//! The reviewer roster, and running one reviewer through its coding-agent
//! command-line tool, headless.
//!
//! osf never holds a model provider's API key. A reviewer is one of the
//! coding-agent tools already installed and logged in on this machine; osf
//! only starts it with a prompt and, where the tool supports it, a JSON
//! Schema to constrain its answer. Every shipped roster entry ships
//! disabled; onboarding, or a repository's own `osf.toml`, turns one on.

use crate::answer::{self, Answer};
use crate::lenses::Lens;
use std::io::Write as _;
#[cfg(unix)]
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt as _;

/// The shipped roster, embedded so `osf` needs no network access to know
/// its own default reviewers.
const SHIPPED_ROSTER: &str = include_str!("../defaults/review-roster.toml");

/// How a reviewer's `schema_flag` value is given: most coding-agent tools
/// take a file path, but at least one (Claude Code's `--json-schema`) takes
/// the schema's own JSON text on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SchemaArg {
    #[default]
    Path,
    Inline,
}

/// One reviewer: a coding-agent harness, run with one model family, through
/// a fixed command line.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reviewer {
    pub name: String,
    pub harness: String,
    pub family: String,
    /// The argument list to run, in order. `{prompt_file}` is replaced with
    /// the path of a file holding the prompt, for a harness that reads one
    /// from a file rather than from standard input.
    pub command: Vec<String>,
    /// The flag that introduces the answer schema, when this harness can be
    /// asked to validate its own output against one.
    #[serde(default)]
    pub schema_flag: Option<String>,
    /// How `schema_flag`'s value is given. Ignored when `schema_flag` is `None`.
    #[serde(default)]
    pub schema_as: SchemaArg,
    /// A JSON pointer to the answer inside the harness's own output
    /// envelope, such as `/structured_output`. Empty when the whole output
    /// is the answer.
    #[serde(default)]
    pub answer_pointer: String,
    #[serde(default)]
    pub enabled: bool,
}

/// One reviewer's outcome for one lens.
#[derive(Debug)]
pub enum Outcome {
    /// The reviewer answered, and the answer validated.
    Answered(Answer),
    /// The reviewer answered twice, and neither answer validated. Counts as missing.
    Invalid(String),
    /// The reviewer's harness could not be started, timed out, or exited non-zero.
    CouldNotRun(String),
}

/// The shipped roster, then this repository's own `osf.toml` `[review]`
/// overrides, a later reviewer of the same `name` replacing an earlier one
/// and a new name appending.
///
/// # Errors
/// Returns an error when the shipped roster fails to parse (a defect in
/// this crate), or `<root>/osf.toml` is not valid TOML, or its `[review]`
/// table does not match the reviewer shape.
pub fn roster(root: &Path) -> Result<Vec<Reviewer>, String> {
    let mut reviewers = shipped_roster()?;
    let overrides = crate::config::review_config(root)
        .map_err(|e| e.to_string())?
        .roster;
    for reviewer in overrides {
        upsert(&mut reviewers, reviewer);
    }
    Ok(reviewers)
}

fn shipped_roster() -> Result<Vec<Reviewer>, String> {
    #[derive(serde::Deserialize)]
    struct Shipped {
        reviewer: Vec<Reviewer>,
    }
    let shipped: Shipped =
        toml::from_str(SHIPPED_ROSTER).map_err(|e| format!("review-roster.toml: {e}"))?;
    Ok(shipped.reviewer)
}

/// Replaces the reviewer named `reviewer.name` in place, keeping roster order, or appends it as new.
fn upsert(reviewers: &mut Vec<Reviewer>, reviewer: Reviewer) {
    match reviewers.iter_mut().find(|r| r.name == reviewer.name) {
        Some(existing) => *existing = reviewer,
        None => reviewers.push(reviewer),
    }
}

/// Runs `reviewer` against `prompt` for `lens` in `workdir`, and returns its
/// validated answer.
///
/// An answer that does not validate is asked for once more, with the
/// validation reason appended to the prompt; a second failure is reported
/// [`Outcome::Invalid`] rather than retried again. A harness that cannot
/// start, that runs past `timeout`, or that exits non-zero is
/// [`Outcome::CouldNotRun`], naming the reason. A run past `timeout` is
/// killed through [`crate::process::kill_tree`], the same tree-kill helper
/// `moon.rs` uses, so a coding-agent tool that spawns its own child
/// processes cannot outlive the timeout.
#[must_use]
pub fn run_one(
    reviewer: &Reviewer,
    prompt: &str,
    lens: &Lens,
    workdir: &Path,
    timeout: Duration,
) -> Outcome {
    let raw = match run_child(reviewer, prompt, workdir, timeout) {
        Ok(text) => text,
        Err(reason) => return Outcome::CouldNotRun(reason),
    };
    match validate_stage(&raw, reviewer, lens) {
        Ok(answer) => Outcome::Answered(answer),
        Err(reason) => {
            let retry_prompt = format!("{prompt}\n\nThe previous answer was invalid: {reason}");
            match run_child(reviewer, &retry_prompt, workdir, timeout) {
                Ok(raw) => match validate_stage(&raw, reviewer, lens) {
                    Ok(answer) => Outcome::Answered(answer),
                    Err(reason) => Outcome::Invalid(reason),
                },
                Err(reason) => Outcome::CouldNotRun(reason),
            }
        }
    }
}

/// `raw`'s answer text, pulled out of `reviewer.answer_pointer` when it
/// names one, then validated against `lens`.
fn validate_stage(raw: &str, reviewer: &Reviewer, lens: &Lens) -> Result<Answer, String> {
    let text = extract_pointer(raw, &reviewer.answer_pointer)?;
    answer::extract_and_validate(&text, lens)
}

/// `raw`, unchanged, when `pointer` is empty; otherwise the JSON value at
/// `pointer` inside `raw`'s own envelope, re-serialised as text so
/// [`answer::extract_and_validate`] can parse it the same way either path.
///
/// # Errors
/// Names the pointer when `raw` is not valid JSON, or holds nothing at
/// `pointer`.
fn extract_pointer(raw: &str, pointer: &str) -> Result<String, String> {
    if pointer.is_empty() {
        return Ok(raw.to_string());
    }
    let envelope: serde_json::Value = serde_json::from_str(raw.trim()).map_err(|e| {
        format!(
            "the answer envelope is not valid JSON at line {} column {}",
            e.line(),
            e.column()
        )
    })?;
    let found = envelope
        .pointer(pointer)
        .ok_or_else(|| format!("the answer envelope has nothing at \"{pointer}\""))?;
    serde_json::to_string(found).map_err(|_| format!("cannot read the value at \"{pointer}\""))
}

/// One attempt at running `reviewer`'s harness to completion: its captured
/// standard output on a successful exit, or the reason it does not count as
/// one.
fn run_child(
    reviewer: &Reviewer,
    prompt: &str,
    workdir: &Path,
    timeout: Duration,
) -> Result<String, String> {
    let prompt_file = write_temp_file("osf-review-prompt", prompt)?;
    let schema_file = write_temp_file("osf-review-schema", answer::SCHEMA)?;
    let cleanup = || {
        let _ = std::fs::remove_file(&prompt_file);
        let _ = std::fs::remove_file(&schema_file);
    };

    let args = build_args(reviewer, &prompt_file, &schema_file);
    let Some((program, rest)) = args.split_first() else {
        cleanup();
        return Err(format!("reviewer '{}' has an empty command", reviewer.name));
    };

    let mut command = Command::new(program);
    command
        .args(rest)
        .current_dir(workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            cleanup();
            return Err(format!("cannot run reviewer '{}': {e}", reviewer.name));
        }
    };

    // Every pipe is drained, and the prompt written, on its own thread,
    // started before `wait_timeout` below: a harness that never reads its
    // stdin, given a prompt bigger than the pipe's own buffer, would
    // otherwise block a synchronous write forever, on the same thread that
    // is supposed to be enforcing `timeout`. `wait_timeout` alone then
    // governs the whole run; the writer unblocks (with an error) once the
    // timeout branch below kills the child and its pipe closes.
    let stdout_reader = child
        .stdout
        .take()
        .map(|mut pipe| std::thread::spawn(move || read_all(&mut pipe)));
    let stderr_reader = child
        .stderr
        .take()
        .map(|mut pipe| std::thread::spawn(move || read_all(&mut pipe)));
    let prompt_owned = prompt.to_string();
    let stdin_writer = child
        .stdin
        .take()
        .map(|mut stdin| std::thread::spawn(move || stdin.write_all(prompt_owned.as_bytes())));

    let status = match child.wait_timeout(timeout) {
        Ok(Some(status)) => status,
        Ok(None) => {
            let killed = crate::process::kill_tree(&child);
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.map(std::thread::JoinHandle::join);
            let _ = stderr_reader.map(std::thread::JoinHandle::join);
            let _ = stdin_writer.map(std::thread::JoinHandle::join);
            cleanup();
            return Err(match killed {
                Ok(()) => format!(
                    "reviewer '{}' timed out after {timeout:?} and was killed",
                    reviewer.name
                ),
                Err(e) => format!(
                    "reviewer '{}' timed out and could not be killed: {e}",
                    reviewer.name
                ),
            });
        }
        Err(e) => {
            cleanup();
            return Err(format!("cannot wait for reviewer '{}': {e}", reviewer.name));
        }
    };

    let stdin_result = stdin_writer.map(|h| {
        h.join()
            .unwrap_or_else(|_| Err(std::io::Error::other("the stdin writer thread panicked")))
    });
    let stdout_text = stdout_reader
        .map(|h| h.join().unwrap_or_default())
        .unwrap_or_default();
    // Drained and joined so the reader thread always finishes cleanly, but
    // never read: a reviewer's stderr is reviewer-controlled text, and this
    // function's own errors are journalled, so it must never appear in one.
    let _stderr_text = stderr_reader
        .map(|h| h.join().unwrap_or_default())
        .unwrap_or_default();
    cleanup();

    if let Some(Err(e)) = stdin_result {
        return Err(format!(
            "cannot write to reviewer '{}' stdin: {e}",
            reviewer.name
        ));
    }

    if !status.success() {
        let code = status
            .code()
            .map_or_else(|| "no exit code".to_string(), |c| c.to_string());
        return Err(format!(
            "reviewer '{}' exited with code {code}",
            reviewer.name
        ));
    }
    Ok(stdout_text)
}

/// `reviewer.command`, with `{prompt_file}` replaced by `prompt_file`'s
/// path, and `schema_flag`/its value appended when the reviewer declares one.
fn build_args(reviewer: &Reviewer, prompt_file: &Path, schema_file: &Path) -> Vec<String> {
    let prompt_path = prompt_file.to_string_lossy().into_owned();
    let mut args: Vec<String> = reviewer
        .command
        .iter()
        .map(|token| {
            if token == "{prompt_file}" {
                prompt_path.clone()
            } else {
                token.clone()
            }
        })
        .collect();
    if let Some(flag) = &reviewer.schema_flag {
        args.push(flag.clone());
        args.push(match reviewer.schema_as {
            SchemaArg::Path => schema_file.to_string_lossy().into_owned(),
            SchemaArg::Inline => answer::SCHEMA.to_string(),
        });
    }
    args
}

/// A file unique to this process and this call, under the OS temp
/// directory, holding `content`.
fn write_temp_file(prefix: &str, content: &str) -> Result<PathBuf, String> {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir() // osf: temp-dir allowed, one file per reviewer run
        .join(format!("{prefix}-{}-{unique}", std::process::id()));
    std::fs::write(&path, content).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(path)
}

/// Reads a pipe to the end, discarding a read error's content but not its
/// occurrence: a partial read is still worth the caller having.
fn read_all(pipe: &mut impl std::io::Read) -> String {
    let mut buf = String::new();
    let _ = std::io::Read::read_to_string(pipe, &mut buf);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reviewer(name: &str, command: Vec<&str>) -> Reviewer {
        Reviewer {
            name: name.to_string(),
            harness: name.to_string(),
            family: "test-family".to_string(),
            command: command.into_iter().map(str::to_string).collect(),
            schema_flag: None,
            schema_as: SchemaArg::default(),
            answer_pointer: String::new(),
            enabled: false,
        }
    }

    #[test]
    fn the_shipped_roster_has_no_gemini_and_every_entry_disabled() {
        let r = roster(&std::env::temp_dir()).expect("roster"); // osf: temp-dir allowed, no osf.toml is read from it here
        assert!(r.iter().all(|x| !x.harness.contains("gemini")));
        assert!(r.iter().all(|x| !x.enabled));
    }

    #[test]
    fn the_shipped_roster_has_the_four_named_harnesses() {
        let r = roster(&std::env::temp_dir()).expect("roster"); // osf: temp-dir allowed, no osf.toml is read from it here
        for harness in ["codex", "claude", "dsh", "opencode"] {
            assert!(
                r.iter().any(|x| x.harness == harness),
                "no roster entry for {harness}: {r:?}"
            );
        }
    }

    #[test]
    fn an_adopter_roster_entry_replaces_a_shipped_one_by_name() {
        let root = crate::test_support::TempDir::new("osf-reviewers-roster-replace");
        std::fs::write(
            root.join("osf.toml"),
            "[[review.roster]]\nname = \"codex\"\nharness = \"codex\"\nfamily = \"openai\"\ncommand = [\"codex\", \"exec\"]\nenabled = true\n",
        )
        .expect("osf.toml writes");
        let r = roster(&root).expect("roster");
        let codex = r.iter().find(|x| x.name == "codex").expect("codex entry");
        assert!(codex.enabled);
        assert_eq!(r.iter().filter(|x| x.name == "codex").count(), 1);
    }

    #[test]
    fn build_args_substitutes_the_prompt_file_and_appends_the_schema_flag() {
        let mut r = reviewer("fake", vec!["fake", "{prompt_file}"]);
        r.schema_flag = Some("--schema".to_string());
        r.schema_as = SchemaArg::Path;
        let args = build_args(
            &r,
            Path::new("/tmp/prompt.txt"),
            Path::new("/tmp/schema.json"),
        );
        assert_eq!(
            args,
            vec!["fake", "/tmp/prompt.txt", "--schema", "/tmp/schema.json"]
        );
    }

    #[test]
    fn build_args_inlines_the_schema_text_when_asked() {
        let mut r = reviewer("fake", vec!["fake"]);
        r.schema_flag = Some("--json-schema".to_string());
        r.schema_as = SchemaArg::Inline;
        let args = build_args(
            &r,
            Path::new("/tmp/prompt.txt"),
            Path::new("/tmp/schema.json"),
        );
        assert_eq!(args, vec!["fake", "--json-schema", answer::SCHEMA]);
    }

    #[test]
    fn extract_pointer_returns_the_raw_text_when_no_pointer_is_set() {
        assert_eq!(
            extract_pointer("{\"a\":1}", "").expect("no pointer"),
            "{\"a\":1}"
        );
    }

    #[test]
    fn extract_pointer_pulls_the_named_field_out_of_the_envelope() {
        let envelope = r#"{"structured_output":{"lens":"x"},"other":1}"#;
        let extracted = extract_pointer(envelope, "/structured_output").expect("pointer resolves");
        assert_eq!(extracted, r#"{"lens":"x"}"#);
    }

    #[test]
    fn extract_pointer_names_the_pointer_when_nothing_is_there() {
        let e =
            extract_pointer(r#"{"other":1}"#, "/structured_output").expect_err("missing pointer");
        assert!(e.contains("/structured_output"), "{e}");
    }

    #[test]
    fn extract_pointer_names_the_pointer_when_the_envelope_is_not_json() {
        let e =
            extract_pointer("not json at all", "/structured_output").expect_err("not JSON at all");
        assert!(e.contains("JSON"), "{e}");
    }
}
