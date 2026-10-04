//! Provider-neutral harness interface: the seam between the factory and an
//! external coding harness. Nothing in this module names a harness, a command
//! or a model vendor.

pub mod fake;

use serde::Serialize;
use std::fmt;

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
    /// that cannot start is [`HarnessError::Failed`] carrying the system's own
    /// text.
    ///
    /// `stdout` and `stderr` are text, so a non-UTF-8 path in tool output is
    /// not carried; widen to bytes when a runner needs it.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError::Failed`] when the process cannot start or the
    /// run itself fails.
    fn run(&self, command: &CommandSpec) -> Result<CommandOutput, HarnessError>;
}

/// One task for a harness to run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HarnessTask {
    pub text: String,
    pub repository_path: String,
    pub timeout_secs: Option<u64>,
}

/// Who ran a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Actor {
    pub harness: String,
    pub model: Option<String>,
    pub model_family: Option<String>,
}

/// Tokens used over a whole run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

/// How a run finished.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HarnessOutcome {
    Finished,
    Asked { question: String },
}

/// Plain data for the journal.
///
/// `changed_files` are repository-relative, sorted, without duplicates.
/// `usage` is the sum over the whole run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HarnessResult {
    pub actor: Actor,
    pub session_id: Option<String>,
    pub final_message: String,
    pub outcome: HarnessOutcome,
    pub exit: i32,
    pub changed_files: Vec<String>,
    pub usage: Option<Usage>,
    pub cost_micro_usd: Option<u64>,
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
pub struct HarnessCapabilities {
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
pub enum HarnessError {
    Rejected(String),
    Failed(String),
}

impl fmt::Display for HarnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(text) | Self::Failed(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for HarnessError {}

/// The seam between the factory and an external coding harness.
pub trait Harness {
    /// Runs one task.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError::Rejected`] when the adapter refuses before it
    /// starts the agent, and [`HarnessError::Failed`] when the run fails.
    fn run(
        &self,
        runner: &dyn CommandRunner,
        task: &HarnessTask,
    ) -> Result<HarnessResult, HarnessError>;

    /// Probes what the harness can do.
    ///
    /// A probe that cannot run is [`Capability::Unknown`], never
    /// [`Capability::Unsupported`], never [`Capability::Supported`].
    fn capabilities(&self, runner: &dyn CommandRunner) -> HarnessCapabilities;
}

/// The three rules every task prompt carries, in order.
const RULES: [&str; 3] = [
    "Edit files only inside the repository.",
    "Never run git commit, git push, git stash or git checkout.",
    "Add no dependency unless the task says so.",
];

/// Builds the harness prompt: `task_text`, then a blank line, then the three
/// rules in order.
#[must_use]
pub fn build_prompt(task_text: &str) -> String {
    format!("{task_text}\n\n{}", RULES.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::fake::{FakeHarness, FakeRunner};

    fn capabilities() -> HarnessCapabilities {
        HarnessCapabilities {
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

    fn task() -> HarnessTask {
        HarnessTask {
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
    fn build_prompt_carries_the_task_and_the_three_rules_in_order() {
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
    fn the_trait_is_object_safe() {
        let fake = FakeHarness::new(Vec::new(), capabilities());
        let dynamic: &dyn Harness = &fake;
        let boxed: Box<dyn Harness> = Box::new(FakeHarness::new(Vec::new(), capabilities()));
        let runner = FakeRunner::new(Vec::new());
        let dynamic_runner: &dyn CommandRunner = &runner;
        assert!(matches!(
            dynamic.run(dynamic_runner, &task()),
            Err(HarnessError::Failed(_))
        ));
        assert_eq!(
            boxed.capabilities(dynamic_runner).binary_present,
            Capability::Supported
        );
        assert!(matches!(
            dynamic_runner.run(&command()),
            Err(HarnessError::Failed(_))
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
            serde_json::to_string(&HarnessOutcome::Finished).expect("serializes"),
            r#""finished""#
        );
        assert_eq!(
            serde_json::to_string(&HarnessOutcome::Asked {
                question: "which?".to_string(),
            })
            .expect("serializes"),
            r#"{"asked":{"question":"which?"}}"#
        );
    }

    #[test]
    fn harness_result_serializes_every_field() {
        let result = HarnessResult {
            actor: Actor {
                harness: "the-harness".to_string(),
                model: Some("the-model".to_string()),
                model_family: Some("the-family".to_string()),
            },
            session_id: Some("session-1".to_string()),
            final_message: "done".to_string(),
            outcome: HarnessOutcome::Finished,
            exit: 0,
            changed_files: vec!["a.rs".to_string()],
            usage: Some(Usage {
                input_tokens: 1,
                output_tokens: 2,
                total_tokens: 3,
            }),
            cost_micro_usd: None,
        };
        assert_eq!(
            serde_json::to_string(&result).expect("serializes"),
            r#"{"actor":{"harness":"the-harness","model":"the-model","model_family":"the-family"},"session_id":"session-1","final_message":"done","outcome":"finished","exit":0,"changed_files":["a.rs"],"usage":{"input_tokens":1,"output_tokens":2,"total_tokens":3},"cost_micro_usd":null}"#
        );
    }

    #[test]
    fn harness_capabilities_serializes_every_field() {
        let caps = HarnessCapabilities {
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
            HarnessError::Rejected("refused".to_string()).to_string(),
            "refused"
        );
        assert_eq!(
            HarnessError::Failed("failed".to_string()).to_string(),
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
}
