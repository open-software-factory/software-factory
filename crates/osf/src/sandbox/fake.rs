//! An in-memory [`Sandbox`] test double: it records every call as plain data and does no I/O.

use crate::sandbox::validate::{validate_command, validate_spec};
use crate::sandbox::{
    Capability, CommandSpec, Destroyed, RunOutcome, RunResult, Sandbox, SandboxCapabilities,
    SandboxError, SandboxId, SandboxSpec,
};
use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Locks `mutex`, recovering the value even if a previous holder panicked.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

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
    calls: Mutex<Vec<Call>>,
    failures: Vec<(Operation, SandboxError)>,
    run_results: Mutex<VecDeque<RunResult>>,
    created: Mutex<Vec<SandboxId>>,
    /// The answer [`Sandbox::capabilities`] returns.
    pub capabilities: SandboxCapabilities,
}

impl FakeSandbox {
    /// A double with the documented defaults and no scripted failure.
    #[must_use]
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            failures: Vec::new(),
            run_results: Mutex::new(VecDeque::new()),
            created: Mutex::new(Vec::new()),
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
        lock(&self.run_results).push_back(result);
        self
    }

    /// Every call recorded so far, in order.
    #[must_use]
    pub fn calls(&self) -> Vec<Call> {
        lock(&self.calls).clone()
    }

    /// Removes `id` from the ids this double remembers; for tests only.
    pub fn forget(&self, id: &SandboxId) {
        lock(&self.created).retain(|known| known != id);
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
        validate_spec(spec)?;
        lock(&self.calls).push(Call::Create { spec: spec.clone() });
        if let Some(error) = self.failure(Operation::Create) {
            return Err(error);
        }
        let id = SandboxId(format!("fake-{}", spec.name));
        lock(&self.created).push(id.clone());
        Ok(id)
    }

    fn run(&self, id: &SandboxId, command: &CommandSpec) -> Result<RunResult, SandboxError> {
        validate_command(command)?;
        lock(&self.calls).push(Call::Run {
            id: id.clone(),
            command: command.clone(),
        });
        if let Some(error) = self.failure(Operation::Run) {
            return Err(error);
        }
        let scripted = lock(&self.run_results).pop_front();
        Ok(scripted.unwrap_or(RunResult {
            outcome: RunOutcome::Exited(0),
            stdout: String::new(),
            stderr: String::new(),
            output_cap_bytes: None,
            stdout_truncated: false,
            stderr_truncated: false,
        }))
    }

    fn destroy(&self, id: &SandboxId) -> Result<Destroyed, SandboxError> {
        lock(&self.calls).push(Call::Destroy { id: id.clone() });
        if let Some(error) = self.failure(Operation::Destroy) {
            return Err(error);
        }
        let mut created = lock(&self.created);
        match created.iter().position(|known| known == id) {
            Some(index) => {
                created.remove(index);
                Ok(Destroyed::Removed)
            }
            None => Ok(Destroyed::AlreadyGone),
        }
    }

    fn capabilities(&self, spec: &SandboxSpec) -> SandboxCapabilities {
        lock(&self.calls).push(Call::Capabilities { spec: spec.clone() });
        self.capabilities.clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::sandbox::fake::{Call, FakeSandbox, Operation};
    use crate::sandbox::{
        Capability, CommandSpec, Destroyed, Limits, Network, RunOutcome, RunResult, Sandbox,
        SandboxError, SandboxId, SandboxSpec,
    };

    fn spec() -> SandboxSpec {
        SandboxSpec {
            name: "build".to_string(),
            image: "example/base:1".to_string(),
            user: "dev".to_string(),
            workdir: "/work".to_string(),
            network: Network::Isolated,
            mounts: Vec::new(),
            limits: Limits::default(),
        }
    }

    fn command() -> CommandSpec {
        CommandSpec {
            program: "run".to_string(),
            args: vec!["--fast".to_string()],
            workdir: None,
            timeout_secs: Some(30),
            env: std::collections::BTreeMap::new(),
            stdin: None,
        }
    }

    fn result(stdout: &str) -> RunResult {
        RunResult {
            outcome: RunOutcome::Exited(0),
            stdout: stdout.to_string(),
            stderr: String::new(),
            output_cap_bytes: None,
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }

    fn create_rejected(bad: &SandboxSpec) {
        let sandbox = FakeSandbox::new();
        let error = sandbox.create(bad).expect_err("the spec is rejected");
        assert!(matches!(error, SandboxError::Rejected(_)), "{error:?}");
        assert!(
            sandbox.calls().is_empty(),
            "a rejected create is not recorded"
        );
    }

    fn run_rejected(command: &CommandSpec) {
        let sandbox = FakeSandbox::new();
        let id = SandboxId("fake-build".to_string());
        let error = sandbox
            .run(&id, command)
            .expect_err("the command is rejected");
        assert!(matches!(error, SandboxError::Rejected(_)), "{error:?}");
        assert!(sandbox.calls().is_empty(), "a rejected run is not recorded");
    }

    #[test]
    fn create_rejects_a_bad_image_without_recording() {
        let mut bad = spec();
        bad.image = "example/base:latest".to_string();
        create_rejected(&bad);
    }

    #[test]
    fn create_rejects_a_root_user_without_recording() {
        let mut bad = spec();
        bad.user = "root".to_string();
        create_rejected(&bad);
    }

    #[test]
    fn create_rejects_a_zero_process_limit_without_recording() {
        let mut bad = spec();
        bad.limits.max_processes = 0;
        create_rejected(&bad);
    }

    #[test]
    fn run_rejects_a_zero_timeout_without_recording() {
        let mut bad = command();
        bad.timeout_secs = Some(0);
        run_rejected(&bad);
    }

    #[test]
    fn run_rejects_a_bad_env_key_without_recording() {
        let mut bad = command();
        bad.env.insert("1BAD".to_string(), "value".to_string());
        run_rejected(&bad);
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
                output_cap_bytes: None,
                stdout_truncated: false,
                stderr_truncated: false,
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
    fn destroy_removes_a_created_id_then_reports_it_already_gone() {
        let sandbox = FakeSandbox::new();
        let id = sandbox.create(&spec()).expect("creates");
        assert_eq!(
            sandbox.destroy(&id).expect("the first destroy"),
            Destroyed::Removed
        );
        assert_eq!(
            sandbox.destroy(&id).expect("the second destroy"),
            Destroyed::AlreadyGone
        );
    }

    #[test]
    fn destroy_of_an_unknown_id_reports_already_gone() {
        let sandbox = FakeSandbox::new();
        let id = SandboxId("never-created".to_string());
        assert_eq!(
            sandbox.destroy(&id).expect("the destroy"),
            Destroyed::AlreadyGone
        );
    }

    #[test]
    fn an_already_gone_destroy_is_still_recorded() {
        let sandbox = FakeSandbox::new();
        let id = SandboxId("never-created".to_string());
        sandbox.destroy(&id).expect("the first destroy");
        sandbox.destroy(&id).expect("the second destroy");
        assert_eq!(
            sandbox.calls(),
            vec![
                Call::Destroy { id: id.clone() },
                Call::Destroy { id: id.clone() },
            ]
        );
    }

    #[test]
    fn forget_makes_a_created_id_unknown() {
        let sandbox = FakeSandbox::new();
        let id = sandbox.create(&spec()).expect("creates");
        sandbox.forget(&id);
        assert_eq!(
            sandbox.destroy(&id).expect("the destroy"),
            Destroyed::AlreadyGone
        );
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
    fn recorded_run_calls_see_env_and_stdin() {
        let sandbox = FakeSandbox::new();
        let id = SandboxId("fake-build".to_string());
        let mut with_env = command();
        with_env
            .env
            .insert("API_KEY".to_string(), "one".to_string());
        let mut other_env = command();
        other_env
            .env
            .insert("API_KEY".to_string(), "two".to_string());
        let mut with_stdin = command();
        with_stdin.stdin = Some(b"input".to_vec());
        sandbox.run(&id, &with_env).expect("runs");
        sandbox.run(&id, &other_env).expect("runs");
        sandbox.run(&id, &with_stdin).expect("runs");
        let calls = sandbox.calls();
        let first = calls.first().cloned();
        assert_ne!(
            first,
            Some(Call::Run {
                id: id.clone(),
                command: other_env,
            })
        );
        assert_ne!(
            first,
            Some(Call::Run {
                id: id.clone(),
                command: with_stdin,
            })
        );
        assert_eq!(
            first,
            Some(Call::Run {
                id: id.clone(),
                command: with_env,
            })
        );
    }

    #[test]
    fn calls_serialize() {
        assert_eq!(
            serde_json::to_string(&Call::Create { spec: spec() }).expect("serializes"),
            r#"{"create":{"spec":{"name":"build","image":"example/base:1","user":"dev","workdir":"/work","network":"isolated","mounts":[],"limits":{"max_processes":512,"memory":"4g"}}}}"#
        );
    }
}
