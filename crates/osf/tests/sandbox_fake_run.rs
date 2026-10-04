//! A full engine-style run over the in-memory sandbox double: no process, file,
//! network or clock is reached anywhere in this test.

use osf::sandbox::fake::{Call, FakeSandbox, Operation};
use osf::sandbox::{
    Capability, CommandSpec, Destroyed, Mount, Network, RunOutcome, RunResult, Sandbox,
    SandboxError, SandboxId, SandboxSpec,
};

fn spec() -> SandboxSpec {
    SandboxSpec {
        name: "run-1".to_string(),
        image: "example/base:1".to_string(),
        user: "dev".to_string(),
        workdir: "/workspace".to_string(),
        network: Network::Isolated,
        mounts: vec![
            Mount {
                host_path: "/host/worktree".to_string(),
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
}

/// The id the double mints for [`spec`], pinned so the recorded calls can be read.
fn id() -> SandboxId {
    SandboxId("fake-run-1".to_string())
}

fn command(program: &str) -> CommandSpec {
    CommandSpec {
        program: program.to_string(),
        args: vec!["--fast".to_string()],
        workdir: None,
        timeout_secs: Some(30),
    }
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

/// The engine's whole use of the sandbox seam: create, run every command in
/// order, and always destroy afterwards, including when a run failed.
fn run_one(
    sandbox: &dyn Sandbox,
    spec: &SandboxSpec,
    commands: &[CommandSpec],
) -> Result<Vec<RunResult>, SandboxError> {
    let id = sandbox.create(spec)?;
    let mut results = Vec::new();
    let mut run_error = None;
    for command in commands {
        match sandbox.run(&id, command) {
            Ok(result) => results.push(result),
            Err(error) => {
                run_error = Some(error);
                break;
            }
        }
    }
    let destroyed = sandbox.destroy(&id);
    if let Some(error) = run_error {
        return Err(error);
    }
    let _ = destroyed?;
    Ok(results)
}

#[test]
fn a_full_run_creates_runs_every_command_then_destroys() {
    let sandbox = FakeSandbox::new()
        .with_run_result(result(RunOutcome::Exited(0), "first", ""))
        .with_run_result(result(RunOutcome::Exited(0), "second", ""));
    let commands = vec![command("build"), command("test")];
    let results = run_one(&sandbox, &spec(), &commands).expect("runs");
    assert_eq!(
        results,
        vec![
            result(RunOutcome::Exited(0), "first", ""),
            result(RunOutcome::Exited(0), "second", ""),
        ]
    );
    assert_eq!(
        sandbox.calls(),
        vec![
            Call::Create { spec: spec() },
            Call::Run {
                id: id(),
                command: command("build"),
            },
            Call::Run {
                id: id(),
                command: command("test"),
            },
            Call::Destroy { id: id() },
        ]
    );
}

#[test]
fn an_exit_one_is_data_and_the_later_commands_still_run_to_destroy() {
    let sandbox = FakeSandbox::new()
        .with_run_result(result(RunOutcome::Exited(1), "", "tests failed"))
        .with_run_result(result(RunOutcome::Exited(0), "done", ""));
    let commands = vec![command("test"), command("report")];
    let results = run_one(&sandbox, &spec(), &commands).expect("runs");
    assert_eq!(
        results,
        vec![
            result(RunOutcome::Exited(1), "", "tests failed"),
            result(RunOutcome::Exited(0), "done", ""),
        ]
    );
    let runs = sandbox
        .calls()
        .iter()
        .filter(|call| matches!(call, Call::Run { .. }))
        .count();
    assert_eq!(runs, 2, "the later command still runs");
    assert!(
        sandbox
            .calls()
            .iter()
            .any(|call| matches!(call, Call::Destroy { .. })),
        "the sandbox is destroyed"
    );
}

#[test]
fn a_timed_out_run_is_returned_as_data_and_the_sandbox_is_destroyed() {
    let sandbox = FakeSandbox::new().with_run_result(result(
        RunOutcome::TimedOut { limit_secs: 30 },
        "partial",
        "",
    ));
    let commands = vec![command("slow")];
    let results = run_one(&sandbox, &spec(), &commands).expect("runs");
    assert_eq!(
        results,
        vec![result(
            RunOutcome::TimedOut { limit_secs: 30 },
            "partial",
            ""
        )]
    );
    assert!(
        sandbox
            .calls()
            .iter()
            .any(|call| matches!(call, Call::Destroy { .. })),
        "the sandbox is destroyed"
    );
}

#[test]
fn a_rejected_create_returns_the_error_and_records_no_run_or_destroy() {
    let sandbox = FakeSandbox::new().fail(
        Operation::Create,
        SandboxError::Rejected("no such image".to_string()),
    );
    let error = run_one(&sandbox, &spec(), &[command("build")]).expect_err("the create fails");
    assert_eq!(error, SandboxError::Rejected("no such image".to_string()));
    assert_eq!(sandbox.calls(), vec![Call::Create { spec: spec() }]);
}

#[test]
fn a_failed_run_still_destroys_and_returns_the_tool_text() {
    let sandbox = FakeSandbox::new().fail(
        Operation::Run,
        SandboxError::Failed("the tool failed: boom".to_string()),
    );
    let error = run_one(&sandbox, &spec(), &[command("build")]).expect_err("the run fails");
    assert_eq!(
        error,
        SandboxError::Failed("the tool failed: boom".to_string())
    );
    assert!(
        sandbox
            .calls()
            .iter()
            .any(|call| matches!(call, Call::Destroy { .. })),
        "the sandbox is destroyed"
    );
}

#[test]
fn a_failed_destroy_is_returned_when_every_run_succeeded() {
    let sandbox = FakeSandbox::new().fail(
        Operation::Destroy,
        SandboxError::Failed("cannot remove the sandbox".to_string()),
    );
    let error = run_one(&sandbox, &spec(), &[command("build")]).expect_err("the destroy fails");
    assert_eq!(
        error,
        SandboxError::Failed("cannot remove the sandbox".to_string())
    );
    assert!(
        sandbox
            .calls()
            .iter()
            .any(|call| matches!(call, Call::Destroy { .. })),
        "the destroy was attempted"
    );
}

#[test]
fn the_result_list_serializes_to_pinned_journal_json() {
    let sandbox = FakeSandbox::new()
        .with_run_result(result(RunOutcome::Exited(0), "out", ""))
        .with_run_result(result(
            RunOutcome::TimedOut { limit_secs: 5 },
            "",
            "partial",
        ));
    let commands = vec![command("build"), command("slow")];
    let results = run_one(&sandbox, &spec(), &commands).expect("runs");
    assert_eq!(
        serde_json::to_string(&results).expect("serializes"),
        r#"[{"outcome":{"exited":0},"stdout":"out","stderr":"","output_cap_bytes":null,"stdout_truncated":false,"stderr_truncated":false},{"outcome":{"timed-out":{"limit_secs":5}},"stdout":"","stderr":"partial","output_cap_bytes":null,"stdout_truncated":false,"stderr_truncated":false}]"#
    );
}

#[test]
fn a_second_destroy_of_the_same_id_is_already_gone() {
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
fn the_double_reports_its_capabilities_through_the_seam() {
    let sandbox = FakeSandbox::new();
    let dynamic: &dyn Sandbox = &sandbox;
    let capabilities = dynamic.capabilities(&spec());
    assert_eq!(capabilities.platforms, vec!["local".to_string()]);
    assert!(matches!(
        capabilities.stop_mid_run,
        Capability::Unsupported(_)
    ));
}
