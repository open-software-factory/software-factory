//! The real runner drives a program over its argv and standard input. A fake
//! `gh` shell script in a temp folder stands in for the installed tool;
//! nothing here reaches the network.

#![cfg(unix)]

use osf::github_tracker::{GhRunner, RealGhRunner};
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
cat > "$dir/stdin"
printf '%s' 'the output'
"#;

const FAIL_SCRIPT: &str = r"#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' 'boom from the fake' >&2
exit 1
";

const HANG_SCRIPT: &str = r"#!/usr/bin/env bash
exec sleep 5
";

const QUICK_SCRIPT: &str = r"#!/usr/bin/env bash
printf '%s' 'the output'
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
        let dir = base.join(format!("osf-tracker-real-runner-{name}-{unique}"));
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

#[test]
fn real_runner_returns_stdout_and_passes_argv_and_stdin() {
    let _lock = serial();
    let area = TempArea::new("ok");
    let script = area.script("gh", OK_SCRIPT);
    let runner = RealGhRunner::new(script.as_path());
    let argv = vec![
        "api".to_string(),
        "graphql".to_string(),
        "--input".to_string(),
        "-".to_string(),
    ];
    let stdin = r#"{"query":"query { viewer { login } }","variables":{}}"#;

    let output = runner.run(&argv, Some(stdin)).expect("the fake runs");
    assert_eq!(output, "the output");

    let seen_argv = fs::read_to_string(area.dir.join("argv")).expect("argv was written");
    assert_eq!(seen_argv, "api\ngraphql\n--input\n-\n");
    let seen_stdin = fs::read_to_string(area.dir.join("stdin")).expect("stdin was written");
    assert_eq!(seen_stdin, stdin);
}

#[test]
fn real_runner_reports_trimmed_stderr_on_a_nonzero_exit() {
    let _lock = serial();
    let area = TempArea::new("fail");
    let script = area.script("gh", FAIL_SCRIPT);
    let runner = RealGhRunner::new(script.as_path());

    let error = runner
        .run(&["api".to_string()], Some("x"))
        .expect_err("the fake fails");
    assert!(error.contains("boom from the fake"));
    assert_eq!(error, "boom from the fake");
}

#[test]
fn real_runner_reports_a_program_that_does_not_exist() {
    let _lock = serial();
    let area = TempArea::new("missing");
    let runner = RealGhRunner::new(area.dir.join("does-not-exist"));

    let error = runner
        .run(&["api".to_string()], None)
        .expect_err("the program is missing");
    assert!(error.starts_with("cannot run gh"));
}

#[test]
fn real_runner_times_out_a_hanging_program() {
    let _lock = serial();
    let area = TempArea::new("timeout");
    let script = area.script("gh", HANG_SCRIPT);
    let runner = RealGhRunner::new(script).with_timeout(Duration::from_millis(300));

    let started = Instant::now();
    let error = runner
        .run(&["api".to_string()], None)
        .expect_err("the fake hangs");
    assert!(error.starts_with("cannot run gh"));
    assert!(error.contains("timed out"));
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn real_runner_defaults_to_a_sixty_second_timeout() {
    let _lock = serial();
    assert_eq!(RealGhRunner::default().timeout(), Duration::from_secs(60));
    assert_eq!(RealGhRunner::new("gh").timeout(), Duration::from_secs(60));
    assert_eq!(
        RealGhRunner::new("gh")
            .with_timeout(Duration::from_secs(5))
            .timeout(),
        Duration::from_secs(5)
    );
}

#[test]
fn real_runner_returns_output_when_the_timeout_is_not_exceeded() {
    let _lock = serial();
    let area = TempArea::new("quick");
    let script = area.script("gh", QUICK_SCRIPT);
    let runner = RealGhRunner::new(script).with_timeout(Duration::from_secs(10));

    let output = runner
        .run(&["api".to_string()], None)
        .expect("the fake finishes");
    assert_eq!(output, "the output");
}
