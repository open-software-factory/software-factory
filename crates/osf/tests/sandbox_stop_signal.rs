//! `osf sandbox run` stays stoppable: a SIGINT or SIGTERM removes the
//! sandbox before osf exits. A fake `docker` shell script stands in for the
//! installed tool.

#![cfg(unix)]

mod common;

use common::{isolated_home, TempDir};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// Held for a whole test: the fake `docker` script is a real file, and two
/// tests spawning at once would race on their folders.
static SPAWN_LOCK: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SPAWN_LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The 64-character lowercase hexadecimal id the fake `create` prints.
const CREATE_ID: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Writes the fake `docker` script: `create_body` runs in `create`, `exec_body` in `exec`.
fn fake_docker(dir: &Path, create_body: &str, exec_body: &str) {
    let path = dir.join("docker");
    let body = format!(
        r#"#!/bin/sh
dir="$(cd "$(dirname "$0")" && pwd)"
printf '%s\n' "$*" >> "$dir/calls.log"
case "$1" in
create)
  {create_body}
  : > "$dir/created"
  printf 'create-done\n' >> "$dir/calls.log"
  printf '{CREATE_ID}\n'
  exit 0 ;;
start) exit 0 ;;
exec)
  prev=
  last=
  for word in "$@"; do prev="$last"; last="$word"; done
  if [ "$prev" = "id" ] && [ "$last" = "-u" ]; then printf '1000\n'; exit 0; fi
  {exec_body}
  ;;
inspect)
  if [ ! -f "$dir/created" ]; then printf 'Error: No such container: x\n' >&2; exit 1; fi
  printf '{CREATE_ID}\n'
  exit 0 ;;
rm) exit 0 ;;
*) exit 1 ;;
esac
"#
    );
    fs::write(&path, body).expect("the fake docker writes");
    let mut perms = fs::metadata(&path)
        .expect("the fake docker has metadata")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("the fake docker is executable");
}

/// `dir` first on `PATH`, so the spawned `osf` finds the fake `docker`.
fn path_with_first(dir: &Path) -> std::ffi::OsString {
    let ambient = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(std::iter::once(dir.to_path_buf()).chain(std::env::split_paths(&ambient)))
        .expect("PATH joins")
}

/// The `osf sandbox run` arguments for one test, ending in `sleep 100`.
fn run_args(repo: &Path, state: &Path, name: &str) -> Vec<String> {
    vec![
        "sandbox".to_string(),
        "run".to_string(),
        "--image".to_string(),
        "example/base:1".to_string(),
        "--repo".to_string(),
        repo.to_string_lossy().into_owned(),
        "--state".to_string(),
        state.to_string_lossy().into_owned(),
        "--name".to_string(),
        name.to_string(),
        "--".to_string(),
        "sleep".to_string(),
        "100".to_string(),
    ]
}

/// The `osf` command with `home` for HOME and `dir` first on PATH.
fn osf_command(dir: &Path, home: &Path, fake_dir: &Path, args: &[String]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_osf"));
    command
        .current_dir(dir)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("PATH", path_with_first(fake_dir))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, _) in std::env::vars() {
        if key.starts_with("OSF_") || key.starts_with("MOON_") || key.starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
}

/// Waits until `path` exists, failing after `limit`.
fn wait_for(path: &Path, limit: Duration) {
    let started = Instant::now();
    while !path.exists() {
        assert!(
            started.elapsed() < limit,
            "the marker never appeared: {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Sends `signal` (like `-TERM`) to `child`.
fn send_signal(child: &Child, signal: &str) {
    let pid = child.id().to_string();
    let status = Command::new("kill")
        .args([signal, pid.as_str()])
        .status()
        .expect("kill runs");
    assert!(status.success(), "kill {signal} failed");
}

/// Spawns a long run, stops it with `signal`, and returns the fake's log and
/// the stopped process's output.
fn stopped_run(name: &str, signal: &str) -> (String, std::process::Output) {
    let area = TempDir::new(name);
    let home = isolated_home(name);
    let repo = TempDir::new(&format!("{name}-repo"));
    let state = TempDir::new(&format!("{name}-state"));
    let marker = area.join("exec-started");
    fake_docker(&area, "", "touch \"$dir/exec-started\"; sleep 5; exit 0");
    let args = run_args(&repo, &state, name);
    let child = osf_command(&repo, &home, &area, &args)
        .spawn()
        .expect("osf spawns");
    wait_for(&marker, Duration::from_secs(10));
    send_signal(&child, signal);
    let output = child.wait_with_output().expect("osf exits");
    let log = fs::read_to_string(area.join("calls.log")).expect("the fake logged its calls");
    (log, output)
}

#[test]
fn a_sigterm_removes_the_sandbox_and_exits_143() {
    let _lock = serial();
    let (log, output) = stopped_run("osf-signal-test", "-TERM");
    assert_eq!(output.status.code(), Some(143), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("stopped by signal 15"), "{stderr}");
    assert!(
        log.lines()
            .any(|line| line.starts_with(&format!("rm --force -- {CREATE_ID}"))),
        "{log}"
    );
}

#[test]
fn a_sigint_removes_the_sandbox_and_exits_130() {
    let _lock = serial();
    let (log, output) = stopped_run("osf-signal-test", "-INT");
    assert_eq!(output.status.code(), Some(130), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("stopped by signal 2"), "{stderr}");
    assert!(
        log.lines()
            .any(|line| line.starts_with(&format!("rm --force -- {CREATE_ID}"))),
        "{log}"
    );
}

#[test]
fn a_sigterm_during_create_waits_for_the_create_then_removes_the_sandbox() {
    let _lock = serial();
    let area = TempDir::new("osf-signal-create");
    let home = isolated_home("osf-signal-create");
    let repo = TempDir::new("osf-signal-create-repo");
    let state = TempDir::new("osf-signal-create-state");
    let started = area.join("create-started");
    fake_docker(&area, "touch \"$dir/create-started\"; sleep 2", "");
    let args = run_args(&repo, &state, "osf-signal-create");
    let child = osf_command(&repo, &home, &area, &args)
        .spawn()
        .expect("osf spawns");
    wait_for(&started, Duration::from_secs(10));
    send_signal(&child, "-TERM");
    let output = child.wait_with_output().expect("osf exits");
    assert_eq!(output.status.code(), Some(143), "{output:?}");
    let log = fs::read_to_string(area.join("calls.log")).expect("the fake logged its calls");
    let lines: Vec<&str> = log.lines().collect();
    let create_done = lines
        .iter()
        .position(|line| *line == "create-done")
        .expect("the create finished");
    let remove_line = format!("rm --force -- {CREATE_ID}");
    let removed = lines
        .iter()
        .position(|line| *line == remove_line.as_str())
        .expect("the sandbox is removed");
    assert!(removed > create_done, "{log}");
    // The create-time user check is the only exec; the requested command must not run.
    let execs: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| line.starts_with("exec"))
        .collect();
    assert_eq!(
        execs.len(),
        1,
        "only the create-time user check runs: {log}"
    );
    assert!(
        execs.first().is_some_and(|line| line.ends_with("id -u")),
        "{log}"
    );
    assert!(area.join("created").exists(), "the sandbox was created");
}

#[test]
fn a_normal_run_removes_the_sandbox_once_and_exits_zero() {
    let _lock = serial();
    let area = TempDir::new("osf-signal-normal");
    let home = isolated_home("osf-signal-normal");
    let repo = TempDir::new("osf-signal-normal-repo");
    let state = TempDir::new("osf-signal-normal-state");
    fake_docker(&area, "", "exit 0");
    let args = run_args(&repo, &state, "osf-signal-normal");
    let output = osf_command(&repo, &home, &area, &args)
        .output()
        .expect("osf runs");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let log = fs::read_to_string(area.join("calls.log")).expect("the fake logged its calls");
    let removes = log.lines().filter(|line| line.starts_with("rm ")).count();
    assert_eq!(removes, 1, "{log}");
}
