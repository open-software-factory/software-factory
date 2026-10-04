//! An in-memory [`CommandRunner`] and [`Harness`] test double: they replay
//! scripted answers, record every call as plain data, and start no process.

use crate::harness::{
    CommandOutput, CommandRunner, CommandSpec, Harness, HarnessCapabilities, HarnessError,
    HarnessResult, HarnessTask,
};
use std::cell::RefCell;
use std::collections::VecDeque;

/// A [`CommandRunner`] that replays scripted answers and records every command.
pub struct FakeRunner {
    responses: RefCell<VecDeque<Result<CommandOutput, HarnessError>>>,
    commands: RefCell<Vec<CommandSpec>>,
}

impl FakeRunner {
    /// A double that replays `responses` in order.
    #[must_use]
    pub fn new(responses: Vec<Result<CommandOutput, HarnessError>>) -> Self {
        Self {
            responses: RefCell::new(responses.into()),
            commands: RefCell::new(Vec::new()),
        }
    }

    /// Every command recorded so far, in order.
    #[must_use]
    pub fn commands(&self) -> Vec<CommandSpec> {
        self.commands.borrow().clone()
    }
}

impl CommandRunner for FakeRunner {
    fn run(&self, command: &CommandSpec) -> Result<CommandOutput, HarnessError> {
        self.commands.borrow_mut().push(command.clone());
        let mut responses = self.responses.borrow_mut();
        match responses.pop_front() {
            Some(response) => response,
            None => Err(HarnessError::Failed("no scripted output left".to_string())),
        }
    }
}

/// A [`Harness`] that replays scripted results and records every task.
pub struct FakeHarness {
    results: RefCell<VecDeque<Result<HarnessResult, HarnessError>>>,
    tasks: RefCell<Vec<HarnessTask>>,
    capabilities: HarnessCapabilities,
}

impl FakeHarness {
    /// A double that replays `results` in order and reports `capabilities`.
    #[must_use]
    pub fn new(
        results: Vec<Result<HarnessResult, HarnessError>>,
        capabilities: HarnessCapabilities,
    ) -> Self {
        Self {
            results: RefCell::new(results.into()),
            tasks: RefCell::new(Vec::new()),
            capabilities,
        }
    }

    /// Every task recorded so far, in order.
    #[must_use]
    pub fn tasks(&self) -> Vec<HarnessTask> {
        self.tasks.borrow().clone()
    }
}

impl Harness for FakeHarness {
    fn run(
        &self,
        _runner: &dyn CommandRunner,
        task: &HarnessTask,
    ) -> Result<HarnessResult, HarnessError> {
        self.tasks.borrow_mut().push(task.clone());
        let mut results = self.results.borrow_mut();
        match results.pop_front() {
            Some(result) => result,
            None => Err(HarnessError::Failed("no scripted result left".to_string())),
        }
    }

    fn capabilities(&self, _runner: &dyn CommandRunner) -> HarnessCapabilities {
        self.capabilities.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::{Actor, Capability, CommandOutcome, HarnessOutcome, Usage};

    fn command(program: &str) -> CommandSpec {
        CommandSpec {
            program: program.to_string(),
            args: vec!["--fast".to_string()],
            workdir: None,
            env: Vec::new(),
            stdin: None,
            timeout_secs: Some(30),
        }
    }

    fn output(stdout: &str) -> CommandOutput {
        CommandOutput {
            outcome: CommandOutcome::Exited(0),
            stdout: stdout.to_string(),
            stderr: String::new(),
        }
    }

    fn task(text: &str) -> HarnessTask {
        HarnessTask {
            text: text.to_string(),
            repository_path: "/repo".to_string(),
            timeout_secs: None,
        }
    }

    fn result(message: &str) -> HarnessResult {
        HarnessResult {
            actor: Actor {
                harness: "the-harness".to_string(),
                model: None,
                model_family: None,
            },
            session_id: None,
            final_message: message.to_string(),
            outcome: HarnessOutcome::Finished,
            exit: 0,
            changed_files: Vec::new(),
            branch: Some("main".to_string()),
            head_commit: "abc123".to_string(),
            usage: Some(Usage {
                input_tokens: 1,
                output_tokens: 2,
                total_tokens: 3,
            }),
            cost_micro_usd: None,
        }
    }

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

    #[test]
    fn the_fake_runner_records_commands_and_replays_outputs_in_order() {
        let runner = FakeRunner::new(vec![Ok(output("first")), Ok(output("second"))]);
        assert_eq!(runner.run(&command("one")).expect("runs").stdout, "first");
        assert_eq!(runner.run(&command("two")).expect("runs").stdout, "second");
        assert_eq!(runner.commands(), vec![command("one"), command("two")]);
    }

    #[test]
    fn the_fake_runner_with_nothing_scripted_is_failed() {
        let runner = FakeRunner::new(Vec::new());
        let error = runner.run(&command("one")).expect_err("fails");
        assert_eq!(
            error,
            HarnessError::Failed("no scripted output left".to_string())
        );
    }

    #[test]
    fn the_fake_harness_records_tasks_and_replays_results() {
        let harness = FakeHarness::new(
            vec![Ok(result("first")), Ok(result("second"))],
            capabilities(),
        );
        let runner = FakeRunner::new(Vec::new());
        assert_eq!(
            harness
                .run(&runner, &task("one"))
                .expect("runs")
                .final_message,
            "first"
        );
        assert_eq!(
            harness
                .run(&runner, &task("two"))
                .expect("runs")
                .final_message,
            "second"
        );
        assert_eq!(harness.tasks(), vec![task("one"), task("two")]);
        assert_eq!(harness.capabilities(&runner), capabilities());
        assert!(runner.commands().is_empty(), "the harness calls no runner");
    }

    #[test]
    fn the_fake_harness_with_nothing_scripted_is_failed() {
        let harness = FakeHarness::new(Vec::new(), capabilities());
        let runner = FakeRunner::new(Vec::new());
        let error = harness.run(&runner, &task("one")).expect_err("fails");
        assert_eq!(
            error,
            HarnessError::Failed("no scripted result left".to_string())
        );
        assert!(runner.commands().is_empty(), "the harness calls no runner");
    }
}
