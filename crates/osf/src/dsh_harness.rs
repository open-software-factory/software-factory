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
//! - A timeout ends the run; there is no mid-run stop. Whether the headless
//!   profile runs a stop hook that checks messages is read from the profile
//!   dump, not assumed.
//! - A turn that ends with a reason other than `completed` fails the run,
//!   even when the exit status is 0.
//! - There is no question event: the prompt asks the agent to end its final
//!   message with a `QUESTION:` line, and the adapter reads that line.
//! - Changed files come from `git status --porcelain=v1 -z`, so ignored files
//!   are not seen. A commit moves the head, and a run that moves it fails.
//! - Git gets only `PATH`, `HOME` and `GIT_*`; no credential reaches it.

use crate::harness::{
    build_prompt, question_line, Actor, Capability, CommandOutcome, CommandRunner, CommandSpec,
    Harness, HarnessCapabilities, HarnessError, HarnessOutcome, HarnessResult, HarnessTask, Usage,
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

/// The minimal environment for git, so a credential the agent needs never
/// reaches it.
#[must_use]
pub fn git_env(config: &DshConfig) -> Vec<(String, String)> {
    config
        .env
        .iter()
        .filter(|(name, _)| name == "PATH" || name == "HOME" || name.starts_with("GIT_"))
        .cloned()
        .collect()
}

/// A git command in `repository_path`, with the minimal git environment.
fn git_command(config: &DshConfig, repository_path: &str, args: &[&str]) -> CommandSpec {
    CommandSpec {
        program: "git".to_string(),
        args: args.iter().copied().map(String::from).collect(),
        workdir: Some(repository_path.to_string()),
        env: git_env(config),
        stdin: None,
        timeout_secs: Some(60),
    }
}

/// The command that lists every change in the repository, NUL separated.
#[must_use]
pub fn status_command(config: &DshConfig, repository_path: &str) -> CommandSpec {
    git_command(
        config,
        repository_path,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )
}

/// The command that reads the head commit of the repository.
#[must_use]
pub fn head_command(config: &DshConfig, repository_path: &str) -> CommandSpec {
    git_command(config, repository_path, &["rev-parse", "HEAD"])
}

/// The command that reads the current branch, or `HEAD` when detached.
#[must_use]
pub fn branch_command(config: &DshConfig, repository_path: &str) -> CommandSpec {
    git_command(
        config,
        repository_path,
        &["rev-parse", "--abbrev-ref", "HEAD"],
    )
}

/// The head commit in `stdout`, trimmed, or `None` when it is empty.
#[must_use]
pub fn parse_head(stdout: &str) -> Option<String> {
    let head = stdout.trim();
    (!head.is_empty()).then(|| head.to_string())
}

/// The branch in `stdout`, trimmed, or `None` when empty or detached (`HEAD`).
#[must_use]
pub fn parse_branch(stdout: &str) -> Option<String> {
    let branch = stdout.trim();
    (!branch.is_empty() && branch != "HEAD").then(|| branch.to_string())
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
    /// The first turn that ended with a reason other than `completed`.
    pub turn_failure: Option<String>,
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
        turn_failure: None,
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
                            cache_read_tokens: sum
                                .cache_read_tokens
                                .saturating_add(step.cache_read_tokens),
                            total_tokens: sum.total_tokens.saturating_add(step.total_tokens),
                        },
                        None => step,
                    });
                }
            }
            Some("status")
                if object.get("phase").and_then(serde_json::Value::as_str) == Some("turn_end")
                    && parsed.turn_failure.is_none() =>
            {
                parsed.turn_failure = turn_failure_of(object);
            }
            _ => {}
        }
    }
    parsed.usage = totals;
    Ok(parsed)
}

/// The failure text of a `turn_end` event whose reason is not `completed`.
fn turn_failure_of(object: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    let reason = object.get("reason")?.as_object()?;
    let kind = reason.get("kind")?.as_str()?;
    if kind == "completed" {
        return None;
    }
    if let Some(error) = reason.get("error").and_then(serde_json::Value::as_object) {
        let code = error.get("code").and_then(serde_json::Value::as_str);
        let message = error.get("message").and_then(serde_json::Value::as_str);
        match (code, message) {
            (Some(code), Some(message)) => return Some(format!("{code}: {message}")),
            (Some(code), None) => return Some(code.to_string()),
            (None, Some(message)) => return Some(message.to_string()),
            (None, None) => {}
        }
    }
    Some(format!("the turn ended with reason {kind}"))
}

/// The text of a line that is not a JSON object, cut to its first 80 chars.
fn not_a_json_object(number: usize, line: &str) -> String {
    let shown: String = line.chars().take(80).collect();
    format!("line {number} is not a JSON object: {shown}")
}

/// The four token figures of one step, missing figures counting as zero.
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
        cache_read_tokens: usage
            .get("cacheReadTokens")
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

/// The package whose profile row checks messages at the end of a turn.
pub const MESSAGE_CHECK_PLUGIN: &str = "@open-software-factory/osf-dsh-plugin";

/// How a profile dump names the package that checks messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageCheck {
    Enabled,
    Absent,
    Disabled,
    Conditional,
}

/// Reads the message-check plugin row from a `--dump-config` dump.
#[must_use]
pub fn message_check_in(dump: &str) -> MessageCheck {
    let mut state = MessageCheck::Absent;
    for row in dump_rows(dump) {
        let Some(name) = row.iter().find_map(|line| row_value(line, "name:")) else {
            continue;
        };
        if unquote(name) != MESSAGE_CHECK_PLUGIN {
            continue;
        }
        let disabled = row.iter().find_map(|line| row_value(line, "disabled:"));
        let check = match disabled {
            Some("true") => MessageCheck::Disabled,
            Some("false") | None => MessageCheck::Enabled,
            Some(_) => MessageCheck::Conditional,
        };
        if preference(check) > preference(state) {
            state = check;
        }
    }
    state
}

/// Splits a dump into rows; a row begins at a `- ` line at column 0.
fn dump_rows(dump: &str) -> Vec<Vec<&str>> {
    let mut rows = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in dump.lines() {
        if line.starts_with("- ") {
            if let Some(row) = current.take() {
                rows.push(row);
            }
            current = Some(vec![line]);
        } else if line.starts_with(char::is_whitespace) {
            if let Some(row) = current.as_mut() {
                row.push(line);
            }
        } else if let Some(row) = current.take() {
            rows.push(row);
        }
    }
    if let Some(row) = current.take() {
        rows.push(row);
    }
    rows
}

/// The value of `key` on a line indented by exactly two spaces, or `None`.
fn row_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = line.strip_prefix("  ")?;
    if rest.starts_with(' ') {
        return None;
    }
    Some(rest.strip_prefix(key)?.trim())
}

/// `value` without one surrounding pair of single or double quotes.
fn unquote(value: &str) -> &str {
    for quote in ['\'', '"'] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

/// How strongly a message check state wins when several rows name the plugin.
fn preference(check: MessageCheck) -> u8 {
    match check {
        MessageCheck::Enabled => 3,
        MessageCheck::Conditional => 2,
        MessageCheck::Disabled => 1,
        MessageCheck::Absent => 0,
    }
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
        let head_before = head_of(runner, &self.config, &task.repository_path)?;
        let branch = branch_of(runner, &self.config, &task.repository_path)?;

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
        if let Some(text) = parsed.turn_failure {
            return Err(HarnessError::Failed(text));
        }
        let Some(final_message) = parsed.final_message else {
            return Err(HarnessError::Failed(
                "dsh exited 0 but printed no final event".to_string(),
            ));
        };

        let changed_files = status_of(runner, &self.config, &task.repository_path)?;
        let head_after = head_of(runner, &self.config, &task.repository_path)?;
        if head_before != head_after {
            return Err(HarnessError::Failed(format!(
                "the agent made a commit although the rules forbid it: head moved from {head_before} to {head_after}"
            )));
        }
        let outcome = match question_line(&final_message) {
            Some(question) => HarnessOutcome::Asked { question },
            None => HarnessOutcome::Finished,
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
            branch,
            head_commit: head_after,
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
            stop_hook_checks_messages: probe_stop_hook(runner, &self.config, version.succeeded),
            question_signal: Capability::Unsupported(
                "prompt-contract: no question event exists; the prompt asks the agent to end with a QUESTION: line and the adapter reads that line"
                    .to_string(),
            ),
        }
    }
}

/// Runs one git command and returns its stdout, or the tool's own failure text.
fn git_stdout(
    runner: &dyn CommandRunner,
    command: &CommandSpec,
    name: &str,
) -> Result<String, HarnessError> {
    let output = runner.run(command)?;
    match &output.outcome {
        CommandOutcome::TimedOut { limit_secs } => Err(HarnessError::Failed(format!(
            "{name} timed out after {limit_secs}s"
        ))),
        CommandOutcome::Exited(0) => Ok(output.stdout.clone()),
        CommandOutcome::Exited(code) => Err(HarnessError::Failed(failure_text(
            name,
            *code,
            &output.stderr,
            &output.stdout,
        ))),
    }
}

/// Runs the clean-tree status command and parses the changed files.
fn status_of(
    runner: &dyn CommandRunner,
    config: &DshConfig,
    repository_path: &str,
) -> Result<Vec<String>, HarnessError> {
    let stdout = git_stdout(
        runner,
        &status_command(config, repository_path),
        "git status",
    )?;
    Ok(parse_changed_files(&stdout))
}

/// Runs the head command and parses the head commit.
fn head_of(
    runner: &dyn CommandRunner,
    config: &DshConfig,
    repository_path: &str,
) -> Result<String, HarnessError> {
    let stdout = git_stdout(
        runner,
        &head_command(config, repository_path),
        "git rev-parse HEAD",
    )?;
    parse_head(&stdout)
        .ok_or_else(|| HarnessError::Failed("git rev-parse HEAD printed nothing".to_string()))
}

/// Runs the branch command and parses the branch.
fn branch_of(
    runner: &dyn CommandRunner,
    config: &DshConfig,
    repository_path: &str,
) -> Result<Option<String>, HarnessError> {
    let stdout = git_stdout(
        runner,
        &branch_command(config, repository_path),
        "git rev-parse --abbrev-ref HEAD",
    )?;
    Ok(parse_branch(&stdout))
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

/// Probes the profile dump for a stop hook that checks messages.
fn probe_stop_hook(
    runner: &dyn CommandRunner,
    config: &DshConfig,
    version_succeeded: bool,
) -> Capability {
    if !version_succeeded {
        return Capability::Unknown(
            "the version probe failed, so the stop hook was not probed".to_string(),
        );
    }
    match runner.run(&probe_command(
        config,
        &["--profile", "headless", "--dump-config"],
    )) {
        Err(error) => Capability::Unknown(error.to_string()),
        Ok(output) => match &output.outcome {
            CommandOutcome::TimedOut { limit_secs } => {
                Capability::Unknown(format!("stop hook probe timed out after {limit_secs}s"))
            }
            CommandOutcome::Exited(0) => match message_check_in(&output.stdout) {
                MessageCheck::Enabled => Capability::Supported,
                MessageCheck::Absent => Capability::Unsupported(
                    "the profile lists no plugin that checks messages at the end of a turn"
                        .to_string(),
                ),
                MessageCheck::Disabled => Capability::Unsupported(
                    "the message check plugin is disabled in the profile".to_string(),
                ),
                MessageCheck::Conditional => Capability::Unknown(
                    "the message check plugin is disabled by an expression the adapter does not evaluate"
                        .to_string(),
                ),
            },
            CommandOutcome::Exited(code) => Capability::Unknown(failure_text(
                "stop hook probe",
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
    const QUESTION_CONTRACT_FIXTURE: &str =
        include_str!("../tests/fixtures/harness/dsh-run-question-contract.jsonl");
    const QUESTION_MARKER_FIXTURE: &str =
        include_str!("../tests/fixtures/harness/dsh-run-question-marker.jsonl");
    const QUESTION_PROSE_QUOTED_FIXTURE: &str =
        include_str!("../tests/fixtures/harness/dsh-run-question-prose-quoted.jsonl");
    const QUESTION_PROSE_STATEMENT_FIXTURE: &str =
        include_str!("../tests/fixtures/harness/dsh-run-question-prose-statement.jsonl");
    const NO_CREDENTIAL_FIXTURE: &str =
        include_str!("../tests/fixtures/harness/dsh-run-no-credential.jsonl");
    const VERSION_FIXTURE: &str = include_str!("../tests/fixtures/harness/dsh-version.txt");
    const HELP_FIXTURE: &str = include_str!("../tests/fixtures/harness/dsh-headless-help.txt");
    const DUMP_CONFIG_FIXTURE: &str = include_str!("../tests/fixtures/harness/dsh-dump-config.yml");
    const ENABLING_ROW: &str = "- id: osf-writing-check\n  name: '@open-software-factory/osf-dsh-plugin'\n  config:\n    command: osf\n";

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
    fn git_env_keeps_only_path_home_and_git_names_in_order() {
        let config = DshConfig::new(
            "dsh",
            vec![
                ("DEEPSEEK_API_KEY".to_string(), "secret".to_string()),
                ("PATH".to_string(), "/bin".to_string()),
                ("UNRELATED".to_string(), "value".to_string()),
                ("NOT_GIT_X".to_string(), "value".to_string()),
                ("GIT_AUTHOR_NAME".to_string(), "someone".to_string()),
                ("HOME".to_string(), "/var/agent-home".to_string()),
            ],
        );
        assert_eq!(
            git_env(&config),
            vec![
                ("PATH".to_string(), "/bin".to_string()),
                ("GIT_AUTHOR_NAME".to_string(), "someone".to_string()),
                ("HOME".to_string(), "/var/agent-home".to_string()),
            ]
        );
    }

    #[test]
    fn git_env_of_an_empty_environment_is_empty() {
        assert!(git_env(&config()).is_empty());
    }

    #[test]
    fn git_commands_carry_only_the_minimal_environment() {
        let config = DshConfig::new(
            "dsh",
            vec![
                ("PATH".to_string(), "/bin".to_string()),
                ("DEEPSEEK_API_KEY".to_string(), "secret".to_string()),
                ("GIT_AUTHOR_NAME".to_string(), "someone".to_string()),
                ("HOME".to_string(), "/var/agent-home".to_string()),
            ],
        );
        let minimal = git_env(&config);
        assert_eq!(status_command(&config, "/repo").env, minimal);
        assert_eq!(head_command(&config, "/repo").env, minimal);
        assert_eq!(branch_command(&config, "/repo").env, minimal);
        assert!(minimal.iter().all(|(name, _)| name != "DEEPSEEK_API_KEY"));
    }

    #[test]
    fn build_command_keeps_the_complete_environment_including_the_key() {
        let env = vec![
            ("PATH".to_string(), "/bin".to_string()),
            ("DEEPSEEK_API_KEY".to_string(), "secret".to_string()),
            ("GIT_AUTHOR_NAME".to_string(), "someone".to_string()),
        ];
        let command = build_command(&DshConfig::new("dsh", env.clone()), "go", "/repo", None);
        assert_eq!(command.env, env);
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
        assert_eq!(command.env, git_env(&config()));
    }

    #[test]
    fn head_command_pins_its_arguments_and_workdir() {
        let command = head_command(&config(), "/repo");
        assert_eq!(command.program, "git");
        assert_eq!(command.args, strings(&["rev-parse", "HEAD"]));
        assert_eq!(command.workdir.as_deref(), Some("/repo"));
        assert_eq!(command.timeout_secs, Some(60));
        assert!(command.stdin.is_none());
        assert_eq!(command.env, git_env(&config()));
    }

    #[test]
    fn branch_command_pins_its_arguments_and_workdir() {
        let command = branch_command(&config(), "/repo");
        assert_eq!(command.program, "git");
        assert_eq!(
            command.args,
            strings(&["rev-parse", "--abbrev-ref", "HEAD"])
        );
        assert_eq!(command.workdir.as_deref(), Some("/repo"));
        assert_eq!(command.timeout_secs, Some(60));
        assert!(command.stdin.is_none());
        assert_eq!(command.env, git_env(&config()));
    }

    #[test]
    fn parse_head_trims_and_rejects_empty_output() {
        assert_eq!(parse_head("abc123\n"), Some("abc123".to_string()));
        assert_eq!(parse_head("  abc123  "), Some("abc123".to_string()));
        assert_eq!(parse_head(""), None);
        assert_eq!(parse_head("  \n"), None);
    }

    #[test]
    fn parse_branch_reads_a_name_and_rejects_detached_and_empty() {
        assert_eq!(parse_branch("main\n"), Some("main".to_string()));
        assert_eq!(parse_branch("feat/x\n"), Some("feat/x".to_string()));
        assert_eq!(parse_branch("HEAD\n"), None);
        assert_eq!(parse_branch("HEAD"), None);
        assert_eq!(parse_branch(""), None);
        assert_eq!(parse_branch("  \n"), None);
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
                cache_read_tokens: 16000,
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
                cache_read_tokens: 5120,
                total_tokens: 5529,
            })
        );
    }

    #[test]
    fn parse_events_usage_fields_sum_to_the_reported_total_on_both_real_fixtures() {
        for (label, fixture) in [
            ("created file", CREATED_FILE_FIXTURE),
            ("question", QUESTION_FIXTURE),
        ] {
            let usage = parse_events(fixture)
                .unwrap_or_else(|error| panic!("{label}: {error}"))
                .usage
                .unwrap_or_else(|| panic!("{label}: the fixture reports usage"));
            assert_eq!(
                usage.input_tokens + usage.output_tokens + usage.cache_read_tokens,
                usage.total_tokens,
                "{label}"
            );
        }
    }

    #[test]
    fn parse_events_reads_a_turn_failure_with_its_code_and_message() {
        let parsed = parse_events(NO_CREDENTIAL_FIXTURE).expect("the fixture parses");
        let text = parsed.turn_failure.expect("the turn is a failure");
        assert!(text.contains("MISSING_CREDENTIAL"), "{text}");
        assert!(text.contains("no API key"), "{text}");
    }

    #[test]
    fn parse_events_finds_no_turn_failure_in_a_completed_run() {
        for (label, fixture) in [
            ("created file", CREATED_FILE_FIXTURE),
            ("question", QUESTION_FIXTURE),
            ("question contract", QUESTION_CONTRACT_FIXTURE),
            ("question marker", QUESTION_MARKER_FIXTURE),
            ("question prose quoted", QUESTION_PROSE_QUOTED_FIXTURE),
            ("question prose statement", QUESTION_PROSE_STATEMENT_FIXTURE),
        ] {
            let parsed = parse_events(fixture).unwrap_or_else(|error| panic!("{label}: {error}"));
            assert_eq!(parsed.turn_failure, None, "{label} must have no failure");
        }
    }

    #[test]
    fn parse_events_reads_a_non_completed_reason_as_a_failure() {
        for (line, expected) in [
            (
                r#"{"type":"status","phase":"turn_end","turn":1,"reason":{"kind":"cancelled"}}"#,
                "the turn ended with reason cancelled",
            ),
            (
                r#"{"type":"status","phase":"turn_end","turn":1,"reason":{"kind":"error","error":{"message":"only message"}}}"#,
                "only message",
            ),
            (
                r#"{"type":"status","phase":"turn_end","turn":1,"reason":{"kind":"error","error":{"code":"ONLY_CODE"}}}"#,
                "ONLY_CODE",
            ),
            (
                r#"{"type":"status","phase":"turn_end","turn":1,"reason":{"kind":"error","error":{}}}"#,
                "the turn ended with reason error",
            ),
        ] {
            let parsed = parse_events(line).expect("the line parses");
            assert_eq!(parsed.turn_failure.as_deref(), Some(expected), "{line}");
        }
    }

    #[test]
    fn parse_events_keeps_the_first_turn_failure() {
        let stdout = concat!(
            r#"{"type":"status","phase":"turn_end","turn":1,"reason":{"kind":"error","error":{"code":"FIRST"}}}"#,
            "\n",
            r#"{"type":"status","phase":"turn_end","turn":1,"reason":{"kind":"error","error":{"code":"SECOND"}}}"#,
            "\n",
        );
        let parsed = parse_events(stdout).expect("the lines parse");
        assert_eq!(parsed.turn_failure.as_deref(), Some("FIRST"));
    }

    #[test]
    fn parse_events_ignores_a_turn_end_without_a_string_reason() {
        let stdout = concat!(
            r#"{"type":"status","phase":"turn_end","turn":1}"#,
            "\n",
            r#"{"type":"status","phase":"turn_end","turn":1,"reason":{}}"#,
            "\n",
            r#"{"type":"status","phase":"turn_end","turn":1,"reason":{"kind":7}}"#,
            "\n",
        );
        let parsed = parse_events(stdout).expect("the lines parse");
        assert_eq!(parsed.turn_failure, None);
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
                cache_read_tokens: 0,
                total_tokens: 7,
            })
        );
    }

    #[test]
    fn parse_events_counts_a_missing_cache_read_figure_as_zero_and_sums_two_steps() {
        let without = "{\"type\":\"status\",\"phase\":\"step_end\",\"usage\":{\"inputTokens\":5,\"totalTokens\":7}}\n";
        let parsed = parse_events(without).expect("the line parses");
        assert_eq!(
            parsed.usage,
            Some(Usage {
                input_tokens: 5,
                output_tokens: 0,
                cache_read_tokens: 0,
                total_tokens: 7,
            })
        );

        let stdout = concat!(
            r#"{"type":"status","phase":"step_end","usage":{"inputTokens":1,"outputTokens":2,"cacheReadTokens":10,"totalTokens":13}}"#,
            "\n",
            r#"{"type":"status","phase":"step_end","usage":{"inputTokens":3,"outputTokens":4,"cacheReadTokens":20,"totalTokens":27}}"#,
            "\n",
        );
        let parsed = parse_events(stdout).expect("the lines parse");
        assert_eq!(
            parsed.usage,
            Some(Usage {
                input_tokens: 4,
                output_tokens: 6,
                cache_read_tokens: 30,
                total_tokens: 40,
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

    /// `DUMP_CONFIG_FIXTURE` with `row` appended after it.
    fn dump_with(row: &str) -> String {
        format!("{DUMP_CONFIG_FIXTURE}{row}")
    }

    #[test]
    fn message_check_in_of_the_stock_fixture_is_absent() {
        assert_eq!(message_check_in(DUMP_CONFIG_FIXTURE), MessageCheck::Absent);
    }

    #[test]
    fn message_check_in_reads_the_enabling_row_as_enabled() {
        assert_eq!(
            message_check_in(&dump_with(ENABLING_ROW)),
            MessageCheck::Enabled
        );
    }

    #[test]
    fn message_check_in_reads_disabled_conditional_quotes_and_deeper_lines() {
        let cases = [
            (
                "disabled true",
                "- id: osf-writing-check\n  name: '@open-software-factory/osf-dsh-plugin'\n  disabled: true\n  config:\n    command: osf\n",
                MessageCheck::Disabled,
            ),
            (
                "conditional expression",
                "- id: osf-writing-check\n  name: '@open-software-factory/osf-dsh-plugin'\n  disabled: !!js process.platform === 'win32'\n  config:\n    command: osf\n",
                MessageCheck::Conditional,
            ),
            (
                "disabled false",
                "- id: osf-writing-check\n  name: '@open-software-factory/osf-dsh-plugin'\n  disabled: false\n  config:\n    command: osf\n",
                MessageCheck::Enabled,
            ),
            (
                "double quoted name",
                "- id: osf-writing-check\n  name: \"@open-software-factory/osf-dsh-plugin\"\n  config:\n    command: osf\n",
                MessageCheck::Enabled,
            ),
            (
                "disabled nested deeper",
                "- id: osf-writing-check\n  name: '@open-software-factory/osf-dsh-plugin'\n  config:\n    disabled: true\n",
                MessageCheck::Enabled,
            ),
            (
                "another plugin alone",
                "- id: claude-code-hooks\n  name: '@deepseek-ai/dsh-hooks-claude-code'\n",
                MessageCheck::Absent,
            ),
        ];
        for (label, row, expected) in cases {
            assert_eq!(message_check_in(&dump_with(row)), expected, "{label}");
        }
    }

    #[test]
    fn message_check_in_of_an_empty_dump_is_absent() {
        assert_eq!(message_check_in(""), MessageCheck::Absent);
    }

    #[test]
    fn message_check_in_prefers_an_enabled_row_over_a_disabled_one() {
        let rows = "- id: first\n  name: '@open-software-factory/osf-dsh-plugin'\n  disabled: true\n- id: second\n  name: '@open-software-factory/osf-dsh-plugin'\n";
        assert_eq!(message_check_in(&dump_with(rows)), MessageCheck::Enabled);
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
