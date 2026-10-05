//! The real runner drives a program over its argv. A fake `docker` shell
//! script in a temp folder stands in for the installed tool; only the
//! ignored smoke test reaches a real daemon.

#![cfg(unix)]

use osf::docker_sandbox::{
    DockerRunner, DockerSandbox, RealDockerRunner, DEFAULT_OUTPUT_CAP_BYTES,
};
use osf::sandbox::{
    CommandSpec, Limits, Mount, Network, RunOutcome, Sandbox, SandboxId, SandboxSpec,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// Held for a whole test: a script that is still open for writing makes a parallel spawn fail with "Text file busy".
static SPAWN_LOCK: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SPAWN_LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

const OK_SCRIPT: &str = r#"#!/usr/bin/env bash
set -euo pipefail
dir="$(cd "$(dirname "$0")" && pwd)"
printf '%s\n' "$@" > "$dir/argv"
printf '%s' 'the output'
"#;

const FAIL_SCRIPT: &str = r"#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' 'boom from the fake' >&2
exit 3
";

const SLEEP_SCRIPT: &str = r"#!/usr/bin/env bash
sleep 5
";

const BACKGROUND_PIPE_SCRIPT: &str = r#"#!/usr/bin/env bash
dir="$(cd "$(dirname "$0")" && pwd)"
sleep 60 &
printf '%s' "$!" > "$dir/child.pid"
"#;

const SETSID_PIPE_SCRIPT: &str = r#"#!/usr/bin/env bash
dir="$(cd "$(dirname "$0")" && pwd)"
setsid sleep 20 &
printf '%s' "$!" > "$dir/child.pid"
"#;

const LONG_SCRIPT: &str = r"#!/usr/bin/env bash
set -euo pipefail
printf 'a%.0s' {1..100}
";

const BIG_TAIL_SCRIPT: &str = r"#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C
head -c 5242880 /dev/zero | tr '\0' x
printf '\nFINAL-EVENT\n'
";

const BIG_TAIL_STDERR_SCRIPT: &str = r"#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C
head -c 5242880 /dev/zero | tr '\0' x >&2
printf '\nFINAL-EVENT\n' >&2
";

/// A throwaway folder holding the fake program, removed on drop.
struct TempArea {
    dir: PathBuf,
}

impl TempArea {
    fn new(name: &str) -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock is after the epoch")
            .as_nanos();
        let base = std::env::temp_dir(); // osf: temp-dir allowed, a unique folder per test
        let dir = base.join(format!("osf-docker-sandbox-real-runner-{name}-{unique}"));
        fs::create_dir_all(&dir).expect("the temp folder creates");
        Self { dir }
    }

    fn script(&self, name: &str, body: &str) -> PathBuf {
        let path = self.dir.join(name);
        fs::write(&path, body).expect("the fake program writes");
        let mut perms = fs::metadata(&path)
            .expect("the fake program has metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).expect("the fake program is executable");
        path
    }
}

impl Drop for TempArea {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// A sandbox name unique to this run.
fn unique_name(prefix: &str) -> String {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is after the epoch")
        .as_nanos();
    format!("{prefix}-{}-{unique}", std::process::id())
}

/// Waits up to three seconds for `pid` to stop existing, checked with `kill -0`.
fn wait_for_process_gone(pid: &str) {
    let started = Instant::now();
    while std::process::Command::new("kill")
        .args(["-0", pid])
        .status()
        .is_ok_and(|status| status.success())
    {
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "the background process {pid} survived the kill"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn real_runner_returns_stdout_and_passes_argv() {
    let _lock = serial();
    let area = TempArea::new("ok");
    let script = area.script("docker", OK_SCRIPT);
    let runner = RealDockerRunner::new(script.as_path());
    let argv = vec![
        "version".to_string(),
        "--format".to_string(),
        "{{.Server.Version}}".to_string(),
    ];

    let output = runner.run(&argv, None).expect("the fake runs");
    assert_eq!(output.status, Some(0));
    assert_eq!(output.stdout, "the output");
    assert_eq!(output.stderr, "");
    assert!(!output.timed_out);

    let seen_argv = fs::read_to_string(area.dir.join("argv")).expect("argv was written");
    assert_eq!(seen_argv, "version\n--format\n{{.Server.Version}}\n");
}

#[test]
fn real_runner_reports_the_default_cap_and_no_truncation_for_small_output() {
    let _lock = serial();
    let area = TempArea::new("default-cap");
    let script = area.script("docker", OK_SCRIPT);
    let runner = RealDockerRunner::new(script.as_path());

    let output = runner
        .run(&["version".to_string()], None)
        .expect("the fake runs");
    assert_eq!(output.output_cap_bytes, Some(DEFAULT_OUTPUT_CAP_BYTES));
    assert!(!output.stdout_truncated);
    assert!(!output.stderr_truncated);
}

#[test]
fn real_runner_caps_stdout_and_reports_truncation() {
    let _lock = serial();
    let area = TempArea::new("cap");
    let script = area.script("docker", LONG_SCRIPT);
    let runner = RealDockerRunner::new(script.as_path()).with_output_cap(10);

    let output = runner
        .run(&["exec".to_string()], None)
        .expect("the fake runs");
    assert_eq!(output.stdout, "aaaaa\n[... 90 bytes omitted ...]\naaaaa");
    assert!(output.stdout_truncated);
    assert!(!output.stderr_truncated);
    assert_eq!(output.output_cap_bytes, Some(10));
}

#[test]
fn real_runner_keeps_the_first_and_last_half_of_a_cut_stdout() {
    let _lock = serial();
    let area = TempArea::new("big-tail-stdout");
    let script = area.script("docker", BIG_TAIL_SCRIPT);
    let runner = RealDockerRunner::new(script.as_path());

    let output = runner
        .run(&["exec".to_string()], None)
        .expect("the fake runs");
    assert!(output.stdout.starts_with('x'), "{:?}", output.stdout);
    assert!(
        output.stdout.ends_with("FINAL-EVENT\n"),
        "{:?}",
        output.stdout
    );
    assert_eq!(
        output.stdout.matches("bytes omitted").count(),
        1,
        "{:?}",
        output.stdout
    );
    assert!(
        u64::try_from(output.stdout.len()).expect("the length fits")
            <= DEFAULT_OUTPUT_CAP_BYTES + 64,
        "{}",
        output.stdout.len()
    );
    assert!(output.stdout_truncated);
    assert!(!output.stderr_truncated);
}

#[test]
fn real_runner_keeps_the_first_and_last_half_of_a_cut_stderr() {
    let _lock = serial();
    let area = TempArea::new("big-tail-stderr");
    let script = area.script("docker", BIG_TAIL_STDERR_SCRIPT);
    let runner = RealDockerRunner::new(script.as_path());

    let output = runner
        .run(&["exec".to_string()], None)
        .expect("the fake runs");
    assert!(output.stderr.starts_with('x'), "{:?}", output.stderr);
    assert!(
        output.stderr.ends_with("FINAL-EVENT\n"),
        "{:?}",
        output.stderr
    );
    assert_eq!(
        output.stderr.matches("bytes omitted").count(),
        1,
        "{:?}",
        output.stderr
    );
    assert!(
        u64::try_from(output.stderr.len()).expect("the length fits")
            <= DEFAULT_OUTPUT_CAP_BYTES + 64,
        "{}",
        output.stderr.len()
    );
    assert!(output.stderr_truncated);
    assert!(!output.stdout_truncated);
}

#[test]
fn real_docker_sandbox_run_keeps_the_tail_of_a_cut_stream() {
    let _lock = serial();
    let area = TempArea::new("big-tail-sandbox");
    let script = area.script("docker", BIG_TAIL_SCRIPT);
    let runner = RealDockerRunner::new(script.as_path());
    let sandbox = DockerSandbox::new(runner);
    let command = CommandSpec {
        program: "run".to_string(),
        args: Vec::new(),
        workdir: None,
        timeout_secs: Some(30),
        env: std::collections::BTreeMap::new(),
        stdin: None,
    };

    let result = sandbox
        .run(&SandboxId("abc".to_string()), &command)
        .expect("the fake runs");
    assert_eq!(result.outcome, RunOutcome::Exited(0));
    assert!(
        result.stdout.ends_with("FINAL-EVENT\n"),
        "{:?}",
        result.stdout
    );
    assert!(result.stdout_truncated);
    assert_eq!(result.output_cap_bytes, Some(DEFAULT_OUTPUT_CAP_BYTES));
}

#[test]
fn real_runner_reports_a_nonzero_status_and_stderr() {
    let _lock = serial();
    let area = TempArea::new("fail");
    let script = area.script("docker", FAIL_SCRIPT);
    let runner = RealDockerRunner::new(script.as_path());

    let output = runner
        .run(&["image".to_string(), "inspect".to_string()], None)
        .expect("the fake runs");
    assert_eq!(output.status, Some(3));
    assert_eq!(output.stderr, "boom from the fake\n");
    assert!(!output.timed_out);
}

#[test]
fn real_runner_times_out_and_reports_it() {
    let _lock = serial();
    let area = TempArea::new("timeout");
    let script = area.script("docker", SLEEP_SCRIPT);
    let runner = RealDockerRunner::new(script.as_path());

    let started = std::time::Instant::now();
    let output = runner
        .run(&["exec".to_string()], Some(Duration::from_secs(1)))
        .expect("the fake runs");
    assert!(output.timed_out);
    assert_eq!(output.status, None);
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the timeout took {:?}",
        started.elapsed()
    );
}

#[test]
fn real_runner_keeps_the_output_printed_before_a_timeout() {
    let _lock = serial();
    let area = TempArea::new("timeout-partial");
    let script = area.script(
        "docker",
        "#!/usr/bin/env bash\nprintf 'before-the-timeout'\nprintf 'err-before' >&2\nsleep 30\n",
    );
    let runner = RealDockerRunner::new(script.as_path());

    let output = runner
        .run(&["exec".to_string()], Some(Duration::from_secs(1)))
        .expect("the fake runs");
    assert!(output.timed_out);
    assert_eq!(output.stdout, "before-the-timeout");
    assert_eq!(output.stderr, "err-before");
}

#[test]
fn real_runner_times_out_when_a_child_keeps_stdout_open() {
    let _lock = serial();
    let area = TempArea::new("timeout-pipe");
    let script = area.script("docker", BACKGROUND_PIPE_SCRIPT);
    let runner = RealDockerRunner::new(script.as_path());

    let started = Instant::now();
    let output = runner
        .run(&["exec".to_string()], Some(Duration::from_secs(2)))
        .expect("the fake runs");
    assert!(output.timed_out);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the timeout took {:?}",
        started.elapsed()
    );
    let pid = fs::read_to_string(area.dir.join("child.pid")).expect("the pid file exists");
    wait_for_process_gone(pid.trim());
}

#[test]
fn real_runner_times_out_when_a_setsid_child_keeps_stdout_open() {
    let _lock = serial();
    let area = TempArea::new("timeout-setsid");
    let script = area.script("docker", SETSID_PIPE_SCRIPT);
    let runner = RealDockerRunner::new(script.as_path());

    let started = Instant::now();
    let output = runner
        .run(&["exec".to_string()], Some(Duration::from_secs(2)))
        .expect("the fake runs");
    assert!(output.timed_out);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the timeout took {:?}",
        started.elapsed()
    );
}

#[test]
fn real_runner_keeps_a_fast_run_with_a_timeout() {
    let _lock = serial();
    let area = TempArea::new("fast-timeout");
    let script = area.script("docker", OK_SCRIPT);
    let runner = RealDockerRunner::new(script.as_path());

    let output = runner
        .run(&["version".to_string()], Some(Duration::from_secs(5)))
        .expect("the fake runs");
    assert!(!output.timed_out);
    assert_eq!(output.status, Some(0));
    assert_eq!(output.stdout, "the output");
}

#[test]
fn real_runner_reports_a_program_that_does_not_exist() {
    let _lock = serial();
    let area = TempArea::new("missing");
    let runner = RealDockerRunner::new(area.dir.join("does-not-exist"));

    let error = runner
        .run(&["version".to_string()], None)
        .expect_err("the program is missing");
    assert!(error.0.starts_with("cannot run docker"), "{error}");
}

#[test]
fn real_runner_writes_stdin_to_the_program() {
    let _lock = serial();
    let area = TempArea::new("stdin");
    let script = area.script(
        "docker",
        r#"#!/usr/bin/env bash
set -euo pipefail
dir="$(cd "$(dirname "$0")" && pwd)"
cat > "$dir/stdin.out"
"#,
    );
    let runner = RealDockerRunner::new(script.as_path());
    let input = vec![b'x'; 300_000];
    let output = runner
        .run_with_stdin(&["exec".to_string()], None, Some(input.as_slice()))
        .expect("the fake runs");
    assert_eq!(output.status, Some(0));
    let written = fs::read(area.dir.join("stdin.out")).expect("the input file exists");
    assert_eq!(written, input);
}

#[test]
fn real_runner_does_not_hang_when_the_program_ignores_a_large_stdin() {
    let _lock = serial();
    let area = TempArea::new("stdin-ignored");
    let script = area.script("docker", "#!/usr/bin/env bash\nexit 0\n");
    let runner = RealDockerRunner::new(script.as_path());
    let input = vec![b'x'; 5 * 1024 * 1024];
    let started = std::time::Instant::now();
    let output = runner
        .run_with_stdin(&["exec".to_string()], None, Some(input.as_slice()))
        .expect("the fake runs");
    assert_eq!(output.status, Some(0));
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the run took {:?}",
        started.elapsed()
    );
}

#[test]
fn run_without_stdin_closes_standard_input_at_once() {
    let _lock = serial();
    let area = TempArea::new("no-stdin");
    let script = area.script("docker", "#!/usr/bin/env bash\ncat\n");
    let runner = RealDockerRunner::new(script.as_path());
    let output = runner
        .run(&["exec".to_string()], None)
        .expect("the fake runs");
    assert_eq!(output.status, Some(0));
    assert_eq!(output.stdout, "");
}

#[test]
fn real_docker_sandbox_run_redacts_env_values_from_a_failed_exec() {
    let _lock = serial();
    let area = TempArea::new("redact");
    let script = area.script(
        "docker",
        r#"#!/usr/bin/env bash
printf '%s\n' "$@" >&2
exit 125
"#,
    );
    let runner = RealDockerRunner::new(script.as_path());
    let sandbox = DockerSandbox::new(runner);
    let mut command = CommandSpec {
        program: "run".to_string(),
        args: vec!["--fast".to_string()],
        workdir: None,
        timeout_secs: Some(30),
        env: std::collections::BTreeMap::new(),
        stdin: None,
    };
    command
        .env
        .insert("API_KEY".to_string(), "s3cr3t-value".to_string());
    let error = sandbox
        .run(&SandboxId("abc".to_string()), &command)
        .expect_err("125 fails");
    assert!(!error.to_string().contains("s3cr3t-value"), "{error}");
    assert!(error.to_string().contains("<redacted>"), "{error}");
}

/// Destroys the smoke sandbox on drop, including on a failed assertion.
struct SandboxGuard<'a> {
    sandbox: &'a DockerSandbox<RealDockerRunner>,
    id: SandboxId,
    state: TempArea,
}

impl Drop for SandboxGuard<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.sandbox.destroy(&self.id) {
            eprintln!("the smoke sandbox was not destroyed: {error}");
        }
    }
}

#[test]
#[ignore = "needs a running docker daemon and OSF_SANDBOX_SMOKE_IMAGE"]
fn real_docker_runs_a_command_and_times_out() {
    let Ok(image) = std::env::var("OSF_SANDBOX_SMOKE_IMAGE") else {
        println!("skipping the docker smoke test: OSF_SANDBOX_SMOKE_IMAGE is unset");
        return;
    };
    let _lock = serial();
    let state = TempArea::new("smoke-state");
    let mut perms = fs::metadata(&state.dir)
        .expect("the state folder has metadata")
        .permissions();
    perms.set_mode(0o777);
    fs::set_permissions(&state.dir, perms).expect("the state folder is world-writable");

    let spec = SandboxSpec {
        name: unique_name("osf-smoke"),
        image,
        user: "dev".to_string(),
        workdir: "/workspace".to_string(),
        network: Network::Isolated,
        mounts: vec![Mount {
            host_path: state.dir.to_string_lossy().into_owned(),
            sandbox_path: "/state".to_string(),
            read_only: false,
            follow_symlinks: false,
        }],
        limits: Limits::default(),
    };
    let sandbox = DockerSandbox::real();
    let id = sandbox.create(&spec).expect("the smoke sandbox creates");
    let guard = SandboxGuard {
        sandbox: &sandbox,
        id,
        state,
    };

    let sh = |script: &str, timeout_secs: Option<u64>| CommandSpec {
        program: "sh".to_string(),
        args: vec!["-c".to_string(), script.to_string()],
        workdir: None,
        timeout_secs,
        env: std::collections::BTreeMap::new(),
        stdin: None,
    };

    let write = guard
        .sandbox
        .run(&guard.id, &sh("echo hello > /state/out.txt", Some(10)))
        .expect("the write runs");
    assert_eq!(write.outcome, RunOutcome::Exited(0));
    let written =
        fs::read_to_string(guard.state.dir.join("out.txt")).expect("the host file exists");
    assert_eq!(written, "hello\n");

    let exit = guard
        .sandbox
        .run(&guard.id, &sh("exit 7", Some(10)))
        .expect("the exit runs");
    assert_eq!(exit.outcome, RunOutcome::Exited(7));

    let started = std::time::Instant::now();
    let sleep = guard
        .sandbox
        .run(&guard.id, &sh("sleep 30", Some(2)))
        .expect("the sleep runs");
    assert_eq!(sleep.outcome, RunOutcome::TimedOut { limit_secs: 2 });
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "the timed-out step took {:?}",
        started.elapsed()
    );

    let missing = guard
        .sandbox
        .run(
            &SandboxId("osf-no-such-sandbox".to_string()),
            &sh("true", Some(10)),
        )
        .expect_err("a missing sandbox is a failure, not an exit status");
    assert!(
        missing.to_string().contains("No such container"),
        "{missing}"
    );

    drop(guard);
}

#[test]
#[ignore = "needs a running docker daemon and OSF_SANDBOX_SMOKE_IMAGE"]
fn osf_sandbox_run_command_runs_and_passes_the_exit_status() {
    let Ok(image) = std::env::var("OSF_SANDBOX_SMOKE_IMAGE") else {
        println!("skipping the docker smoke test: OSF_SANDBOX_SMOKE_IMAGE is unset");
        return;
    };
    let _lock = serial();
    let repo = TempArea::new("sandbox-cli-repo");
    let state = TempArea::new("sandbox-cli-state");
    let mut perms = fs::metadata(&state.dir)
        .expect("the state folder has metadata")
        .permissions();
    perms.set_mode(0o777);
    fs::set_permissions(&state.dir, perms).expect("the state folder is world-writable");

    let args = vec![
        "sandbox".to_string(),
        "run".to_string(),
        "--image".to_string(),
        image,
        "--repo".to_string(),
        repo.dir.to_string_lossy().into_owned(),
        "--state".to_string(),
        state.dir.to_string_lossy().into_owned(),
        "--".to_string(),
        "sh".to_string(),
        "-c".to_string(),
        "echo hello > /state/out.txt; exit 7".to_string(),
    ];
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_osf"))
        .args(&args)
        .output()
        .expect("osf runs");
    assert_eq!(output.status.code(), Some(7), "{output:?}");
    let written = fs::read_to_string(state.dir.join("out.txt")).expect("the host file exists");
    assert_eq!(written, "hello\n");
}

/// The container ids `docker` prints for `label=osf.sandbox=<name>` under
/// `listing`, one per line and trimmed.
fn docker_ids(name: &str, listing: &[&str]) -> String {
    let output = std::process::Command::new("docker")
        .args(listing)
        .arg("--filter")
        .arg(format!("label=osf.sandbox={name}"))
        .arg("--format")
        .arg("{{.ID}}")
        .output()
        .expect("docker runs");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Waits up to `limit` for a running container with the sandbox label.
fn wait_for_container(name: &str, limit: Duration) -> bool {
    let started = Instant::now();
    while docker_ids(name, &["ps"]).is_empty() {
        if started.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    true
}

/// Removes the container by name when it goes out of scope, so a failed
/// assertion leaves nothing behind.
struct ContainerGuard {
    name: String,
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker")
            .args(["rm", "--force", "--", &self.name])
            .status();
    }
}

#[test]
#[ignore = "needs a running docker daemon and OSF_SANDBOX_SMOKE_IMAGE"]
fn osf_sandbox_run_removes_the_sandbox_when_sent_sigterm() {
    let Ok(image) = std::env::var("OSF_SANDBOX_SMOKE_IMAGE") else {
        println!("skipping the docker smoke test: OSF_SANDBOX_SMOKE_IMAGE is unset");
        return;
    };
    let _lock = serial();
    let repo = TempArea::new("sigterm-repo");
    let state = TempArea::new("sigterm-state");
    let mut perms = fs::metadata(&state.dir)
        .expect("the state folder has metadata")
        .permissions();
    perms.set_mode(0o777);
    fs::set_permissions(&state.dir, perms).expect("the state folder is world-writable");
    let name = unique_name("osf-sigterm");
    let _guard = ContainerGuard { name: name.clone() };

    let repo_arg = repo.dir.to_string_lossy().into_owned();
    let state_arg = state.dir.to_string_lossy().into_owned();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_osf"))
        .args([
            "sandbox",
            "run",
            "--image",
            image.as_str(),
            "--repo",
            repo_arg.as_str(),
            "--state",
            state_arg.as_str(),
            "--name",
            name.as_str(),
            "--",
            "sleep",
            "100",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("osf spawns");

    if !wait_for_container(&name, Duration::from_secs(60)) {
        let _ = child.kill();
        let _ = child.wait();
        panic!("the sandbox with label osf.sandbox={name} never appeared");
    }

    let pid = child.id().to_string();
    let sent = std::process::Command::new("kill")
        .args(["-TERM", pid.as_str()])
        .status()
        .expect("kill runs");
    assert!(sent.success(), "SIGTERM was not sent");
    let output = child.wait_with_output().expect("osf exits");
    assert_eq!(output.status.code(), Some(143), "{output:?}");
    let remaining = docker_ids(&name, &["ps", "-a"]);
    assert!(
        remaining.is_empty(),
        "the sandbox is still there: {remaining}"
    );
}
