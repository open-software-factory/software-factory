//! The `dsh` harness adapter: the first concrete [`Harness`], built from
//! pure functions and run only through a [`CommandRunner`].
//!
//! It runs `dsh --profile headless --json -`, feeds the prompt on stdin, and
//! reads newline-delimited JSON events from stdout. It starts no process of
//! its own; the runner does.
//!
//! Limits, stated rather than assumed:
//!
//! - `dsh` reports no model name and no cost figure, so the adapter records
//!   the model family and leaves the model and the cost unset.
//! - A timeout ends the run; there is no mid-run stop and the headless
//!   profile runs no stop hook.
//! - There is no question event: the adapter infers a question from a final
//!   message that ends in `?` and a run that changed no file.
//! - Changed files come from `git status --porcelain=v1 -z`, so ignored files
//!   and commits the agent made are not seen. The prompt therefore forbids
//!   commits.

use crate::harness::{
    build_prompt, Actor, Capability, CommandOutcome, CommandRunner, CommandSpec, Harness,
    HarnessCapabilities, HarnessError, HarnessOutcome, HarnessResult, HarnessTask, Usage,
};

/// The harness name recorded in every result.
pub const HARNESS_NAME: &str = "dsh";

/// The model family this harness runs; the adapter never names a model.
pub const MODEL_FAMILY: &str = "DeepSeek";

/// The version this adapter is pinned to.
pub const PINNED_VERSION: &str = "0.2.0-rc.2";

/// How to reach the `dsh` binary: its program, its complete child
/// environment, and the version this adapter expects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DshConfig {
    pub program: String,
    /// The complete environment of the child; nothing is inherited.
    pub env: Vec<(String, String)>,
    /// The pinned version `--version` must report.
    pub version: String,
}

impl DshConfig {
    /// A config for `program` and `env`, pinned to [`PINNED_VERSION`].
    #[must_use]
    pub fn new(program: impl Into<String>, env: Vec<(String, String)>) -> Self {
        Self {
            program: program.into(),
            env,
            version: PINNED_VERSION.to_string(),
        }
    }
}

/// The concrete adapter for the `dsh` harness.
#[derive(Debug, Clone)]
pub struct DshHarness {
    config: DshConfig,
}

impl DshHarness {
    /// An adapter that runs `config`.
    #[must_use]
    pub fn new(config: DshConfig) -> Self {
        Self { config }
    }
}

/// The command that runs one headless task, reading the prompt on stdin.
#[must_use]
pub fn build_command(
    config: &DshConfig,
    prompt: &str,
    repository_path: &str,
    timeout_secs: Option<u64>,
) -> CommandSpec {
    CommandSpec {
        program: config.program.clone(),
        args: vec![
            "--profile".to_string(),
            "headless".to_string(),
            "--json".to_string(),
            "-".to_string(),
        ],
        workdir: Some(repository_path.to_string()),
        env: config.env.clone(),
        stdin: Some(prompt.to_string()),
        timeout_secs,
    }
}

/// The command that lists every change in the repository, NUL separated.
#[must_use]
pub fn status_command(config: &DshConfig, repository_path: &str) -> CommandSpec {
    CommandSpec {
        program: "git".to_string(),
        args: vec![
            "status".to_string(),
            "--porcelain=v1".to_string(),
            "-z".to_string(),
            "--untracked-files=all".to_string(),
        ],
        workdir: Some(repository_path.to_string()),
        env: config.env.clone(),
        stdin: None,
        timeout_secs: Some(60),
    }
}

/// The command that probes `dsh` itself, for `--version` and `--help`.
#[must_use]
pub fn probe_command(config: &DshConfig, args: &[&str]) -> CommandSpec {
    CommandSpec {
        program: config.program.clone(),
        args: args.iter().copied().map(String::from).collect(),
        workdir: None,
        env: config.env.clone(),
        stdin: None,
        timeout_secs: Some(30),
    }
}

/// The events one run printed, reduced to what the factory records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub session_id: Option<String>,
    pub final_message: Option<String>,
    pub usage: Option<Usage>,
}

/// Parses the newline-delimited JSON events `dsh` prints for one run.
///
/// # Errors
///
/// Returns a message naming the line when a non-empty line is not a JSON
/// object.
pub fn parse_events(stdout: &str) -> Result<Parsed, String> {
    let mut parsed = Parsed {
        session_id: None,
        final_message: None,
        usage: None,
    };
    let mut totals: Option<Usage> = None;
    for (index, line) in stdout.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            return Err(not_a_json_object(index + 1, line));
        };
        let Some(object) = value.as_object() else {
            return Err(not_a_json_object(index + 1, line));
        };
        match object.get("type").and_then(serde_json::Value::as_str) {
            Some("session") => {
                if let Some(id) = object.get("sessionId").and_then(serde_json::Value::as_str) {
                    parsed.session_id = Some(id.to_string());
                }
            }
            Some("final") => {
                if let Some(text) = object.get("text").and_then(serde_json::Value::as_str) {
                    parsed.final_message = Some(text.to_string());
                }
            }
            Some("status")
                if object.get("phase").and_then(serde_json::Value::as_str) == Some("step_end") =>
            {
                if let Some(usage) = object.get("usage").and_then(serde_json::Value::as_object) {
                    let step = usage_of(usage);
                    totals = Some(match totals {
                        Some(sum) => Usage {
                            input_tokens: sum.input_tokens.saturating_add(step.input_tokens),
                            output_tokens: sum.output_tokens.saturating_add(step.output_tokens),
                            total_tokens: sum.total_tokens.saturating_add(step.total_tokens),
                        },
                        None => step,
                    });
                }
            }
            _ => {}
        }
    }
    parsed.usage = totals;
    Ok(parsed)
}

/// The text of a line that is not a JSON object, cut to its first 80 chars.
fn not_a_json_object(number: usize, line: &str) -> String {
    let shown: String = line.chars().take(80).collect();
    format!("line {number} is not a JSON object: {shown}")
}

/// The three token figures of one step, missing figures counting as zero.
fn usage_of(usage: &serde_json::Map<String, serde_json::Value>) -> Usage {
    Usage {
        input_tokens: usage
            .get("inputTokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        output_tokens: usage
            .get("outputTokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        total_tokens: usage
            .get("totalTokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
    }
}

/// The repository-relative paths in `git status --porcelain=v1 -z` output,
/// sorted and without duplicates, including a rename's original path.
#[must_use]
pub fn parse_changed_files(porcelain_z: &str) -> Vec<String> {
    let fields: Vec<&str> = porcelain_z
        .split('\0')
        .filter(|field| !field.is_empty())
        .collect();
    let mut files = Vec::new();
    let mut index = 0;
    while let Some(entry) = fields.get(index) {
        if let Some(path) = entry.get(3..) {
            files.push(path.to_string());
        }
        let bytes = entry.as_bytes();
        let renamed = bytes
            .first()
            .is_some_and(|byte| *byte == b'R' || *byte == b'C')
            || bytes
                .get(1)
                .is_some_and(|byte| *byte == b'R' || *byte == b'C');
        if renamed {
            if let Some(original) = fields.get(index + 1) {
                files.push((*original).to_string());
                index += 2;
                continue;
            }
        }
        index += 1;
    }
    files.sort();
    files.dedup();
    files
}

/// The version text, trimmed.
#[must_use]
pub fn parse_version(stdout: &str) -> String {
    stdout.trim().to_string()
}

/// Whether the headless help promises JSON events and reading stdin.
#[must_use]
pub fn headless_help_is_adequate(help: &str) -> bool {
    let collapsed = help.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.contains("--json") && collapsed.contains("`-` reads stdin")
}

/// Whether a run ended with a question and changed no file.
#[must_use]
pub fn asks_a_question(final_message: &str, changed_files: &[String]) -> bool {
    final_message.trim().ends_with('?') && changed_files.is_empty()
}

impl Harness for DshHarness {
    fn run(
        &self,
        runner: &dyn CommandRunner,
        task: &HarnessTask,
    ) -> Result<HarnessResult, HarnessError> {
        if task.text.trim().is_empty() {
            return Err(HarnessError::Rejected("the task text is empty".to_string()));
        }
        if task.repository_path.is_empty() {
            return Err(HarnessError::Rejected(
                "the repository path is empty".to_string(),
            ));
        }
        let before = status_of(runner, &self.config, &task.repository_path)?;
        if !before.is_empty() {
            return Err(HarnessError::Rejected(format!(
                "the repository has uncommitted changes: {}",
                before.join(", ")
            )));
        }

        let command = build_command(
            &self.config,
            &build_prompt(&task.text),
            &task.repository_path,
            task.timeout_secs,
        );
        let output = runner.run(&command)?;
        let stdout = match &output.outcome {
            CommandOutcome::TimedOut { limit_secs } => {
                return Err(HarnessError::Failed(format!(
                    "dsh timed out after {limit_secs}s"
                )));
            }
            CommandOutcome::Exited(0) => output.stdout.clone(),
            CommandOutcome::Exited(code) => {
                return Err(HarnessError::Failed(failure_text(
                    "dsh",
                    *code,
                    &output.stderr,
                    &output.stdout,
                )));
            }
        };

        let parsed = parse_events(&stdout).map_err(HarnessError::Failed)?;
        let Some(final_message) = parsed.final_message else {
            return Err(HarnessError::Failed(
                "dsh exited 0 but printed no final event".to_string(),
            ));
        };

        let changed_files = status_of(runner, &self.config, &task.repository_path)?;
        let outcome = if asks_a_question(&final_message, &changed_files) {
            HarnessOutcome::Asked {
                question: final_message.clone(),
            }
        } else {
            HarnessOutcome::Finished
        };

        Ok(HarnessResult {
            actor: Actor {
                harness: HARNESS_NAME.to_string(),
                model: None,
                model_family: Some(MODEL_FAMILY.to_string()),
            },
            session_id: parsed.session_id,
            final_message,
            outcome,
            exit: 0,
            changed_files,
            usage: parsed.usage,
            cost_micro_usd: None,
        })
    }

    fn capabilities(&self, runner: &dyn CommandRunner) -> HarnessCapabilities {
        let version = probe_version(runner, &self.config);
        HarnessCapabilities {
            binary_present: version.binary_present,
            version_pinned: version.version_pinned,
            headless: probe_headless(runner, &self.config, version.succeeded),
            structured_result: Capability::Supported,
            cost_reporting: Capability::Unsupported("token usage only, no cost figure".to_string()),
            stop_mid_run: Capability::Unsupported(
                "a timeout ends the run; the tool has no mid-run stop".to_string(),
            ),
            stop_hook_checks_messages: Capability::Unsupported(
                "the headless profile runs no stop hook".to_string(),
            ),
            question_signal: Capability::Unsupported(
                "no question event; a run is reported as a question when the final message ends with a question mark and no file changed"
                    .to_string(),
            ),
        }
    }
}

/// Runs the clean-tree status command and parses the changed files.
fn status_of(
    runner: &dyn CommandRunner,
    config: &DshConfig,
    repository_path: &str,
) -> Result<Vec<String>, HarnessError> {
    let output = runner.run(&status_command(config, repository_path))?;
    match &output.outcome {
        CommandOutcome::TimedOut { limit_secs } => Err(HarnessError::Failed(format!(
            "git status timed out after {limit_secs}s"
        ))),
        CommandOutcome::Exited(0) => Ok(parse_changed_files(&output.stdout)),
        CommandOutcome::Exited(code) => Err(HarnessError::Failed(failure_text(
            "git status",
            *code,
            &output.stderr,
            &output.stdout,
        ))),
    }
}

/// The tool's own text for a failed command, or a status fallback.
fn failure_text(tool: &str, status: i32, stderr: &str, stdout: &str) -> String {
    let stderr = stderr.trim();
    if !stderr.is_empty() {
        return stderr.to_string();
    }
    let stdout = stdout.trim();
    if !stdout.is_empty() {
        return stdout.to_string();
    }
    format!("{tool} exited with status {status}")
}

/// What the version probe found and whether the headless probe may run.
struct VersionProbe {
    binary_present: Capability,
    version_pinned: Capability,
    succeeded: bool,
}

/// Probes `--version`, which tells presence, the pin, and whether to
/// continue.
fn probe_version(runner: &dyn CommandRunner, config: &DshConfig) -> VersionProbe {
    match runner.run(&probe_command(config, &["--version"])) {
        Err(error) => {
            let text = error.to_string();
            VersionProbe {
                binary_present: Capability::Unknown(text.clone()),
                version_pinned: Capability::Unknown(text),
                succeeded: false,
            }
        }
        Ok(output) => match &output.outcome {
            CommandOutcome::TimedOut { limit_secs } => {
                let text = format!("version probe timed out after {limit_secs}s");
                VersionProbe {
                    binary_present: Capability::Unknown(text.clone()),
                    version_pinned: Capability::Unknown(text),
                    succeeded: false,
                }
            }
            CommandOutcome::Exited(0) => {
                let installed = parse_version(&output.stdout);
                let pinned = &config.version;
                let version_pinned = if installed == *pinned {
                    Capability::Supported
                } else {
                    Capability::Unsupported(format!(
                        "version {installed} is installed, {pinned} is pinned"
                    ))
                };
                VersionProbe {
                    binary_present: Capability::Supported,
                    version_pinned,
                    succeeded: true,
                }
            }
            CommandOutcome::Exited(code) => {
                let text = failure_text("version probe", *code, &output.stderr, &output.stdout);
                VersionProbe {
                    binary_present: Capability::Unknown(text.clone()),
                    version_pinned: Capability::Unknown(text),
                    succeeded: false,
                }
            }
        },
    }
}

/// Probes the headless help, but only when the version probe succeeded.
fn probe_headless(
    runner: &dyn CommandRunner,
    config: &DshConfig,
    version_succeeded: bool,
) -> Capability {
    if !version_succeeded {
        return Capability::Unknown(
            "the version probe failed, so headless mode was not probed".to_string(),
        );
    }
    match runner.run(&probe_command(config, &["--profile", "headless", "--help"])) {
        Err(error) => Capability::Unknown(error.to_string()),
        Ok(output) => match &output.outcome {
            CommandOutcome::TimedOut { limit_secs } => {
                Capability::Unknown(format!("headless probe timed out after {limit_secs}s"))
            }
            CommandOutcome::Exited(0) => {
                if headless_help_is_adequate(&output.stdout) {
                    Capability::Supported
                } else {
                    Capability::Unsupported(
                        "the headless help does not show --json and stdin input".to_string(),
                    )
                }
            }
            CommandOutcome::Exited(code) => Capability::Unknown(failure_text(
                "headless probe",
                *code,
                &output.stderr,
                &output.stdout,
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::fake::FakeRunner;
    use crate::harness::{CommandOutcome, CommandOutput, HarnessError, HarnessTask, Usage};

    const CREATED_FILE_FIXTURE: &str =
        include_str!("../tests/fixtures/harness/dsh-run-created-file.jsonl");
    const QUESTION_FIXTURE: &str = include_str!("../tests/fixtures/harness/dsh-run-question.jsonl");
    const VERSION_FIXTURE: &str = include_str!("../tests/fixtures/harness/dsh-version.txt");
    const HELP_FIXTURE: &str = include_str!("../tests/fixtures/harness/dsh-headless-help.txt");

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().copied().map(String::from).collect()
    }

    fn config() -> DshConfig {
        DshConfig::new("dsh", Vec::new())
    }

    #[test]
    fn build_command_is_headless_json_reading_stdin() {
        let command = build_command(&config(), "do the thing", "/repo", Some(12));
        assert_eq!(command.program, "dsh");
        assert_eq!(
            command.args,
            strings(&["--profile", "headless", "--json", "-"])
        );
        assert_eq!(command.stdin.as_deref(), Some("do the thing"));
        assert_eq!(command.workdir.as_deref(), Some("/repo"));
        assert_eq!(command.timeout_secs, Some(12));
    }

    #[test]
    fn build_command_carries_exactly_the_configured_environment() {
        let env = vec![
            ("PATH".to_string(), "/bin".to_string()),
            ("HOME".to_string(), "/var/agent-home".to_string()),
        ];
        let command = build_command(&DshConfig::new("dsh", env.clone()), "go", "/repo", None);
        assert_eq!(command.env, env);

        let empty = build_command(&config(), "go", "/repo", None);
        assert!(empty.env.is_empty(), "an empty environment stays empty");
    }

    #[test]
    fn status_command_pins_its_arguments_and_workdir() {
        let command = status_command(&config(), "/repo");
        assert_eq!(command.program, "git");
        assert_eq!(
            command.args,
            strings(&["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        );
        assert_eq!(command.workdir.as_deref(), Some("/repo"));
        assert_eq!(command.timeout_secs, Some(60));
        assert!(command.stdin.is_none());
    }

    #[test]
    fn probe_command_carries_the_arguments_and_a_thirty_second_timeout() {
        let version = probe_command(&config(), &["--version"]);
        assert_eq!(version.program, "dsh");
        assert_eq!(version.args, strings(&["--version"]));
        assert_eq!(version.timeout_secs, Some(30));

        let help = probe_command(&config(), &["--profile", "headless", "--help"]);
        assert_eq!(help.args, strings(&["--profile", "headless", "--help"]));
    }

    #[test]
    fn parse_events_reads_the_created_file_fixture() {
        let parsed = parse_events(CREATED_FILE_FIXTURE).expect("the fixture parses");
        assert_eq!(
            parsed.session_id.as_deref(),
            Some("session-51e234bb-b776-44fd-89d7-e846db4cdaab")
        );
        assert_eq!(
            parsed.final_message.as_deref(),
            Some("I created `/workspace/hello.txt` containing the single word `hello`.")
        );
        assert_eq!(
            parsed.usage,
            Some(Usage {
                input_tokens: 789,
                output_tokens: 184,
                total_tokens: 16973,
            })
        );
    }

    #[test]
    fn parse_events_reads_the_question_fixture() {
        let parsed = parse_events(QUESTION_FIXTURE).expect("the fixture parses");
        assert_eq!(
            parsed.session_id.as_deref(),
            Some("session-a656c023-b94d-4016-89f5-6e93b8d4abaa")
        );
        assert_eq!(
            parsed.usage,
            Some(Usage {
                input_tokens: 369,
                output_tokens: 40,
                total_tokens: 5529,
            })
        );
    }

    #[test]
    fn parse_events_rejects_a_non_json_line_and_names_its_number() {
        let stdout = "{\"type\":\"session\",\"sessionId\":\"s\"}\n\nnot json\n";
        let error = parse_events(stdout).expect_err("a non-JSON line is refused");
        assert_eq!(error, "line 3 is not a JSON object: not json");
    }

    #[test]
    fn parse_events_rejects_a_json_line_that_is_not_an_object() {
        let error = parse_events("42\n").expect_err("a bare number is refused");
        assert_eq!(error, "line 1 is not a JSON object: 42");
    }

    #[test]
    fn parse_events_ignores_an_unknown_event_type() {
        let stdout =
            "{\"type\":\"mystery\",\"text\":\"ignored\"}\n{\"type\":\"final\",\"text\":\"kept\"}\n";
        let parsed = parse_events(stdout).expect("the lines parse");
        assert_eq!(parsed.final_message.as_deref(), Some("kept"));
    }

    #[test]
    fn parse_events_without_a_final_event_has_no_final_message() {
        let parsed =
            parse_events("{\"type\":\"session\",\"sessionId\":\"s\"}\n").expect("the line parses");
        assert!(parsed.final_message.is_none());
    }

    #[test]
    fn parse_events_keeps_the_last_final_event() {
        let stdout =
            "{\"type\":\"final\",\"text\":\"first\"}\n{\"type\":\"final\",\"text\":\"second\"}\n";
        let parsed = parse_events(stdout).expect("the lines parse");
        assert_eq!(parsed.final_message.as_deref(), Some("second"));
    }

    #[test]
    fn parse_events_without_usage_events_has_no_usage() {
        let parsed =
            parse_events("{\"type\":\"final\",\"text\":\"done\"}\n").expect("the line parses");
        assert!(parsed.usage.is_none());
    }

    #[test]
    fn parse_events_counts_a_missing_usage_field_as_zero() {
        let stdout = "{\"type\":\"status\",\"phase\":\"step_end\",\"usage\":{\"inputTokens\":5,\"totalTokens\":7}}\n";
        let parsed = parse_events(stdout).expect("the line parses");
        assert_eq!(
            parsed.usage,
            Some(Usage {
                input_tokens: 5,
                output_tokens: 0,
                total_tokens: 7,
            })
        );
    }

    #[test]
    fn parse_changed_files_reads_untracked_modified_and_deleted_entries_sorted() {
        let porcelain = "?? new.txt\0 M edited.txt\0 D deleted.txt\0";
        assert_eq!(
            parse_changed_files(porcelain),
            strings(&["deleted.txt", "edited.txt", "new.txt"])
        );
    }

    #[test]
    fn parse_changed_files_reads_a_rename_with_its_original_path() {
        let porcelain = "R  new.txt\0old.txt\0";
        assert_eq!(
            parse_changed_files(porcelain),
            strings(&["new.txt", "old.txt"])
        );
    }

    #[test]
    fn parse_changed_files_dedups() {
        let porcelain = "?? same.txt\0?? same.txt\0";
        assert_eq!(parse_changed_files(porcelain), strings(&["same.txt"]));
    }

    #[test]
    fn parse_changed_files_on_empty_output_is_empty() {
        assert!(parse_changed_files("").is_empty());
    }

    #[test]
    fn parse_version_trims_the_fixture() {
        assert_eq!(parse_version(VERSION_FIXTURE), "0.2.0-rc.2");
    }

    #[test]
    fn headless_help_is_adequate_on_the_help_fixture() {
        assert!(headless_help_is_adequate(HELP_FIXTURE));
    }

    #[test]
    fn headless_help_is_not_adequate_without_json() {
        let help = HELP_FIXTURE.replace("--json", "no-events");
        assert!(!headless_help_is_adequate(&help));
    }

    #[test]
    fn headless_help_is_not_adequate_without_the_stdin_sentence() {
        let help = HELP_FIXTURE.replace("reads stdin", "reads nothing");
        assert!(!headless_help_is_adequate(&help));
    }

    #[test]
    fn asks_a_question_only_with_a_trailing_mark_and_no_changed_file() {
        assert!(asks_a_question("Which one?", &[]));
        assert!(!asks_a_question("Which one?", &strings(&["a.rs"])));
        assert!(!asks_a_question("Done.", &[]));
        assert!(!asks_a_question("Done.", &strings(&["a.rs"])));
    }

    #[test]
    fn the_question_fixture_final_message_is_a_question() {
        let parsed = parse_events(QUESTION_FIXTURE).expect("the fixture parses");
        let message = parsed
            .final_message
            .expect("the fixture has a final message");
        assert!(asks_a_question(&message, &[]));
    }

    #[test]
    fn the_created_file_fixture_final_message_is_not_a_question() {
        let parsed = parse_events(CREATED_FILE_FIXTURE).expect("the fixture parses");
        let message = parsed
            .final_message
            .expect("the fixture has a final message");
        assert!(!asks_a_question(&message, &[]));
    }

    #[test]
    fn run_rejects_an_empty_task_before_any_command() {
        let runner = FakeRunner::new(Vec::new());
        let task = HarnessTask {
            text: "   \n".to_string(),
            repository_path: "/repo".to_string(),
            timeout_secs: None,
        };
        let error = DshHarness::new(config())
            .run(&runner, &task)
            .expect_err("an empty task is rejected");
        assert_eq!(
            error,
            HarnessError::Rejected("the task text is empty".to_string())
        );
        assert!(
            runner.commands().is_empty(),
            "no command reached the runner"
        );
    }

    #[test]
    fn run_rejects_a_dirty_tree_before_the_agent_starts() {
        let dirty = CommandOutput {
            outcome: CommandOutcome::Exited(0),
            stdout: "?? dirty.txt\0".to_string(),
            stderr: String::new(),
        };
        let runner = FakeRunner::new(vec![Ok(dirty)]);
        let task = HarnessTask {
            text: "do the thing".to_string(),
            repository_path: "/repo".to_string(),
            timeout_secs: None,
        };
        let error = DshHarness::new(config())
            .run(&runner, &task)
            .expect_err("a dirty tree is rejected");
        assert_eq!(
            error,
            HarnessError::Rejected("the repository has uncommitted changes: dirty.txt".to_string())
        );
        let commands = runner.commands();
        assert_eq!(commands.len(), 1, "only the status command ran");
        assert_eq!(commands.first().expect("one command").program, "git");
    }
}
