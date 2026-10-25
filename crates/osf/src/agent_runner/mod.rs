//! Provider-neutral harness interface: the seam between the factory and an
//! external coding harness. Nothing in this module names a harness, a command
//! or a model vendor.

pub mod fake;

use crate::journal::{JobBlocked, Payload};
use serde::Serialize;
use std::fmt;
use std::io::{Read, Write as _};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// One command to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub workdir: Option<String>,
    /// The complete environment of the child; nothing is inherited.
    pub env: Vec<(String, String)>,
    pub stdin: Option<String>,
    pub timeout_secs: Option<u64>,
}

/// How a command ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutcome {
    Exited(i32),
    TimedOut { limit_secs: u64 },
}

/// What a command produced: its outcome and both streams.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub outcome: CommandOutcome,
    pub stdout: String,
    pub stderr: String,
}

/// Runs one command.
pub trait CommandRunner {
    /// Runs one command.
    ///
    /// The runner runs `command` with exactly its `env`, in its `workdir`, and
    /// with its `stdin` as the child's input. At `timeout_secs` it kills the
    /// whole process tree and answers [`CommandOutcome::TimedOut`]. A process
    /// that cannot start is [`AgentError::CouldNotRun`] when the program is
    /// missing, [`AgentError::Failed`] otherwise, carrying the system's own
    /// text.
    ///
    /// `stdout` and `stderr` are text, so a non-UTF-8 path in tool output is
    /// not carried; widen to bytes when a runner needs it.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::CouldNotRun`] when the program is missing, and
    /// [`AgentError::Failed`] when it cannot start or the run itself fails.
    fn run(&self, command: &CommandSpec) -> Result<CommandOutput, AgentError>;
}

/// The [`CommandRunner`] over a real child process.
///
/// It clears the environment and sets exactly the command's own, starts the
/// child as its own process-group leader on Unix, and kills the whole tree at
/// the deadline. A command with no timeout gets the default, so an unattended
/// run is always bounded.
pub struct RealCommandRunner {
    default_timeout: Duration,
}

impl RealCommandRunner {
    /// The bound for a command that names none: one hour.
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3600);

    /// A runner with the default bound.
    #[must_use]
    pub fn new() -> Self {
        Self {
            default_timeout: Self::DEFAULT_TIMEOUT,
        }
    }

    /// A runner with a different bound for a command that names none.
    #[must_use]
    pub fn with_default_timeout(mut self, timeout: Duration) -> Self {
        self.default_timeout = timeout;
        self
    }

    /// Reads one child pipe to its end on its own thread.
    fn drain<R: Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = reader.read_to_end(&mut bytes);
            bytes
        })
    }
}

impl Default for RealCommandRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRunner for RealCommandRunner {
    fn run(&self, command: &CommandSpec) -> Result<CommandOutput, AgentError> {
        let mut pending = Command::new(&command.program);
        pending
            .args(&command.args)
            .env_clear()
            .envs(command.env.iter().map(|(name, value)| (name, value)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(workdir) = &command.workdir {
            pending.current_dir(workdir);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            pending.process_group(0);
        }
        let mut child = pending.spawn().map_err(|error| {
            let text = format!("cannot start {}: {error}", command.program);
            if error.kind() == std::io::ErrorKind::NotFound {
                AgentError::CouldNotRun(text)
            } else {
                AgentError::Failed(text)
            }
        })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AgentError::Failed("no standard output".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AgentError::Failed("no standard error".to_string()))?;
        let stdout = Self::drain(stdout);
        let stderr = Self::drain(stderr);
        let mut input = Some({
            let mut handle = child
                .stdin
                .take()
                .ok_or_else(|| AgentError::Failed("no standard input".to_string()))?;
            let payload = command.stdin.clone();
            std::thread::spawn(move || match payload {
                Some(text) => handle.write_all(text.as_bytes()),
                None => Ok(()),
            })
        });
        let limit = command
            .timeout_secs
            .map_or(self.default_timeout, Duration::from_secs);
        let deadline = Instant::now() + limit;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = crate::process::kill_tree(&child);
                    let _ = child.kill();
                    let _ = child.wait();
                    if let Some(handle) = input.take() {
                        let _ = handle.join();
                    }
                    // The tree kill closed the pipes; do not join the readers,
                    // so a descendant that survived the kill cannot hold the wait.
                    return Ok(CommandOutput {
                        outcome: CommandOutcome::TimedOut {
                            limit_secs: limit.as_secs(),
                        },
                        stdout: String::new(),
                        stderr: String::new(),
                    });
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(error) => return Err(AgentError::Failed(format!("cannot wait: {error}"))),
            }
        };
        let out = stdout.join().unwrap_or_default();
        let err = stderr.join().unwrap_or_default();
        if let Some(handle) = input.take() {
            let _ = handle.join();
        }
        Ok(CommandOutput {
            outcome: CommandOutcome::Exited(status.code().unwrap_or(-1)),
            stdout: String::from_utf8_lossy(&out).into_owned(),
            stderr: String::from_utf8_lossy(&err).into_owned(),
        })
    }
}

/// One task for a harness to run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentTask {
    pub text: String,
    pub repository_path: String,
    pub timeout_secs: Option<u64>,
}

/// Who ran a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Actor {
    pub agent: String,
    pub model: Option<String>,
    pub model_family: Option<String>,
}

/// Tokens used over a whole run.
///
/// `input_tokens` counts tokens sent that were not read from a cache;
/// `cache_read_tokens` counts tokens the provider read from its cache.
/// `total_tokens` is the figure the harness reports, and it includes the
/// cache-read tokens: input plus output plus cache-read equals it for the
/// harness the adapter reads today. Cache-write tokens are not carried
/// because the recorded runs report none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_tokens: u64,
}

/// How a run finished.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentOutcome {
    Finished,
    Asked { question: String },
}

/// Plain data for the journal.
///
/// `changed_files` are repository-relative, sorted, without duplicates.
/// `branch` is `None` on a detached head. `head_commit` is the repository
/// head after the run, and it equals the head before the run because a run
/// that moved it fails.
/// `usage` is the sum over the whole run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentRunResult {
    pub actor: Actor,
    pub session_id: Option<String>,
    pub final_message: String,
    pub outcome: AgentOutcome,
    pub exit: i32,
    pub changed_files: Vec<String>,
    pub branch: Option<String>,
    pub head_commit: String,
    pub usage: Option<Usage>,
    pub cost_micro_usd: Option<u64>,
}

impl AgentRunResult {
    /// The `job-blocked` event for a run that ended by asking a question.
    #[must_use]
    pub fn blocked_event(&self, job: &str, work_item: Option<&str>) -> Option<Payload> {
        match &self.outcome {
            AgentOutcome::Asked { question } => Some(Payload::JobBlocked(JobBlocked {
                job: job.to_string(),
                work_item: work_item.map(str::to_string),
                cause: "clarification".to_string(),
                question: Some(question.clone()),
            })),
            AgentOutcome::Finished => None,
        }
    }
}

/// Whether one harness operation is available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    Supported,
    Unsupported(String),
    Unknown(String),
}

/// What a harness can do, stated rather than assumed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentCapabilities {
    pub binary_present: Capability,
    pub version_pinned: Capability,
    pub headless: Capability,
    pub structured_result: Capability,
    pub cost_reporting: Capability,
    pub stop_mid_run: Capability,
    pub stop_hook_checks_messages: Capability,
    pub question_signal: Capability,
}

/// A harness operation failed: the adapter refused it, or it could not run.
///
/// `Rejected` means the adapter refused before it started the agent (an empty
/// task, an empty repository path, or a repository with uncommitted changes).
/// `Failed` means something ran and went wrong, or could not run; the text is
/// the tool's own text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentError {
    Rejected(String),
    CouldNotRun(String),
    Failed(String),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(text) | Self::CouldNotRun(text) | Self::Failed(text) => {
                f.write_str(text)
            }
        }
    }
}

impl std::error::Error for AgentError {}

/// The seam between the factory and an external coding harness.
pub trait AgentRunner {
    /// Runs one task.
    ///
    /// A run that ends with no `QUESTION:` line and with an empty
    /// `changed_files` is [`AgentOutcome::Finished`] with an empty change
    /// set; the engine must treat that as failed.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::Rejected`] when the adapter refuses before it
    /// starts the agent, and [`AgentError::Failed`] when the run fails.
    fn run(
        &self,
        runner: &dyn CommandRunner,
        task: &AgentTask,
    ) -> Result<AgentRunResult, AgentError>;

    /// Probes what the harness can do.
    ///
    /// A probe that cannot run is [`Capability::Unknown`], never
    /// [`Capability::Unsupported`], never [`Capability::Supported`].
    fn capabilities(&self, runner: &dyn CommandRunner) -> AgentCapabilities;
}

/// The rules every task prompt carries, in order.
const RULES: [&str; 4] = [
    "Edit files only inside the repository.",
    "Never run git commit, git push, git stash or git checkout.",
    "Add no dependency unless the task says so.",
    "If you need an answer before you can finish, end your final message with one line: QUESTION: <the question>",
];

/// The marker a final message ends with to carry a question.
pub const QUESTION_PREFIX: &str = "QUESTION:";

/// Builds the harness prompt: `task_text`, then a blank line, then the rules
/// in order.
#[must_use]
pub fn build_prompt(task_text: &str) -> String {
    format!("{task_text}\n\n{}", RULES.join("\n"))
}

/// The question text when the last non-blank line of `final_message` carries
/// [`QUESTION_PREFIX`].
///
/// The prefix is case sensitive and the text after it is trimmed. A line
/// before the last non-blank line is ignored, and empty text after the prefix
/// gives `None`.
#[must_use]
pub fn question_line(final_message: &str) -> Option<String> {
    let last = final_message
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())?;
    let question = last.trim().strip_prefix(QUESTION_PREFIX)?.trim();
    (!question.is_empty()).then(|| question.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runner::fake::{FakeAgentRunner, FakeRunner};

    fn capabilities() -> AgentCapabilities {
        AgentCapabilities {
            binary_present: Capability::Supported,
            version_pinned: Capability::Supported,
            headless: Capability::Supported,
            structured_result: Capability::Supported,
            cost_reporting: Capability::Supported,
            stop_mid_run: Capability::Supported,
            stop_hook_checks_messages: Capability::Supported,
            question_signal: Capability::Supported,
        }
    }

    fn task() -> AgentTask {
        AgentTask {
            text: "do the thing".to_string(),
            repository_path: "/repo".to_string(),
            timeout_secs: Some(5),
        }
    }

    fn command() -> CommandSpec {
        CommandSpec {
            program: "the-tool".to_string(),
            args: Vec::new(),
            workdir: None,
            env: Vec::new(),
            stdin: None,
            timeout_secs: None,
        }
    }

    #[test]
    fn build_prompt_carries_the_task_and_the_rules_in_order() {
        let prompt = build_prompt("do the thing");
        assert_eq!(prompt, format!("do the thing\n\n{}", RULES.join("\n")));
        for rule in RULES {
            assert!(prompt.contains(rule), "the prompt must carry {rule}");
        }
    }

    #[test]
    fn build_prompt_is_deterministic() {
        assert_eq!(build_prompt("same"), build_prompt("same"));
    }

    #[test]
    fn question_line_reads_only_the_last_non_blank_line() {
        assert_eq!(
            question_line("QUESTION: which one?"),
            Some("which one?".to_string())
        );
        assert_eq!(
            question_line("some text\n\nQUESTION: which one?"),
            Some("which one?".to_string())
        );
        assert_eq!(
            question_line("some text\nQUESTION: which one?\n\n"),
            Some("which one?".to_string())
        );
    }

    #[test]
    fn question_line_ignores_a_marker_that_is_not_last() {
        assert_eq!(question_line("QUESTION: first\nDone."), None);
    }

    #[test]
    fn question_line_requires_text_after_the_prefix() {
        assert_eq!(question_line("QUESTION:"), None);
        assert_eq!(question_line("QUESTION:   "), None);
    }

    #[test]
    fn question_line_is_case_sensitive_and_trims_the_text() {
        assert_eq!(question_line("question: which one?"), None);
        assert_eq!(
            question_line("  QUESTION:   which one?  "),
            Some("which one?".to_string())
        );
    }

    #[test]
    fn question_line_on_a_message_without_a_marker_is_none() {
        assert_eq!(question_line(""), None);
        assert_eq!(question_line("Done."), None);
    }

    #[test]
    fn the_trait_is_object_safe() {
        let fake = FakeAgentRunner::new(Vec::new(), capabilities());
        let dynamic: &dyn AgentRunner = &fake;
        let boxed: Box<dyn AgentRunner> =
            Box::new(FakeAgentRunner::new(Vec::new(), capabilities()));
        let runner = FakeRunner::new(Vec::new());
        let dynamic_runner: &dyn CommandRunner = &runner;
        assert!(matches!(
            dynamic.run(dynamic_runner, &task()),
            Err(AgentError::Failed(_))
        ));
        assert_eq!(
            boxed.capabilities(dynamic_runner).binary_present,
            Capability::Supported
        );
        assert!(matches!(
            dynamic_runner.run(&command()),
            Err(AgentError::Failed(_))
        ));
    }

    #[test]
    fn capability_serde_names_are_pinned() {
        assert_eq!(
            serde_json::to_string(&Capability::Supported).expect("serializes"),
            r#""supported""#
        );
        assert_eq!(
            serde_json::to_string(&Capability::Unsupported("no".to_string())).expect("serializes"),
            r#"{"unsupported":"no"}"#
        );
        assert_eq!(
            serde_json::to_string(&Capability::Unknown("maybe".to_string())).expect("serializes"),
            r#"{"unknown":"maybe"}"#
        );
    }

    #[test]
    fn harness_outcome_serde_names_are_pinned() {
        assert_eq!(
            serde_json::to_string(&AgentOutcome::Finished).expect("serializes"),
            r#""finished""#
        );
        assert_eq!(
            serde_json::to_string(&AgentOutcome::Asked {
                question: "which?".to_string(),
            })
            .expect("serializes"),
            r#"{"asked":{"question":"which?"}}"#
        );
    }

    #[test]
    fn an_asked_result_becomes_a_blocked_job_event() {
        let mut result = AgentRunResult {
            actor: Actor {
                agent: "the-harness".to_string(),
                model: None,
                model_family: None,
            },
            session_id: None,
            final_message: "QUESTION: Which?".to_string(),
            outcome: AgentOutcome::Finished,
            exit: 0,
            changed_files: Vec::new(),
            branch: None,
            head_commit: "abc123".to_string(),
            usage: None,
            cost_micro_usd: None,
        };
        assert!(result.blocked_event("job-1", None).is_none());
        result.outcome = AgentOutcome::Asked {
            question: "Which?".to_string(),
        };
        let Some(Payload::JobBlocked(blocked)) =
            result.blocked_event("job-1", Some("github:open-software-factory/demo#1"))
        else {
            panic!("an asked result is a blocked job event");
        };
        assert_eq!(blocked.job, "job-1");
        assert_eq!(
            blocked.work_item.as_deref(),
            Some("github:open-software-factory/demo#1")
        );
        assert_eq!(blocked.cause, "clarification");
        assert_eq!(blocked.question.as_deref(), Some("Which?"));
    }

    #[test]
    fn agent_result_serializes_every_field() {
        let result = AgentRunResult {
            actor: Actor {
                agent: "the-agent".to_string(),
                model: Some("the-model".to_string()),
                model_family: Some("the-family".to_string()),
            },
            session_id: Some("session-1".to_string()),
            final_message: "done".to_string(),
            outcome: AgentOutcome::Finished,
            exit: 0,
            changed_files: vec!["a.rs".to_string()],
            branch: Some("main".to_string()),
            head_commit: "abc123".to_string(),
            usage: Some(Usage {
                input_tokens: 1,
                output_tokens: 2,
                cache_read_tokens: 0,
                total_tokens: 3,
            }),
            cost_micro_usd: None,
        };
        assert_eq!(
            serde_json::to_string(&result).expect("serializes"),
            r#"{"actor":{"agent":"the-agent","model":"the-model","model_family":"the-family"},"session_id":"session-1","final_message":"done","outcome":"finished","exit":0,"changed_files":["a.rs"],"branch":"main","head_commit":"abc123","usage":{"input_tokens":1,"output_tokens":2,"cache_read_tokens":0,"total_tokens":3},"cost_micro_usd":null}"#
        );
    }

    #[test]
    fn harness_capabilities_serializes_every_field() {
        let caps = AgentCapabilities {
            binary_present: Capability::Supported,
            version_pinned: Capability::Unsupported("no".to_string()),
            headless: Capability::Unknown("maybe".to_string()),
            structured_result: Capability::Supported,
            cost_reporting: Capability::Unknown("unknown".to_string()),
            stop_mid_run: Capability::Unsupported("no".to_string()),
            stop_hook_checks_messages: Capability::Supported,
            question_signal: Capability::Supported,
        };
        assert_eq!(
            serde_json::to_string(&caps).expect("serializes"),
            r#"{"binary_present":"supported","version_pinned":{"unsupported":"no"},"headless":{"unknown":"maybe"},"structured_result":"supported","cost_reporting":{"unknown":"unknown"},"stop_mid_run":{"unsupported":"no"},"stop_hook_checks_messages":"supported","question_signal":"supported"}"#
        );
    }

    #[test]
    fn harness_error_display_prints_the_inner_text_only() {
        assert_eq!(
            AgentError::Rejected("refused".to_string()).to_string(),
            "refused"
        );
        assert_eq!(
            AgentError::Failed("failed".to_string()).to_string(),
            "failed"
        );
    }

    #[test]
    fn harness_task_serializes_its_fields() {
        assert_eq!(
            serde_json::to_string(&task()).expect("serializes"),
            r#"{"text":"do the thing","repository_path":"/repo","timeout_secs":5}"#
        );
    }

    #[test]
    fn a_missing_program_is_could_not_run() {
        let command = CommandSpec {
            program: "osf-no-such-program-9f1c".to_string(),
            args: Vec::new(),
            workdir: None,
            env: Vec::new(),
            stdin: None,
            timeout_secs: Some(5),
        };
        assert!(matches!(
            RealCommandRunner::new().run(&command),
            Err(AgentError::CouldNotRun(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn the_real_runner_captures_output_and_the_exit_code() {
        let command = CommandSpec {
            program: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf out; printf err >&2; exit 3".to_string(),
            ],
            workdir: None,
            env: Vec::new(),
            stdin: None,
            timeout_secs: Some(10),
        };
        let output = RealCommandRunner::new().run(&command).expect("runs");
        assert_eq!(output.outcome, CommandOutcome::Exited(3));
        assert_eq!(output.stdout, "out");
        assert_eq!(output.stderr, "err");
    }

    #[cfg(unix)]
    #[test]
    fn the_real_runner_passes_stdin_and_the_environment() {
        let command = CommandSpec {
            program: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "cat; printf '|%s' \"$OSF_TEST_VALUE\"".to_string(),
            ],
            workdir: None,
            env: vec![("OSF_TEST_VALUE".to_string(), "set".to_string())],
            stdin: Some("in".to_string()),
            timeout_secs: Some(10),
        };
        let output = RealCommandRunner::new().run(&command).expect("runs");
        assert_eq!(output.outcome, CommandOutcome::Exited(0));
        assert_eq!(output.stdout, "in|set");
    }

    #[cfg(unix)]
    #[test]
    fn the_real_runner_times_out() {
        let command = CommandSpec {
            program: "sh".to_string(),
            args: vec!["-c".to_string(), "sleep 30".to_string()],
            workdir: None,
            env: Vec::new(),
            stdin: None,
            timeout_secs: Some(1),
        };
        let started = Instant::now();
        let output = RealCommandRunner::new().run(&command).expect("runs");
        assert_eq!(output.outcome, CommandOutcome::TimedOut { limit_secs: 1 });
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[cfg(unix)]
    #[test]
    fn a_timeout_kills_the_descendants_too() {
        let command = CommandSpec {
            program: "sh".to_string(),
            args: vec!["-c".to_string(), "sleep 30 & sleep 30".to_string()],
            workdir: None,
            env: Vec::new(),
            stdin: None,
            timeout_secs: Some(1),
        };
        let started = Instant::now();
        let output = RealCommandRunner::new().run(&command).expect("runs");
        assert_eq!(output.outcome, CommandOutcome::TimedOut { limit_secs: 1 });
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[cfg(windows)]
    #[test]
    fn the_real_runner_captures_output_and_the_exit_code_on_windows() {
        let command = CommandSpec {
            program: "cmd".to_string(),
            args: vec![
                "/C".to_string(),
                "echo out & echo err 1>&2 & exit /b 3".to_string(),
            ],
            workdir: None,
            env: Vec::new(),
            stdin: None,
            timeout_secs: Some(10),
        };
        let output = RealCommandRunner::new().run(&command).expect("runs");
        assert_eq!(output.outcome, CommandOutcome::Exited(3));
        assert_eq!(output.stdout.trim_end(), "out");
        assert_eq!(output.stderr.trim_end(), "err");
    }
}
