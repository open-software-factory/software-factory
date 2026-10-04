//! The real runner drives a program over its argv. A fake `docker` shell
//! script in a temp folder stands in for the installed tool; only the
//! ignored smoke test reaches a real daemon.

#![cfg(unix)]

use osf::docker_sandbox::{DockerRunner, DockerSandbox, RealDockerRunner};
use osf::sandbox::{CommandSpec, Mount, Network, RunOutcome, Sandbox, SandboxId, SandboxSpec};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

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
fn real_runner_reports_a_program_that_does_not_exist() {
    let _lock = serial();
    let area = TempArea::new("missing");
    let runner = RealDockerRunner::new(area.dir.join("does-not-exist"));

    let error = runner
        .run(&["version".to_string()], None)
        .expect_err("the program is missing");
    assert!(error.0.starts_with("cannot run docker"), "{error}");
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
        }],
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
