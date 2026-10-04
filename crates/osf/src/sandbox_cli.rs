//! The host side of `osf sandbox run`: turn a request into a plan, render the
//! dry run, and execute the plan over the [`Sandbox`] seam.

use crate::docker_sandbox::{
    create_argv, exec_argv, remove_argv, start_argv, validate_command, validate_spec,
};
use crate::sandbox::{
    CommandSpec, Destroyed, Mount, Network, RunOutcome, RunResult, Sandbox, SandboxError,
    SandboxSpec,
};
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
            },
            Mount {
                host_path: request.state.to_string_lossy().into_owned(),
                sandbox_path: "/state".to_string(),
                read_only: false,
            },
        ],
    };
    validate_spec(&spec)?;
    let mut words = request.command.iter();
    let program = words.next().cloned().unwrap_or_default();
    let command = CommandSpec {
        program,
        args: words.cloned().collect(),
        workdir: None,
        timeout_secs: request.timeout_secs,
    };
    validate_command(&command)?;
    Ok(Plan { spec, command })
}

/// The dry-run JSON: the exact `create`, `start`, `exec` and `remove` argv,
/// with [`DRY_RUN_ID`] standing in for the container id.
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
        "remove": remove_argv(DRY_RUN_ID),
    });
    let mut text =
        serde_json::to_string_pretty(&object).expect("a JSON value always serialises to JSON");
    text.push('\n');
    text
}

/// Runs the plan over `sandbox`: create, run, then always destroy; a provider
/// error or a failed removal becomes the `osf:` message and its exit code.
#[must_use]
pub fn execute(sandbox: &dyn Sandbox, plan: &Plan) -> Execution {
    let id = match sandbox.create(&plan.spec) {
        Ok(id) => id,
        Err(error) => return refused(&error),
    };
    let run = sandbox.run(&id, &plan.command);
    let destroyed = sandbox.destroy(&id);
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
        notes.push(format!("stdout was cut at {cap} bytes"));
    }
    if result.stderr_truncated {
        notes.push(format!("stderr was cut at {cap} bytes"));
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
                    },
                    Mount {
                        host_path: "/host/state".to_string(),
                        sandbox_path: "/state".to_string(),
                        read_only: false,
                    },
                ],
            }
        );
        assert_eq!(
            plan.command,
            CommandSpec {
                program: "sh".to_string(),
                args: vec!["-c".to_string(), "echo hi".to_string()],
                workdir: None,
                timeout_secs: Some(30),
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
                    "--cap-drop",
                    "ALL",
                    "--security-opt",
                    "no-new-privileges",
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
            Some("stdout was cut at 10 bytes")
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
            Some("stderr was cut at 10 bytes")
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
                "stdout was cut at 5 bytes; stderr was cut at 5 bytes; \
                 the sandbox was already gone when it was removed"
            )
        );
    }
}
