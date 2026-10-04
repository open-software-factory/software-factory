//! An in-memory [`Sandbox`] test double: it records every call as plain data and does no I/O.

use crate::sandbox::{
    Capability, CommandSpec, RunOutcome, RunResult, Sandbox, SandboxCapabilities, SandboxError,
    SandboxId, SandboxSpec,
};
use std::cell::RefCell;
use std::collections::VecDeque;

/// One call the double recorded, in the order it happened.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Call {
    Create { spec: SandboxSpec },
    Run { id: SandboxId, command: CommandSpec },
    Destroy { id: SandboxId },
    Capabilities { spec: SandboxSpec },
}

/// The double's operations that can be told to fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Create,
    Run,
    Destroy,
}

/// A [`Sandbox`] that answers from memory and records every call.
pub struct FakeSandbox {
    calls: RefCell<Vec<Call>>,
    failures: Vec<(Operation, SandboxError)>,
    run_results: RefCell<VecDeque<RunResult>>,
    /// The answer [`Sandbox::capabilities`] returns.
    pub capabilities: SandboxCapabilities,
}

impl FakeSandbox {
    /// A double with the documented defaults and no scripted failure.
    #[must_use]
    pub fn new() -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            failures: Vec::new(),
            run_results: RefCell::new(VecDeque::new()),
            capabilities: SandboxCapabilities {
                platforms: vec!["local".to_string()],
                runtime: Capability::Supported,
                image: Capability::Supported,
                network_isolation: Capability::Supported,
                mounts: Capability::Supported,
                stop_mid_run: Capability::Unsupported(
                    "a run stops only by its timeout; no caller-initiated stop".to_string(),
                ),
            },
        }
    }

    /// Makes `operation` fail with `error` after still recording its call.
    #[must_use]
    pub fn fail(mut self, operation: Operation, error: SandboxError) -> Self {
        self.failures.push((operation, error));
        self
    }

    /// Queues `result` as the next answer [`Sandbox::run`] returns.
    #[must_use]
    pub fn with_run_result(self, result: RunResult) -> Self {
        self.run_results.borrow_mut().push_back(result);
        self
    }

    /// Every call recorded so far, in order.
    #[must_use]
    pub fn calls(&self) -> Vec<Call> {
        self.calls.borrow().clone()
    }

    /// The scripted failure for `operation`, if any.
    fn failure(&self, operation: Operation) -> Option<SandboxError> {
        self.failures
            .iter()
            .find(|(failed, _)| *failed == operation)
            .map(|(_, error)| error.clone())
    }
}

impl Default for FakeSandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Sandbox for FakeSandbox {
    fn create(&self, spec: &SandboxSpec) -> Result<SandboxId, SandboxError> {
        self.calls
            .borrow_mut()
            .push(Call::Create { spec: spec.clone() });
        if let Some(error) = self.failure(Operation::Create) {
            return Err(error);
        }
        Ok(SandboxId(format!("fake-{}", spec.name)))
    }

    fn run(&self, id: &SandboxId, command: &CommandSpec) -> Result<RunResult, SandboxError> {
        self.calls.borrow_mut().push(Call::Run {
            id: id.clone(),
            command: command.clone(),
        });
        if let Some(error) = self.failure(Operation::Run) {
            return Err(error);
        }
        let scripted = self.run_results.borrow_mut().pop_front();
        Ok(scripted.unwrap_or(RunResult {
            outcome: RunOutcome::Exited(0),
            stdout: String::new(),
            stderr: String::new(),
        }))
    }

    fn destroy(&self, id: &SandboxId) -> Result<(), SandboxError> {
        self.calls
            .borrow_mut()
            .push(Call::Destroy { id: id.clone() });
        if let Some(error) = self.failure(Operation::Destroy) {
            return Err(error);
        }
        Ok(())
    }

    fn capabilities(&self, spec: &SandboxSpec) -> SandboxCapabilities {
        self.calls
            .borrow_mut()
            .push(Call::Capabilities { spec: spec.clone() });
        self.capabilities.clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::sandbox::fake::{Call, FakeSandbox, Operation};
    use crate::sandbox::{
        Capability, CommandSpec, Network, RunOutcome, RunResult, Sandbox, SandboxError, SandboxId,
        SandboxSpec,
    };

    fn spec() -> SandboxSpec {
        SandboxSpec {
            name: "build".to_string(),
            image: "example/base:1".to_string(),
            user: "dev".to_string(),
            workdir: "/work".to_string(),
            network: Network::Isolated,
            mounts: Vec::new(),
        }
    }

    fn command() -> CommandSpec {
        CommandSpec {
            program: "run".to_string(),
            args: vec!["--fast".to_string()],
            workdir: None,
            timeout_secs: Some(30),
        }
    }

    fn result(stdout: &str) -> RunResult {
        RunResult {
            outcome: RunOutcome::Exited(0),
            stdout: stdout.to_string(),
            stderr: String::new(),
        }
    }

    #[test]
    fn the_double_records_every_call_in_order() {
        let sandbox = FakeSandbox::new();
        let id = sandbox.create(&spec()).expect("creates");
        let _ = sandbox.run(&id, &command()).expect("runs");
        sandbox.destroy(&id).expect("destroys");
        let _ = sandbox.capabilities(&spec());
        assert_eq!(
            sandbox.calls(),
            vec![
                Call::Create { spec: spec() },
                Call::Run {
                    id: id.clone(),
                    command: command(),
                },
                Call::Destroy { id: id.clone() },
                Call::Capabilities { spec: spec() },
            ]
        );
    }

    #[test]
    fn create_names_the_sandbox_after_the_spec() {
        let sandbox = FakeSandbox::new();
        let id = sandbox.create(&spec()).expect("creates");
        assert_eq!(id, SandboxId("fake-build".to_string()));
    }

    #[test]
    fn scripted_run_results_are_returned_in_order_then_the_default() {
        let sandbox = FakeSandbox::new()
            .with_run_result(result("first"))
            .with_run_result(result("second"));
        let id = SandboxId("fake-build".to_string());
        assert_eq!(sandbox.run(&id, &command()).expect("runs").stdout, "first");
        assert_eq!(sandbox.run(&id, &command()).expect("runs").stdout, "second");
        assert_eq!(
            sandbox.run(&id, &command()).expect("runs"),
            RunResult {
                outcome: RunOutcome::Exited(0),
                stdout: String::new(),
                stderr: String::new(),
            }
        );
    }

    #[test]
    fn a_scripted_failure_is_returned_after_the_call_is_recorded() {
        let sandbox = FakeSandbox::new().fail(
            Operation::Run,
            SandboxError::Failed("tool failed".to_string()),
        );
        let id = SandboxId("fake-build".to_string());
        let error = sandbox.run(&id, &command()).expect_err("fails");
        assert_eq!(error, SandboxError::Failed("tool failed".to_string()));
        assert_eq!(
            sandbox.calls(),
            vec![Call::Run {
                id: id.clone(),
                command: command(),
            }]
        );
    }

    #[test]
    fn a_failure_for_one_operation_does_not_affect_another() {
        let sandbox =
            FakeSandbox::new().fail(Operation::Create, SandboxError::Rejected("no".to_string()));
        let error = sandbox.create(&spec()).expect_err("fails");
        assert_eq!(error, SandboxError::Rejected("no".to_string()));
        let id = SandboxId("fake-build".to_string());
        sandbox.destroy(&id).expect("destroys");
    }

    #[test]
    fn the_default_capabilities_support_everything_but_a_mid_run_stop() {
        let sandbox = FakeSandbox::new();
        let capabilities = sandbox.capabilities(&spec());
        assert_eq!(capabilities.platforms, vec!["local".to_string()]);
        assert_eq!(capabilities.runtime, Capability::Supported);
        assert_eq!(capabilities.image, Capability::Supported);
        assert_eq!(capabilities.network_isolation, Capability::Supported);
        assert_eq!(capabilities.mounts, Capability::Supported);
        assert_eq!(
            capabilities.stop_mid_run,
            Capability::Unsupported(
                "a run stops only by its timeout; no caller-initiated stop".to_string()
            )
        );
    }

    #[test]
    fn calls_serialize() {
        assert_eq!(
            serde_json::to_string(&Call::Create { spec: spec() }).expect("serializes"),
            r#"{"create":{"spec":{"name":"build","image":"example/base:1","user":"dev","workdir":"/work","network":"isolated","mounts":[]}}}"#
        );
    }
}
