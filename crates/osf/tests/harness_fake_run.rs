//! A full run and capability report through the neutral harness interface,
//! with the in-memory fakes. The only concrete adapter named here is `dsh`.

use osf::dsh_harness::{
    parse_events, probe_command, status_command, DshConfig, DshHarness, PINNED_VERSION,
};
use osf::harness::fake::{FakeHarness, FakeRunner};
use osf::harness::{
    build_prompt, Actor, Capability, CommandOutcome, CommandOutput, CommandRunner, Harness,
    HarnessCapabilities, HarnessError, HarnessOutcome, HarnessResult, HarnessTask, Usage,
};

const CREATED_FILE: &str = include_str!("fixtures/harness/dsh-run-created-file.jsonl");
const QUESTION: &str = include_str!("fixtures/harness/dsh-run-question.jsonl");
const QUESTION_PROSE_STATEMENT: &str =
    include_str!("fixtures/harness/dsh-run-question-prose-statement.jsonl");
const QUESTION_PROSE_QUOTED: &str =
    include_str!("fixtures/harness/dsh-run-question-prose-quoted.jsonl");
const QUESTION_MARKER: &str = include_str!("fixtures/harness/dsh-run-question-marker.jsonl");
const QUESTION_CONTRACT: &str = include_str!("fixtures/harness/dsh-run-question-contract.jsonl");
const QUESTION_MARKER_TEXT: &str = "Which colour should `hello.txt` mention, red or blue?";
const QUESTION_SIGNAL_REASON: &str = "prompt-contract: no question event exists; the prompt asks the agent to end with a QUESTION: line and the adapter reads that line";
const VERSION: &str = include_str!("fixtures/harness/dsh-version.txt");
const HELP: &str = include_str!("fixtures/harness/dsh-headless-help.txt");

/// A successful command (exited 0, empty stderr); the script holds `Result`s.
#[allow(clippy::unnecessary_wraps)]
fn ok(stdout: &str) -> Result<CommandOutput, HarnessError> {
    Ok(CommandOutput {
        outcome: CommandOutcome::Exited(0),
        stdout: stdout.to_string(),
        stderr: String::new(),
    })
}

/// An exited command with `stderr` and no stdout; the script holds `Result`s.
#[allow(clippy::unnecessary_wraps)]
fn exited(code: i32, stderr: &str) -> Result<CommandOutput, HarnessError> {
    Ok(CommandOutput {
        outcome: CommandOutcome::Exited(code),
        stdout: String::new(),
        stderr: stderr.to_string(),
    })
}

/// A timed-out command; the script holds `Result`s.
#[allow(clippy::unnecessary_wraps)]
fn timed_out(limit_secs: u64) -> Result<CommandOutput, HarnessError> {
    Ok(CommandOutput {
        outcome: CommandOutcome::TimedOut { limit_secs },
        stdout: String::new(),
        stderr: String::new(),
    })
}

/// A clean repository status.
fn clean() -> Result<CommandOutput, HarnessError> {
    ok("")
}

/// A status holding one new file.
fn one_new_file() -> Result<CommandOutput, HarnessError> {
    ok("?? hello.txt\0")
}

/// The script of one successful run: clean, created-file fixture, one file.
fn successful_script() -> Vec<Result<CommandOutput, HarnessError>> {
    vec![clean(), ok(CREATED_FILE), one_new_file()]
}

/// The adapter config: a program and a complete child environment.
fn config() -> DshConfig {
    DshConfig::new(
        "dsh",
        vec![
            ("PATH".into(), "/usr/bin".into()),
            ("HOME".into(), "/var/agent-home".into()),
        ],
    )
}

/// The adapter under test.
fn harness() -> DshHarness {
    DshHarness::new(config())
}

/// The task every run in this file uses.
fn task() -> HarnessTask {
    HarnessTask {
        text: "Create hello.txt".to_string(),
        repository_path: "/repo".to_string(),
        timeout_secs: Some(600),
    }
}

/// `values` as owned strings.
fn strings(values: &[&str]) -> Vec<String> {
    values.iter().copied().map(String::from).collect()
}

/// The final message text carried by a recorded run fixture.
fn final_message(fixture: &str) -> String {
    parse_events(fixture)
        .expect("the fixture parses")
        .final_message
        .expect("the fixture has a final message")
}

/// The text inside a `Failed` error; any other variant fails the test.
fn failed(error: HarnessError) -> String {
    match error {
        HarnessError::Failed(text) => text,
        other @ HarnessError::Rejected(_) => panic!("expected Failed, got {other:?}"),
    }
}

/// Every capability supported, for constructing a fake.
fn all_supported() -> HarnessCapabilities {
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

/// Runs the shared task through the trait objects only.
fn run_one(harness: &dyn Harness, runner: &dyn CommandRunner) -> HarnessResult {
    harness.run(runner, &task()).expect("the run succeeds")
}

/// The source of `run_one`, from its declaration to its closing brace.
fn run_one_source(source: &str) -> &str {
    let start = source.find("\nfn run_one(").expect("run_one is declared");
    let rest = &source[start + 1..];
    let end = rest.find("\n}").expect("run_one closes");
    &rest[..end]
}

#[test]
fn a_created_file_run_reports_the_file_the_message_and_usage() {
    let runner = FakeRunner::new(successful_script());
    let result = harness().run(&runner, &task()).expect("the run succeeds");
    assert_eq!(result.changed_files, strings(&["hello.txt"]));
    assert_eq!(result.final_message, final_message(CREATED_FILE));
    assert_eq!(
        result.session_id.as_deref(),
        Some("session-51e234bb-b776-44fd-89d7-e846db4cdaab")
    );
    assert_eq!(
        result.usage,
        Some(Usage {
            input_tokens: 789,
            output_tokens: 184,
            total_tokens: 16973,
        })
    );
    assert_eq!(result.outcome, HarnessOutcome::Finished);
    assert_eq!(result.exit, 0);
    assert_eq!(result.cost_micro_usd, None);
    assert_eq!(result.actor.harness, "dsh");
    assert_eq!(result.actor.model, None);
    assert_eq!(result.actor.model_family.as_deref(), Some("DeepSeek"));
}

#[test]
fn a_final_message_without_a_question_line_is_finished_even_when_it_asks() {
    for (label, fixture) in [
        ("question mark", QUESTION),
        ("prose statement", QUESTION_PROSE_STATEMENT),
        ("prose quoted", QUESTION_PROSE_QUOTED),
    ] {
        let runner = FakeRunner::new(vec![clean(), ok(fixture), clean()]);
        let result = harness().run(&runner, &task()).expect("the run succeeds");
        assert_eq!(
            result.outcome,
            HarnessOutcome::Finished,
            "{label} must not be reported as asked"
        );
        assert!(
            result.changed_files.is_empty(),
            "{label} must report no changed file"
        );
    }
}

#[test]
fn a_question_line_is_asked_with_no_changed_file() {
    let runner = FakeRunner::new(vec![clean(), ok(QUESTION_MARKER), clean()]);
    let result = harness().run(&runner, &task()).expect("the run succeeds");
    assert_eq!(
        result.outcome,
        HarnessOutcome::Asked {
            question: QUESTION_MARKER_TEXT.to_string(),
        }
    );
    assert!(result.changed_files.is_empty());
}

#[test]
fn a_real_run_that_followed_the_question_rule_is_asked() {
    let runner = FakeRunner::new(vec![clean(), ok(QUESTION_CONTRACT), clean()]);
    let result = harness().run(&runner, &task()).expect("the run succeeds");
    assert_eq!(
        result.outcome,
        HarnessOutcome::Asked {
            question: "Which colour do you prefer, red or blue?".to_string(),
        }
    );
}

#[test]
fn a_question_line_that_changed_a_file_is_still_asked_and_reports_the_file() {
    let runner = FakeRunner::new(vec![clean(), ok(QUESTION_MARKER), one_new_file()]);
    let result = harness().run(&runner, &task()).expect("the run succeeds");
    assert_eq!(
        result.outcome,
        HarnessOutcome::Asked {
            question: QUESTION_MARKER_TEXT.to_string(),
        }
    );
    assert_eq!(result.changed_files, strings(&["hello.txt"]));
}

#[test]
fn a_non_zero_exit_is_failed_with_the_tool_stderr() {
    let runner = FakeRunner::new(vec![clean(), exited(3, "boom\n")]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert_eq!(error, HarnessError::Failed("boom".to_string()));
    assert_eq!(runner.commands().len(), 2, "the status and the agent only");
}

#[test]
fn a_non_zero_exit_with_no_text_is_failed_with_the_status() {
    let runner = FakeRunner::new(vec![clean(), exited(4, "")]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert_eq!(
        error,
        HarnessError::Failed("dsh exited with status 4".to_string())
    );
}

#[test]
fn a_timeout_is_failed_and_names_the_limit() {
    let runner = FakeRunner::new(vec![clean(), timed_out(7)]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert_eq!(
        error,
        HarnessError::Failed("dsh timed out after 7s".to_string())
    );
}

#[test]
fn exit_zero_with_no_final_event_is_failed() {
    let first_line = CREATED_FILE.lines().next().expect("the fixture has a line");
    let runner = FakeRunner::new(vec![clean(), ok(first_line)]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert!(failed(error).contains("no final event"));
}

#[test]
fn a_non_json_line_is_failed_with_its_line_number() {
    let runner = FakeRunner::new(vec![clean(), ok("oops\n")]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert_eq!(failed(error), "line 1 is not a JSON object: oops");
}

#[test]
fn a_failed_status_before_the_run_is_failed_with_its_text() {
    let runner = FakeRunner::new(vec![exited(128, "fatal: not a git repository\n")]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert_eq!(
        error,
        HarnessError::Failed("fatal: not a git repository".to_string())
    );
    assert_eq!(runner.commands().len(), 1);
}

#[test]
fn a_failed_status_after_the_run_is_failed_with_its_text() {
    let runner = FakeRunner::new(vec![
        clean(),
        ok(CREATED_FILE),
        exited(128, "fatal: broken index\n"),
    ]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert_eq!(failed(error), "fatal: broken index".to_string());
}

#[test]
fn a_runner_error_passes_through_unchanged() {
    let runner = FakeRunner::new(vec![Err(HarnessError::Failed("cannot start".to_string()))]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert_eq!(error, HarnessError::Failed("cannot start".to_string()));
}

#[test]
fn a_dirty_repository_is_rejected_with_the_file_names() {
    let runner = FakeRunner::new(vec![ok("?? a.txt\0 M b.txt\0")]);
    let error = harness()
        .run(&runner, &task())
        .expect_err("the run is rejected");
    assert_eq!(
        error,
        HarnessError::Rejected("the repository has uncommitted changes: a.txt, b.txt".to_string())
    );
    assert_eq!(runner.commands().len(), 1);
}

#[test]
fn an_empty_repository_path_is_rejected() {
    let runner = FakeRunner::new(Vec::new());
    let empty = HarnessTask {
        text: "Create hello.txt".to_string(),
        repository_path: String::new(),
        timeout_secs: Some(600),
    };
    let error = harness()
        .run(&runner, &empty)
        .expect_err("the run is rejected");
    assert_eq!(
        error,
        HarnessError::Rejected("the repository path is empty".to_string())
    );
    assert!(runner.commands().is_empty());
}

#[test]
fn the_commands_reaching_the_runner_are_the_documented_ones() {
    let runner = FakeRunner::new(successful_script());
    harness().run(&runner, &task()).expect("the run succeeds");
    let commands = runner.commands();
    assert_eq!(commands.len(), 3, "status, agent, status");

    let status = status_command(&config(), "/repo");
    assert_eq!(commands.first().expect("the status before"), &status);
    assert_eq!(commands.get(2).expect("the status after"), &status);

    let agent = commands.get(1).expect("the agent command");
    assert_eq!(agent.program, "dsh");
    assert_eq!(
        agent.args,
        strings(&["--profile", "headless", "--json", "-"])
    );
    let prompt = build_prompt("Create hello.txt");
    assert_eq!(agent.stdin.as_deref(), Some(prompt.as_str()));
    assert_eq!(agent.workdir.as_deref(), Some("/repo"));
    assert_eq!(agent.timeout_secs, Some(600));
    assert_eq!(agent.env, config().env);

    for command in &commands {
        assert_eq!(command.env, config().env, "every command carries the env");
        assert_eq!(
            command.workdir.as_deref(),
            Some("/repo"),
            "every command runs in the workdir"
        );
    }
}

#[test]
fn the_result_is_plain_data_for_the_journal() {
    let runner = FakeRunner::new(successful_script());
    let result = harness().run(&runner, &task()).expect("the run succeeds");
    let serialized = serde_json::to_string(&result).expect("the result serializes");
    let value: serde_json::Value = serde_json::from_str(&serialized).expect("the result parses");
    assert_eq!(
        value
            .get("actor")
            .and_then(|actor| actor.get("harness"))
            .and_then(serde_json::Value::as_str),
        Some("dsh")
    );
    assert_eq!(
        value.get("session_id").and_then(serde_json::Value::as_str),
        Some("session-51e234bb-b776-44fd-89d7-e846db4cdaab")
    );
    assert_eq!(
        value.get("changed_files").cloned(),
        Some(serde_json::json!(["hello.txt"]))
    );
    assert_eq!(
        value
            .get("usage")
            .and_then(|usage| usage.get("total_tokens"))
            .and_then(serde_json::Value::as_u64),
        Some(16973)
    );
    assert_eq!(
        value.get("outcome").and_then(serde_json::Value::as_str),
        Some("finished")
    );
    assert!(value
        .get("cost_micro_usd")
        .is_some_and(serde_json::Value::is_null));
}

#[test]
fn capability_report_for_a_pinned_installed_harness() {
    let runner = FakeRunner::new(vec![ok(VERSION), ok(HELP)]);
    let capabilities = harness().capabilities(&runner);
    assert_eq!(capabilities.binary_present, Capability::Supported);
    assert_eq!(capabilities.version_pinned, Capability::Supported);
    assert_eq!(capabilities.headless, Capability::Supported);
    assert_eq!(capabilities.structured_result, Capability::Supported);
    assert!(matches!(
        capabilities.cost_reporting,
        Capability::Unsupported(_)
    ));
    assert!(matches!(
        capabilities.stop_mid_run,
        Capability::Unsupported(_)
    ));
    assert!(matches!(
        capabilities.stop_hook_checks_messages,
        Capability::Unsupported(_)
    ));
    assert_eq!(
        capabilities.question_signal,
        Capability::Unsupported(QUESTION_SIGNAL_REASON.to_string())
    );

    let commands = runner.commands();
    assert_eq!(commands.len(), 2);
    assert_eq!(
        commands.first().expect("the version probe"),
        &probe_command(&config(), &["--version"])
    );
    assert_eq!(
        commands.get(1).expect("the help probe"),
        &probe_command(&config(), &["--profile", "headless", "--help"])
    );
}

#[test]
fn capability_report_when_the_binary_cannot_start() {
    let runner = FakeRunner::new(vec![Err(HarnessError::Failed("cannot start".to_string()))]);
    let capabilities = harness().capabilities(&runner);
    assert_eq!(
        capabilities.binary_present,
        Capability::Unknown("cannot start".to_string())
    );
    assert_eq!(
        capabilities.version_pinned,
        Capability::Unknown("cannot start".to_string())
    );
    assert!(matches!(capabilities.headless, Capability::Unknown(_)));
    assert_eq!(runner.commands().len(), 1);
    assert!(matches!(
        capabilities.cost_reporting,
        Capability::Unsupported(_)
    ));
    assert!(matches!(
        capabilities.stop_mid_run,
        Capability::Unsupported(_)
    ));
    assert!(matches!(
        capabilities.stop_hook_checks_messages,
        Capability::Unsupported(_)
    ));
    assert_eq!(
        capabilities.question_signal,
        Capability::Unsupported(QUESTION_SIGNAL_REASON.to_string())
    );
}

#[test]
fn capability_report_when_the_version_differs() {
    let runner = FakeRunner::new(vec![ok("9.9.9\n"), ok(HELP)]);
    let capabilities = harness().capabilities(&runner);
    assert_eq!(
        capabilities.version_pinned,
        Capability::Unsupported(format!(
            "version 9.9.9 is installed, {PINNED_VERSION} is pinned"
        ))
    );
    assert_eq!(capabilities.binary_present, Capability::Supported);
}

#[test]
fn capability_report_when_the_version_probe_exits_non_zero() {
    let runner = FakeRunner::new(vec![exited(2, "bad\n")]);
    let capabilities = harness().capabilities(&runner);
    assert_eq!(
        capabilities.binary_present,
        Capability::Unknown("bad".to_string())
    );
    assert_eq!(
        capabilities.version_pinned,
        Capability::Unknown("bad".to_string())
    );
    assert!(matches!(capabilities.headless, Capability::Unknown(_)));
}

#[test]
fn capability_report_when_the_version_probe_times_out() {
    let runner = FakeRunner::new(vec![timed_out(30)]);
    let capabilities = harness().capabilities(&runner);
    assert_eq!(
        capabilities.binary_present,
        Capability::Unknown("version probe timed out after 30s".to_string())
    );
}

#[test]
fn capability_report_when_the_headless_help_lacks_json() {
    let help = HELP.replace("--json", "no-events");
    let runner = FakeRunner::new(vec![ok(VERSION), ok(&help)]);
    let capabilities = harness().capabilities(&runner);
    assert!(matches!(capabilities.headless, Capability::Unsupported(_)));
}

#[test]
fn capability_report_when_the_help_probe_fails() {
    let runner = FakeRunner::new(vec![ok(VERSION), exited(1, "no help\n")]);
    let capabilities = harness().capabilities(&runner);
    assert_eq!(
        capabilities.headless,
        Capability::Unknown("no help".to_string())
    );
}

#[test]
fn two_harnesses_run_the_same_task_through_one_function() {
    let dsh_runner = FakeRunner::new(successful_script());
    let dsh_result = run_one(&harness(), &dsh_runner);
    assert_eq!(dsh_result.actor.harness, "dsh");

    let fake_result = HarnessResult {
        actor: Actor {
            harness: "the-fake".to_string(),
            model: None,
            model_family: None,
        },
        session_id: None,
        final_message: "done".to_string(),
        outcome: HarnessOutcome::Finished,
        exit: 0,
        changed_files: Vec::new(),
        usage: None,
        cost_micro_usd: None,
    };
    let fake = FakeHarness::new(vec![Ok(fake_result)], all_supported());
    let fake_runner = FakeRunner::new(Vec::new());
    let fake_answer = run_one(&fake, &fake_runner);
    assert_eq!(fake_answer.actor.harness, "the-fake");
    assert_eq!(fake_answer.final_message, "done");
    assert_eq!(
        fake.tasks(),
        vec![task()],
        "both harnesses ran the same task"
    );

    let source = include_str!("harness_fake_run.rs");
    let body = run_one_source(source);
    for banned in ["Dsh", "dsh", "Fake", "fake"] {
        assert!(!body.contains(banned), "run_one names {banned}: {body}");
    }
}
