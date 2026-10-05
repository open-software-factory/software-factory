//! The host side of `osf sandbox run`: turn a request into a plan, render the
//! dry run, and execute the plan over the [`Sandbox`] seam.

use crate::docker_sandbox::{
    create_argv, exec_argv, inspect_argv, remove_argv, start_argv, validate_command, validate_spec,
};
use crate::sandbox::{
    CommandSpec, Destroyed, Limits, Mount, Network, RunOutcome, RunResult, Sandbox, SandboxError,
    SandboxId, SandboxSpec,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The container id the dry-run JSON prints in place of a real one.
const DRY_RUN_ID: &str = "<id>";

/// One `osf sandbox run` request, before validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRequest {
    pub image: String,
    pub repo: PathBuf,
    pub state: PathBuf,
    pub network: Network,
    pub user: String,
    pub timeout_secs: Option<u64>,
    pub name: String,
    pub command: Vec<String>,
}

/// The validated sandbox spec and command one run uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub spec: SandboxSpec,
    pub command: CommandSpec,
}

/// What one execution produced; `message` is the text after `osf:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: u8,
    pub message: Option<String>,
}

/// The sandbox one tracked run created, shared with the signal watcher so a
/// stop can find it.
#[derive(Debug, Default)]
pub struct SandboxTracker {
    current: std::sync::Mutex<Option<SandboxId>>,
    stopping: std::sync::atomic::AtomicBool,
    gate: std::sync::Mutex<()>,
}

impl SandboxTracker {
    /// A tracker that has not seen a sandbox yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks the run as stopped by a signal, so the run loop leaves the removal to the stop.
    pub fn begin_stop(&self) {
        self.stopping
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Whether a signal stop has begun.
    #[must_use]
    pub fn is_stopping(&self) -> bool {
        self.stopping.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Holds the removal gate: one removal at a time, or docker reports one already in progress.
    pub fn gate(&self) -> std::sync::MutexGuard<'_, ()> {
        self.gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Records `id` as the sandbox in use.
    pub fn set(&self, id: SandboxId) {
        let mut current = self
            .current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *current = Some(id);
    }

    /// Forgets the recorded sandbox.
    pub fn clear(&self) {
        let mut current = self
            .current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *current = None;
    }

    /// The recorded sandbox, if any.
    #[must_use]
    pub fn current(&self) -> Option<SandboxId> {
        self.current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// Builds the sandbox spec and command, then checks both with the adapter's
/// own validators before any docker call.
///
/// # Errors
/// Returns [`SandboxError::Rejected`] when the adapter refuses the spec or the command.
pub fn plan(request: &RunRequest) -> Result<Plan, SandboxError> {
    let spec = SandboxSpec {
        name: request.name.clone(),
        image: request.image.clone(),
        user: request.user.clone(),
        workdir: "/workspace".to_string(),
        network: request.network.clone(),
        mounts: vec![
            Mount {
                host_path: request.repo.to_string_lossy().into_owned(),
                sandbox_path: "/workspace".to_string(),
                read_only: false,
                follow_symlinks: false,
            },
            Mount {
                host_path: request.state.to_string_lossy().into_owned(),
                sandbox_path: "/state".to_string(),
                read_only: false,
                follow_symlinks: false,
            },
        ],
        limits: Limits::default(),
    };
    validate_spec(&spec)?;
    let mut words = request.command.iter();
    let program = words.next().cloned().unwrap_or_default();
    let command = CommandSpec {
        program,
        args: words.cloned().collect(),
        workdir: None,
        timeout_secs: request.timeout_secs,
        env: BTreeMap::new(),
        stdin: None,
    };
    validate_command(&command)?;
    Ok(Plan { spec, command })
}

/// The dry-run JSON: the exact `create`, `start`, `exec`, `inspect` and
/// `remove` argv, with [`DRY_RUN_ID`] standing in for the container id.
///
/// # Panics
/// Never in practice: the value holds only strings and arrays, so
/// `serde_json` cannot fail on it.
#[must_use]
pub fn dry_run_json(plan: &Plan) -> String {
    let object = serde_json::json!({
        "create": create_argv(&plan.spec),
        "start": start_argv(DRY_RUN_ID),
        "exec": exec_argv(DRY_RUN_ID, &plan.command),
        "inspect": inspect_argv(DRY_RUN_ID),
        "remove": remove_argv(DRY_RUN_ID),
    });
    let mut text =
        serde_json::to_string_pretty(&object).expect("a JSON value always serialises to JSON");
    text.push('\n');
    text
}

/// Runs the plan over `sandbox`: create, run, then always destroy; a provider
/// error or a failed removal becomes the `osf:` message and its exit code.
/// Records the created sandbox in `tracker` while it exists.
#[must_use]
pub fn execute_tracked(sandbox: &dyn Sandbox, plan: &Plan, tracker: &SandboxTracker) -> Execution {
    let id = match sandbox.create(&plan.spec) {
        Ok(id) => id,
        Err(error) => return refused(&error),
    };
    tracker.set(id.clone());
    let run = sandbox.run(&id, &plan.command);
    let gate = tracker.gate();
    if tracker.is_stopping() {
        return Execution {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 125,
            message: Some("stopped by a signal; the stop removes the sandbox".to_string()),
        };
    }
    let destroyed = sandbox.destroy(&id);
    tracker.clear();
    drop(gate);
    let already_gone = matches!(destroyed, Ok(Destroyed::AlreadyGone));
    let result = match run {
        Ok(result) => result,
        Err(error) => {
            let mut execution = refused(&error);
            match &destroyed {
                Err(removal) => {
                    execution.message =
                        Some(format!("{error}; the sandbox was not removed: {removal}"));
                }
                Ok(Destroyed::AlreadyGone) => {
                    execution.message = Some(format!(
                        "{error}; the sandbox was already gone when it was removed"
                    ));
                }
                Ok(Destroyed::Removed) => {}
            }
            return execution;
        }
    };
    let notes = truncation_notes(&result);
    if let Err(error) = &destroyed {
        let mut message = Some(format!("the sandbox was not removed: {error}"));
        for note in &notes {
            append_note(&mut message, note.clone());
        }
        return Execution {
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: 125,
            message,
        };
    }
    let (exit_code, mut message) = match result.outcome {
        RunOutcome::TimedOut { limit_secs } => (
            124,
            Some(format!("the command timed out after {limit_secs} seconds")),
        ),
        RunOutcome::Exited(code) => (clamp(code), None),
    };
    for note in &notes {
        append_note(&mut message, note.clone());
    }
    if already_gone {
        append_note(
            &mut message,
            "the sandbox was already gone when it was removed".to_string(),
        );
    }
    Execution {
        stdout: result.stdout,
        stderr: result.stderr,
        exit_code,
        message,
    }
}

/// Runs the plan over `sandbox` with a tracker no other caller sees.
#[must_use]
pub fn execute(sandbox: &dyn Sandbox, plan: &Plan) -> Execution {
    execute_tracked(sandbox, plan, &SandboxTracker::new())
}

/// Stops the sandbox for `signal`: the tracked id, or the container name
/// `name` when no id was recorded yet.
#[must_use]
pub fn stop_for_signal(
    sandbox: &dyn Sandbox,
    tracker: &SandboxTracker,
    name: &str,
    signal: i32,
) -> Execution {
    tracker.begin_stop();
    let gate = tracker.gate();
    let target = tracker
        .current()
        .unwrap_or_else(|| SandboxId(name.to_string()));
    let destroyed = sandbox.destroy(&target);
    tracker.clear();
    drop(gate);
    let message = match destroyed {
        Ok(Destroyed::Removed) => format!("stopped by signal {signal}; the sandbox was removed"),
        Ok(Destroyed::AlreadyGone) => {
            format!("stopped by signal {signal}; the sandbox was already gone")
        }
        Err(error) => format!("stopped by signal {signal}; the sandbox was not removed: {error}"),
    };
    Execution {
        stdout: String::new(),
        stderr: String::new(),
        exit_code: u8::try_from(128 + signal).unwrap_or(125),
        message: Some(message),
    }
}

/// Runs `plan` over `sandbox`, installing the signal watcher so SIGINT or
/// SIGTERM removes the sandbox before osf exits.
///
/// # Errors
/// Returns the text when the signal handler cannot be installed.
pub fn run_plan(sandbox: std::sync::Arc<dyn Sandbox>, plan: &Plan) -> Result<Execution, String> {
    #[cfg(unix)]
    {
        crate::stop_signal::install()
            .map_err(|error| format!("cannot install the signal handler: {error}"))?;
        let tracker = std::sync::Arc::new(SandboxTracker::new());
        let watcher_sandbox = std::sync::Arc::clone(&sandbox);
        let watcher_tracker = std::sync::Arc::clone(&tracker);
        let name = plan.spec.name.clone();
        let _ = std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if let Some(signal) = crate::stop_signal::take() {
                let execution = stop_for_signal(
                    watcher_sandbox.as_ref(),
                    watcher_tracker.as_ref(),
                    &name,
                    signal,
                );
                if let Some(message) = &execution.message {
                    eprintln!("osf: {message}");
                }
                std::process::exit(i32::from(execution.exit_code));
            }
        });
        let execution = execute_tracked(sandbox.as_ref(), plan, tracker.as_ref());
        // The watcher thread removes the sandbox and exits with the signal's code.
        while tracker.is_stopping() {
            std::thread::park();
        }
        drop(sandbox);
        Ok(execution)
    }
    #[cfg(not(unix))]
    {
        Ok(execute(sandbox.as_ref(), plan))
    }
}

/// Appends `note` to `message`, joined with `; ` when one already exists.
fn append_note(message: &mut Option<String>, note: String) {
    match message {
        Some(existing) => {
            existing.push_str("; ");
            existing.push_str(&note);
        }
        None => *message = Some(note),
    }
}

/// The cut-stream notes for `result`, one per stream that was cut.
fn truncation_notes(result: &RunResult) -> Vec<String> {
    let Some(cap) = result.output_cap_bytes else {
        return Vec::new();
    };
    let mut notes = Vec::new();
    if result.stdout_truncated {
        notes.push(format!(
            "stdout was cut to {cap} bytes (start and end kept)"
        ));
    }
    if result.stderr_truncated {
        notes.push(format!(
            "stderr was cut to {cap} bytes (start and end kept)"
        ));
    }
    notes
}

/// The execution one provider error maps to: 2 for a refusal, else 125.
fn refused(error: &SandboxError) -> Execution {
    let exit_code = match error {
        SandboxError::Rejected(_) => 2,
        SandboxError::Failed(_) => 125,
    };
    Execution {
        stdout: String::new(),
        stderr: String::new(),
        exit_code,
        message: Some(error.to_string()),
    }
}

/// An exit status outside `0..=255` becomes 125.
fn clamp(code: i32) -> u8 {
    u8::try_from(code).unwrap_or(125)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::fake::{Call, FakeSandbox, Operation};
    use crate::sandbox::{Destroyed, RunResult, SandboxCapabilities, SandboxId, SandboxSpec};
    use std::sync::{Arc, Mutex, PoisonError};

    /// A double that forgets the id it just created, so destroy sees it gone.
    struct ForgetfulSandbox(FakeSandbox);

    impl Sandbox for ForgetfulSandbox {
        fn create(&self, spec: &SandboxSpec) -> Result<SandboxId, SandboxError> {
            let id = self.0.create(spec)?;
            self.0.forget(&id);
            Ok(id)
        }

        fn run(&self, id: &SandboxId, command: &CommandSpec) -> Result<RunResult, SandboxError> {
            self.0.run(id, command)
        }

        fn destroy(&self, id: &SandboxId) -> Result<Destroyed, SandboxError> {
            self.0.destroy(id)
        }

        fn capabilities(&self, spec: &SandboxSpec) -> SandboxCapabilities {
            self.0.capabilities(spec)
        }
    }

    /// A double that records the tracker's id at the moment `run` is entered.
    struct TrackingSandbox {
        inner: FakeSandbox,
        tracker: Arc<SandboxTracker>,
        seen: Mutex<Option<SandboxId>>,
    }

    impl Sandbox for TrackingSandbox {
        fn create(&self, spec: &SandboxSpec) -> Result<SandboxId, SandboxError> {
            self.inner.create(spec)
        }

        fn run(&self, id: &SandboxId, command: &CommandSpec) -> Result<RunResult, SandboxError> {
            let seen = self.tracker.current();
            *self.seen.lock().unwrap_or_else(PoisonError::into_inner) = seen;
            self.inner.run(id, command)
        }

        fn destroy(&self, id: &SandboxId) -> Result<Destroyed, SandboxError> {
            self.inner.destroy(id)
        }

        fn capabilities(&self, spec: &SandboxSpec) -> SandboxCapabilities {
            self.inner.capabilities(spec)
        }
    }

    /// The single `Destroy` call `sandbox` recorded.
    fn one_destroy(sandbox: &FakeSandbox) -> Call {
        let mut destroys = sandbox
            .calls()
            .into_iter()
            .filter(|call| matches!(call, Call::Destroy { .. }));
        let first = destroys.next().expect("one destroy");
        assert!(destroys.next().is_none(), "exactly one destroy");
        first
    }

    fn request() -> RunRequest {
        RunRequest {
            image: "example/base:1".to_string(),
            repo: PathBuf::from("/host/repo"),
            state: PathBuf::from("/host/state"),
            network: Network::Isolated,
            user: "dev".to_string(),
            timeout_secs: Some(30),
            name: "osf-sandbox-1".to_string(),
            command: vec!["sh".to_string(), "-c".to_string(), "echo hi".to_string()],
        }
    }

    fn plan_of(request: &RunRequest) -> Plan {
        plan(request).expect("the request plans")
    }

    fn result(outcome: RunOutcome, stdout: &str, stderr: &str) -> RunResult {
        RunResult {
            outcome,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            output_cap_bytes: None,
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }

    #[test]
    fn plan_builds_the_exact_spec_and_command() {
        let plan = plan_of(&request());
        assert_eq!(
            plan.spec,
            SandboxSpec {
                name: "osf-sandbox-1".to_string(),
                image: "example/base:1".to_string(),
                user: "dev".to_string(),
                workdir: "/workspace".to_string(),
                network: Network::Isolated,
                mounts: vec![
                    Mount {
                        host_path: "/host/repo".to_string(),
                        sandbox_path: "/workspace".to_string(),
                        read_only: false,
                        follow_symlinks: false,
                    },
                    Mount {
                        host_path: "/host/state".to_string(),
                        sandbox_path: "/state".to_string(),
                        read_only: false,
                        follow_symlinks: false,
                    },
                ],
                limits: Limits::default(),
            }
        );
        assert_eq!(
            plan.command,
            CommandSpec {
                program: "sh".to_string(),
                args: vec!["-c".to_string(), "echo hi".to_string()],
                workdir: None,
                timeout_secs: Some(30),
                env: BTreeMap::new(),
                stdin: None,
            }
        );
    }

    #[test]
    fn plan_refuses_an_unpinned_image() {
        for image in [
            "example/base:latest",
            "example/base",
            "--user=0:1",
            "-x",
            "sleep",
            "example/Base:1",
        ] {
            let mut request = request();
            request.image = image.to_string();
            assert!(
                matches!(plan(&request), Err(SandboxError::Rejected(_))),
                "{image}"
            );
        }
    }

    #[test]
    fn plan_refuses_root() {
        for user in ["root", "0", "00", "000", "+0", "00:1", "0:0", "root:root"] {
            let mut request = request();
            request.user = user.to_string();
            assert!(
                matches!(plan(&request), Err(SandboxError::Rejected(_))),
                "{user}"
            );
        }
    }

    #[test]
    fn plan_refuses_an_empty_command_program() {
        for command in [Vec::new(), vec![String::new()]] {
            let mut request = request();
            request.command = command;
            assert!(matches!(plan(&request), Err(SandboxError::Rejected(_))));
        }
    }

    #[test]
    fn dry_run_json_pins_every_argv() {
        let plan = plan_of(&request());
        let text = dry_run_json(&plan);
        assert!(text.ends_with('\n'), "{text}");
        let value: serde_json::Value = serde_json::from_str(&text).expect("parses");
        assert_eq!(
            value,
            serde_json::json!({
                "create": [
                    "create",
                    "--name=osf-sandbox-1",
                    "--label=osf.sandbox=osf-sandbox-1",
                    "--env=PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                    "--env=HOME=/tmp",
                    "--env=LANG=C.UTF-8",
                    "--cap-drop",
                    "ALL",
                    "--security-opt",
                    "no-new-privileges",
                    "--init",
                    "--pids-limit",
                    "512",
                    "--memory",
                    "4g",
                    "--user=dev",
                    "--workdir=/workspace",
                    "--network",
                    "none",
                    "--mount=type=bind,source=/host/repo,target=/workspace",
                    "--mount=type=bind,source=/host/state,target=/state",
                    "--",
                    "example/base:1",
                    "sleep",
                    "2147483647",
                ],
                "start": ["start", "--", "<id>"],
                "exec": ["exec", "--", "<id>", "sh", "-c", "echo hi"],
                "inspect": ["inspect", "--type", "container", "--format", "{{.Id}}", "--", "<id>"],
                "remove": ["rm", "--force", "--", "<id>"],
            })
        );
    }

    #[test]
    fn dry_run_exec_keeps_dashed_command_words_after_the_id() {
        let mut request = request();
        request.command = vec!["--user=0:0".to_string(), "-x".to_string()];
        let plan = plan_of(&request);
        let value: serde_json::Value = serde_json::from_str(&dry_run_json(&plan)).expect("parses");
        assert_eq!(
            value.get("exec"),
            Some(&serde_json::json!([
                "exec",
                "--",
                "<id>",
                "--user=0:0",
                "-x"
            ]))
        );
    }

    #[test]
    fn dry_run_exec_names_env_variables_without_their_values() {
        let mut plan = plan_of(&request());
        plan.command
            .env
            .insert("API_KEY".to_string(), "s3cr3t-value".to_string());
        plan.command
            .env
            .insert("OTHER".to_string(), "another-secret".to_string());
        let text = dry_run_json(&plan);
        assert!(!text.contains("s3cr3t-value"), "{text}");
        assert!(!text.contains("another-secret"), "{text}");
        let value: serde_json::Value = serde_json::from_str(&text).expect("parses");
        assert_eq!(
            value.get("exec"),
            Some(&serde_json::json!([
                "exec", "--env", "API_KEY", "--env", "OTHER", "--", "<id>", "sh", "-c", "echo hi",
            ]))
        );
    }

    #[test]
    fn execute_passes_the_exit_status_and_both_streams_through() {
        let sandbox =
            FakeSandbox::new().with_run_result(result(RunOutcome::Exited(0), "out", "err"));
        let execution = execute(&sandbox, &plan_of(&request()));
        assert_eq!(execution.stdout, "out");
        assert_eq!(execution.stderr, "err");
        assert_eq!(execution.exit_code, 0);
        assert_eq!(execution.message, None);
    }

    #[test]
    fn execute_maps_a_nonzero_exit_to_the_same_code() {
        let sandbox = FakeSandbox::new().with_run_result(result(RunOutcome::Exited(7), "", ""));
        let execution = execute(&sandbox, &plan_of(&request()));
        assert_eq!(execution.exit_code, 7);
        assert_eq!(execution.message, None);
    }

    #[test]
    fn execute_clamps_an_out_of_range_exit_to_125() {
        for code in [-1, 256, 300] {
            let sandbox =
                FakeSandbox::new().with_run_result(result(RunOutcome::Exited(code), "", ""));
            let execution = execute(&sandbox, &plan_of(&request()));
            assert_eq!(execution.exit_code, 125, "{code}");
        }
    }

    #[test]
    fn execute_maps_a_timeout_to_124_and_the_message() {
        let sandbox = FakeSandbox::new().with_run_result(result(
            RunOutcome::TimedOut { limit_secs: 5 },
            "partial",
            "",
        ));
        let execution = execute(&sandbox, &plan_of(&request()));
        assert_eq!(execution.exit_code, 124);
        assert_eq!(execution.stdout, "partial");
        assert_eq!(
            execution.message.as_deref(),
            Some("the command timed out after 5 seconds")
        );
    }

    #[test]
    fn execute_maps_a_failed_create_to_125() {
        let sandbox = FakeSandbox::new().fail(
            Operation::Create,
            SandboxError::Failed("cannot create".to_string()),
        );
        let execution = execute(&sandbox, &plan_of(&request()));
        assert_eq!(execution.exit_code, 125);
        assert_eq!(execution.message.as_deref(), Some("cannot create"));
    }

    #[test]
    fn execute_maps_a_rejected_create_to_2() {
        let sandbox = FakeSandbox::new().fail(
            Operation::Create,
            SandboxError::Rejected("no such image".to_string()),
        );
        let execution = execute(&sandbox, &plan_of(&request()));
        assert_eq!(execution.exit_code, 2);
        assert_eq!(execution.message.as_deref(), Some("no such image"));
    }

    #[test]
    fn execute_maps_a_failed_run_to_125_and_still_destroys() {
        let sandbox = FakeSandbox::new().fail(
            Operation::Run,
            SandboxError::Failed("the tool failed: boom".to_string()),
        );
        let execution = execute(&sandbox, &plan_of(&request()));
        assert_eq!(execution.exit_code, 125);
        assert_eq!(execution.message.as_deref(), Some("the tool failed: boom"));
        assert!(sandbox
            .calls()
            .iter()
            .any(|call| matches!(call, Call::Destroy { .. })));
    }

    #[test]
    fn execute_maps_a_failed_destroy_after_a_good_run_to_125() {
        let sandbox = FakeSandbox::new()
            .with_run_result(result(RunOutcome::Exited(0), "out", "err"))
            .fail(
                Operation::Destroy,
                SandboxError::Failed("cannot remove".to_string()),
            );
        let execution = execute(&sandbox, &plan_of(&request()));
        assert_eq!(execution.exit_code, 125);
        assert_eq!(execution.stdout, "out");
        assert_eq!(execution.stderr, "err");
        assert_eq!(
            execution.message.as_deref(),
            Some("the sandbox was not removed: cannot remove")
        );
    }

    #[test]
    fn execute_reports_a_failed_run_and_a_failed_destroy_together() {
        let sandbox = FakeSandbox::new()
            .fail(
                Operation::Run,
                SandboxError::Failed("run broke".to_string()),
            )
            .fail(
                Operation::Destroy,
                SandboxError::Failed("cannot remove".to_string()),
            );
        let execution = execute(&sandbox, &plan_of(&request()));
        assert_eq!(execution.exit_code, 125);
        assert_eq!(
            execution.message.as_deref(),
            Some("run broke; the sandbox was not removed: cannot remove")
        );
    }

    #[test]
    fn execute_records_create_run_destroy_in_order() {
        let sandbox = FakeSandbox::new().with_run_result(result(RunOutcome::Exited(0), "", ""));
        let plan = plan_of(&request());
        let _ = execute(&sandbox, &plan);
        assert_eq!(
            sandbox.calls(),
            vec![
                Call::Create { spec: plan.spec },
                Call::Run {
                    id: SandboxId("fake-osf-sandbox-1".to_string()),
                    command: plan.command,
                },
                Call::Destroy {
                    id: SandboxId("fake-osf-sandbox-1".to_string()),
                },
            ]
        );
    }

    #[test]
    fn execute_notes_an_already_gone_destroy() {
        let sandbox = ForgetfulSandbox(FakeSandbox::new());
        let execution = execute(&sandbox, &plan_of(&request()));
        assert_eq!(execution.exit_code, 0);
        assert_eq!(
            execution.message.as_deref(),
            Some("the sandbox was already gone when it was removed")
        );
    }

    #[test]
    fn execute_notes_a_cut_stdout() {
        let sandbox = FakeSandbox::new().with_run_result(RunResult {
            outcome: RunOutcome::Exited(0),
            stdout: "out".to_string(),
            stderr: String::new(),
            output_cap_bytes: Some(10),
            stdout_truncated: true,
            stderr_truncated: false,
        });
        let execution = execute(&sandbox, &plan_of(&request()));
        assert_eq!(execution.exit_code, 0);
        assert_eq!(
            execution.message.as_deref(),
            Some("stdout was cut to 10 bytes (start and end kept)")
        );
    }

    #[test]
    fn execute_notes_a_cut_stderr() {
        let sandbox = FakeSandbox::new().with_run_result(RunResult {
            outcome: RunOutcome::Exited(0),
            stdout: String::new(),
            stderr: "err".to_string(),
            output_cap_bytes: Some(10),
            stdout_truncated: false,
            stderr_truncated: true,
        });
        let execution = execute(&sandbox, &plan_of(&request()));
        assert_eq!(execution.exit_code, 0);
        assert_eq!(
            execution.message.as_deref(),
            Some("stderr was cut to 10 bytes (start and end kept)")
        );
    }

    #[test]
    fn execute_orders_the_truncation_notes_before_the_already_gone_note() {
        let sandbox = ForgetfulSandbox(FakeSandbox::new().with_run_result(RunResult {
            outcome: RunOutcome::Exited(0),
            stdout: "out".to_string(),
            stderr: "err".to_string(),
            output_cap_bytes: Some(5),
            stdout_truncated: true,
            stderr_truncated: true,
        }));
        let execution = execute(&sandbox, &plan_of(&request()));
        assert_eq!(
            execution.message.as_deref(),
            Some(
                "stdout was cut to 5 bytes (start and end kept); \
                 stderr was cut to 5 bytes (start and end kept); \
                 the sandbox was already gone when it was removed"
            )
        );
    }

    #[test]
    fn execute_tracked_holds_the_id_while_it_runs_and_clears_it_after() {
        let tracker = Arc::new(SandboxTracker::new());
        let sandbox = TrackingSandbox {
            inner: FakeSandbox::new(),
            tracker: Arc::clone(&tracker),
            seen: Mutex::new(None),
        };
        let execution = execute_tracked(&sandbox, &plan_of(&request()), tracker.as_ref());
        assert_eq!(execution.exit_code, 0);
        assert_eq!(
            *sandbox.seen.lock().unwrap_or_else(PoisonError::into_inner),
            Some(SandboxId("fake-osf-sandbox-1".to_string()))
        );
        assert_eq!(tracker.current(), None);
    }

    #[test]
    fn stop_for_signal_destroys_the_tracked_id_and_reports_removed() {
        for (signal, code) in [(15_i32, 143_u8), (2_i32, 130_u8)] {
            let sandbox = FakeSandbox::new();
            let id = sandbox
                .create(&plan_of(&request()).spec)
                .expect("the fake creates");
            let tracker = SandboxTracker::new();
            tracker.set(id.clone());
            let execution = stop_for_signal(&sandbox, &tracker, "fallback", signal);
            assert_eq!(execution.exit_code, code, "{signal}");
            let expected = format!("stopped by signal {signal}; the sandbox was removed");
            assert_eq!(execution.message.as_deref(), Some(expected.as_str()));
            assert_eq!(execution.stdout, "");
            assert_eq!(execution.stderr, "");
            assert_eq!(one_destroy(&sandbox), Call::Destroy { id });
            assert_eq!(tracker.current(), None);
        }
    }

    #[test]
    fn stop_for_signal_without_an_id_destroys_the_name() {
        let sandbox = FakeSandbox::new();
        let tracker = SandboxTracker::new();
        let execution = stop_for_signal(&sandbox, &tracker, "named-sandbox", 15);
        assert_eq!(execution.exit_code, 143);
        assert_eq!(
            one_destroy(&sandbox),
            Call::Destroy {
                id: SandboxId("named-sandbox".to_string()),
            }
        );
        assert!(execution
            .message
            .as_deref()
            .is_some_and(|message| message.contains("already gone")));
        assert_eq!(tracker.current(), None);
    }

    #[test]
    fn stop_for_signal_reports_an_unknown_id_as_already_gone() {
        let sandbox = FakeSandbox::new();
        let tracker = SandboxTracker::new();
        let id = SandboxId("never-created".to_string());
        tracker.set(id.clone());
        let execution = stop_for_signal(&sandbox, &tracker, "fallback", 15);
        assert_eq!(execution.exit_code, 143);
        assert_eq!(
            execution.message.as_deref(),
            Some("stopped by signal 15; the sandbox was already gone")
        );
        assert_eq!(one_destroy(&sandbox), Call::Destroy { id });
        assert_eq!(tracker.current(), None);
    }

    #[test]
    fn stop_for_signal_reports_a_failed_removal_and_keeps_the_signal_code() {
        let sandbox = FakeSandbox::new().fail(
            Operation::Destroy,
            SandboxError::Failed("cannot remove".to_string()),
        );
        let tracker = SandboxTracker::new();
        let id = SandboxId("abc".to_string());
        tracker.set(id.clone());
        let execution = stop_for_signal(&sandbox, &tracker, "fallback", 15);
        assert_eq!(execution.exit_code, 143);
        assert_eq!(
            execution.message.as_deref(),
            Some("stopped by signal 15; the sandbox was not removed: cannot remove")
        );
        assert_eq!(one_destroy(&sandbox), Call::Destroy { id });
        assert_eq!(tracker.current(), None);
    }

    #[test]
    fn stop_for_signal_marks_the_tracker_as_stopping() {
        let sandbox = FakeSandbox::new();
        let tracker = SandboxTracker::new();
        assert!(!tracker.is_stopping());
        let _ = stop_for_signal(&sandbox, &tracker, "named", 15);
        assert!(tracker.is_stopping());
    }

    #[test]
    fn a_run_that_ends_after_a_stop_began_leaves_the_removal_to_the_stop() {
        let sandbox = FakeSandbox::new();
        let tracker = SandboxTracker::new();
        tracker.begin_stop();
        let execution = execute_tracked(&sandbox, &plan_of(&request()), &tracker);
        assert_eq!(execution.exit_code, 125);
        assert!(execution
            .message
            .as_deref()
            .is_some_and(|message| message.contains("stopped by a signal")));
        let destroys = sandbox
            .calls()
            .into_iter()
            .filter(|call| matches!(call, Call::Destroy { .. }))
            .count();
        assert_eq!(destroys, 0);
    }
}
