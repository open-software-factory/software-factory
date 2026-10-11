//! A full run and capability report through the neutral harness interface,
//! with the in-memory fakes. The only concrete adapter named here is `dsh`.

use osf::dsh_harness::{
    branch_command, git_env, head_command, parse_events, probe_command, status_command, DshConfig,
    DshHarness, PINNED_VERSION,
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
const NO_CREDENTIAL: &str = include_str!("fixtures/harness/dsh-run-no-credential.jsonl");
const QUESTION_MARKER_TEXT: &str = "Which colour should `hello.txt` mention, red or blue?";
const QUESTION_SIGNAL_REASON: &str = "prompt-contract: no question event exists; the prompt asks the agent to end with a QUESTION: line and the adapter reads that line";
const VERSION: &str = include_str!("fixtures/harness/dsh-version.txt");
const HELP: &str = include_str!("fixtures/harness/dsh-headless-help.txt");
const DUMP: &str = include_str!("fixtures/harness/dsh-dump-config.yml");
const ABSENT_CHECK_REASON: &str =
    "the profile lists no plugin that checks messages at the end of a turn";
const VERSION_PROBE_FAILED_REASON: &str =
    "the version probe failed, so the stop hook was not probed";
const ENABLING_ROW: &str = "- id: osf-writing-check\n  name: '@open-software-factory/osf-dsh-plugin'\n  config:\n    command: osf\n";

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

/// The head commit before and after a run that follows the rules.
const HEAD_BEFORE: &str = "1111111111111111111111111111111111111111";
const HEAD_AFTER: &str = "1111111111111111111111111111111111111111";

/// The branch a run started on.
const BRANCH: &str = "feat/harness-adapter";

/// A head output line for `commit`.
fn head(commit: &str) -> Result<CommandOutput, HarnessError> {
    ok(&format!("{commit}\n"))
}

/// A branch output line for `name`.
fn branch(name: &str) -> Result<CommandOutput, HarnessError> {
    ok(&format!("{name}\n"))
}

/// The script of one successful run: clean, head, branch, fixture, file, head.
fn successful_script() -> Vec<Result<CommandOutput, HarnessError>> {
    vec![
        clean(),
        head(HEAD_BEFORE),
        branch(BRANCH),
        ok(CREATED_FILE),
        one_new_file(),
        head(HEAD_AFTER),
    ]
}

/// The adapter config: a program and a complete child environment.
fn config() -> DshConfig {
    DshConfig::new(
        "dsh",
        vec![
            ("PATH".into(), "/usr/bin".into()),
            ("HOME".into(), "/var/agent-home".into()),
            ("DEEPSEEK_API_KEY".into(), "key-value-for-test".into()),
            ("GIT_AUTHOR_NAME".into(), "someone".into()),
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
    assert_eq!(result.branch.as_deref(), Some(BRANCH));
    assert_eq!(result.head_commit, HEAD_AFTER);
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
            cache_read_tokens: 16000,
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
        let runner = FakeRunner::new(vec![
            clean(),
            head(HEAD_BEFORE),
            branch(BRANCH),
            ok(fixture),
            clean(),
            head(HEAD_AFTER),
        ]);
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
    let runner = FakeRunner::new(vec![
        clean(),
        head(HEAD_BEFORE),
        branch(BRANCH),
        ok(QUESTION_MARKER),
        clean(),
        head(HEAD_AFTER),
    ]);
    let result = harness().run(&runner, &task()).expect("the run succeeds");
    assert_eq!(
        result.outcome,
        HarnessOutcome::Asked {
            question: QUESTION_MARKER_TEXT.to_string(),
        }
    );
    assert_eq!(result.changed_files, Vec::<String>::new());
}

#[test]
fn a_real_run_that_followed_the_question_rule_is_asked() {
    let runner = FakeRunner::new(vec![
        clean(),
        head(HEAD_BEFORE),
        branch(BRANCH),
        ok(QUESTION_CONTRACT),
        clean(),
        head(HEAD_AFTER),
    ]);
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
    let runner = FakeRunner::new(vec![
        clean(),
        head(HEAD_BEFORE),
        branch(BRANCH),
        ok(QUESTION_MARKER),
        one_new_file(),
        head(HEAD_AFTER),
    ]);
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
    let runner = FakeRunner::new(vec![
        clean(),
        head(HEAD_BEFORE),
        branch(BRANCH),
        exited(3, "boom\n"),
    ]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert_eq!(error, HarnessError::Failed("boom".to_string()));
    assert_eq!(
        runner.commands().len(),
        4,
        "the status, the head, the branch and the agent only"
    );
}

#[test]
fn a_non_zero_exit_with_no_text_is_failed_with_the_status() {
    let runner = FakeRunner::new(vec![
        clean(),
        head(HEAD_BEFORE),
        branch(BRANCH),
        exited(4, ""),
    ]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert_eq!(
        error,
        HarnessError::Failed("dsh exited with status 4".to_string())
    );
}

#[test]
fn a_timeout_is_failed_and_names_the_limit() {
    let runner = FakeRunner::new(vec![
        clean(),
        head(HEAD_BEFORE),
        branch(BRANCH),
        timed_out(7),
    ]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert_eq!(
        error,
        HarnessError::Failed("dsh timed out after 7s".to_string())
    );
}

#[test]
fn exit_zero_with_no_final_event_is_failed() {
    let first_line = CREATED_FILE.lines().next().expect("the fixture has a line");
    let runner = FakeRunner::new(vec![
        clean(),
        head(HEAD_BEFORE),
        branch(BRANCH),
        ok(first_line),
    ]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert!(failed(error).contains("no final event"));
}

#[test]
fn a_turn_that_ended_with_an_error_fails_even_at_exit_zero() {
    let runner = FakeRunner::new(vec![
        clean(),
        head(HEAD_BEFORE),
        branch(BRANCH),
        ok(NO_CREDENTIAL),
    ]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    let text = failed(error);
    assert!(text.contains("MISSING_CREDENTIAL"), "{text}");
    assert!(text.contains("no API key"), "{text}");
    assert_eq!(
        runner.commands().len(),
        4,
        "the status, the head, the branch and the agent only"
    );
}

#[test]
fn a_turn_failure_with_exit_one_still_falls_back_to_stdout() {
    let output = Ok(CommandOutput {
        outcome: CommandOutcome::Exited(1),
        stdout: NO_CREDENTIAL.to_string(),
        stderr: String::new(),
    });
    let runner = FakeRunner::new(vec![clean(), head(HEAD_BEFORE), branch(BRANCH), output]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    let text = failed(error);
    assert!(text.contains("MISSING_CREDENTIAL"), "{text}");
}

#[test]
fn a_non_json_line_is_failed_with_its_line_number() {
    let runner = FakeRunner::new(vec![
        clean(),
        head(HEAD_BEFORE),
        branch(BRANCH),
        ok("oops\n"),
    ]);
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
        head(HEAD_BEFORE),
        branch(BRANCH),
        ok(CREATED_FILE),
        exited(128, "fatal: broken index\n"),
    ]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert_eq!(failed(error), "fatal: broken index".to_string());
}

#[test]
fn a_detached_head_reports_no_branch() {
    let runner = FakeRunner::new(vec![
        clean(),
        head(HEAD_BEFORE),
        branch("HEAD"),
        ok(CREATED_FILE),
        one_new_file(),
        head(HEAD_AFTER),
    ]);
    let result = harness().run(&runner, &task()).expect("the run succeeds");
    assert_eq!(result.branch, None);
    assert_eq!(result.head_commit, HEAD_AFTER);
}

#[test]
fn a_commit_by_the_agent_is_failed_with_both_heads() {
    let moved = "2222222222222222222222222222222222222222";
    let runner = FakeRunner::new(vec![
        clean(),
        head(HEAD_BEFORE),
        branch(BRANCH),
        ok(CREATED_FILE),
        one_new_file(),
        head(moved),
    ]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    let text = failed(error);
    assert!(text.contains(moved), "{text} must name the new head");
    assert!(text.contains(HEAD_BEFORE), "{text} must name the old head");
}

#[test]
fn a_failed_head_before_the_run_is_failed_with_its_text() {
    let runner = FakeRunner::new(vec![clean(), exited(128, "fatal: bad revision\n")]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert_eq!(failed(error), "fatal: bad revision".to_string());
    let commands = runner.commands();
    assert_eq!(commands.len(), 2, "the status and the head only");
    assert_eq!(
        commands.get(1).expect("the head before"),
        &head_command(&config(), "/repo")
    );
}

#[test]
fn a_failed_head_after_the_run_is_failed_with_its_text() {
    let runner = FakeRunner::new(vec![
        clean(),
        head(HEAD_BEFORE),
        branch(BRANCH),
        ok(CREATED_FILE),
        one_new_file(),
        exited(128, "fatal: bad revision\n"),
    ]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert_eq!(failed(error), "fatal: bad revision".to_string());
}

#[test]
fn a_head_that_prints_nothing_is_failed() {
    let runner = FakeRunner::new(vec![clean(), ok("")]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert_eq!(failed(error), "git rev-parse HEAD printed nothing");
}

#[test]
fn a_head_that_times_out_is_failed_and_names_the_limit() {
    let runner = FakeRunner::new(vec![clean(), timed_out(9)]);
    let error = harness().run(&runner, &task()).expect_err("the run fails");
    assert_eq!(failed(error), "git rev-parse HEAD timed out after 9s");
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
    assert_eq!(runner.commands(), Vec::<osf::harness::CommandSpec>::new());
}

#[test]
fn the_commands_reaching_the_runner_are_the_documented_ones() {
    let runner = FakeRunner::new(successful_script());
    harness().run(&runner, &task()).expect("the run succeeds");
    let commands = runner.commands();
    assert_eq!(
        commands.len(),
        6,
        "status, head, branch, agent, status, head"
    );

    let status = status_command(&config(), "/repo");
    let head = head_command(&config(), "/repo");
    let branch = branch_command(&config(), "/repo");
    assert_eq!(commands.first().expect("the status before"), &status);
    assert_eq!(commands.get(1).expect("the head before"), &head);
    assert_eq!(commands.get(2).expect("the branch"), &branch);
    assert_eq!(commands.get(4).expect("the status after"), &status);
    assert_eq!(commands.get(5).expect("the head after"), &head);

    let agent = commands.get(3).expect("the agent command");
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
    assert!(
        agent.env.iter().any(|(name, _)| name == "DEEPSEEK_API_KEY"),
        "the agent command carries the API key"
    );

    let minimal = git_env(&config());
    assert_eq!(
        minimal,
        vec![
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("HOME".to_string(), "/var/agent-home".to_string()),
            ("GIT_AUTHOR_NAME".to_string(), "someone".to_string()),
        ]
    );
    for command in &commands {
        if command.program == "git" {
            assert_eq!(
                command.env, minimal,
                "a git command carries only the minimal environment"
            );
            assert!(
                !command
                    .env
                    .iter()
                    .any(|(name, _)| name == "DEEPSEEK_API_KEY"),
                "no git command carries the API key"
            );
        }
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
        value.get("branch").and_then(serde_json::Value::as_str),
        Some(BRANCH)
    );
    assert_eq!(
        value.get("head_commit").and_then(serde_json::Value::as_str),
        Some(HEAD_AFTER)
    );
    assert_eq!(
        value
            .get("usage")
            .and_then(|usage| usage.get("total_tokens"))
            .and_then(serde_json::Value::as_u64),
        Some(16973)
    );
    assert_eq!(
        value
            .get("usage")
            .and_then(|usage| usage.get("cache_read_tokens"))
            .and_then(serde_json::Value::as_u64),
        Some(16000)
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
    let runner = FakeRunner::new(vec![ok(VERSION), ok(HELP), ok(DUMP)]);
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
    assert_eq!(
        capabilities.stop_hook_checks_messages,
        Capability::Unsupported(ABSENT_CHECK_REASON.to_string())
    );
    assert_eq!(
        capabilities.question_signal,
        Capability::Unsupported(QUESTION_SIGNAL_REASON.to_string())
    );

    let commands = runner.commands();
    assert_eq!(commands.len(), 3);
    assert_eq!(
        commands.first().expect("the version probe"),
        &probe_command(&config(), &["--version"])
    );
    assert_eq!(
        commands.get(1).expect("the help probe"),
        &probe_command(&config(), &["--profile", "headless", "--help"])
    );
    assert_eq!(
        commands.get(2).expect("the dump probe"),
        &probe_command(&config(), &["--profile", "headless", "--dump-config"])
    );
}

#[test]
fn capability_report_when_the_message_check_plugin_is_present() {
    let dump = format!("{DUMP}{ENABLING_ROW}");
    let runner = FakeRunner::new(vec![ok(VERSION), ok(HELP), ok(&dump)]);
    let capabilities = harness().capabilities(&runner);
    assert_eq!(
        capabilities.stop_hook_checks_messages,
        Capability::Supported
    );
}

#[test]
fn capability_report_when_the_dump_probe_fails() {
    let runner = FakeRunner::new(vec![ok(VERSION), ok(HELP), exited(1, "dump failed\n")]);
    let capabilities = harness().capabilities(&runner);
    assert_eq!(
        capabilities.stop_hook_checks_messages,
        Capability::Unknown("dump failed".to_string())
    );
}

#[test]
fn capability_report_when_the_dump_probe_times_out() {
    let runner = FakeRunner::new(vec![ok(VERSION), ok(HELP), timed_out(30)]);
    let capabilities = harness().capabilities(&runner);
    assert_eq!(
        capabilities.stop_hook_checks_messages,
        Capability::Unknown("stop hook probe timed out after 30s".to_string())
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
    assert_eq!(
        capabilities.stop_hook_checks_messages,
        Capability::Unknown(VERSION_PROBE_FAILED_REASON.to_string())
    );
    assert_eq!(
        capabilities.question_signal,
        Capability::Unsupported(QUESTION_SIGNAL_REASON.to_string())
    );
}

#[test]
fn capability_report_when_the_version_differs() {
    let runner = FakeRunner::new(vec![ok("9.9.9\n"), ok(HELP), ok(DUMP)]);
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
    assert_eq!(
        capabilities.stop_hook_checks_messages,
        Capability::Unknown(VERSION_PROBE_FAILED_REASON.to_string())
    );
    assert_eq!(runner.commands().len(), 1);
}

#[test]
fn capability_report_when_the_version_probe_times_out() {
    let runner = FakeRunner::new(vec![timed_out(30)]);
    let capabilities = harness().capabilities(&runner);
    assert_eq!(
        capabilities.binary_present,
        Capability::Unknown("version probe timed out after 30s".to_string())
    );
    assert_eq!(
        capabilities.stop_hook_checks_messages,
        Capability::Unknown(VERSION_PROBE_FAILED_REASON.to_string())
    );
    assert_eq!(runner.commands().len(), 1);
}

#[test]
fn capability_report_when_the_headless_help_lacks_json() {
    let help = HELP.replace("--json", "no-events");
    let runner = FakeRunner::new(vec![ok(VERSION), ok(&help), ok(DUMP)]);
    let capabilities = harness().capabilities(&runner);
    assert!(matches!(capabilities.headless, Capability::Unsupported(_)));
}

#[test]
fn capability_report_when_the_help_probe_fails() {
    let runner = FakeRunner::new(vec![ok(VERSION), exited(1, "no help\n"), ok(DUMP)]);
    let capabilities = harness().capabilities(&runner);
    assert_eq!(
        capabilities.headless,
        Capability::Unknown("no help".to_string())
    );
    assert_eq!(
        capabilities.stop_hook_checks_messages,
        Capability::Unsupported(ABSENT_CHECK_REASON.to_string())
    );
    let commands = runner.commands();
    assert_eq!(commands.len(), 3);
    assert_eq!(
        commands.get(2).expect("the dump probe"),
        &probe_command(&config(), &["--profile", "headless", "--dump-config"])
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
        branch: Some("main".to_string()),
        head_commit: "abc123".to_string(),
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
