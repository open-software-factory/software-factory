//! The moon adapter: runs `moon run <targets> --affected --stdin` for the
//! checkpoint runner and reports what happened, without the runner ever
//! shelling out to moon itself.

use std::io::Write as _;
#[cfg(unix)]
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt as _;

/// The oldest moon version this adapter trusts.
pub const MINIMUM: (u64, u64) = (2, 3);

/// One `moon run` request.
pub struct Invocation<'a> {
    /// The workspace root: where `.moon/` lives, and where moon runs from.
    pub root: &'a Path,
    /// The targets to pass to `moon run`, such as `:#osf-pre-commit`.
    pub targets: &'a [String],
    /// Repository-relative, forward-slash paths for moon's `--stdin`
    /// affected-file selection. Never passed to tasks directly: moon has no
    /// way to forward them, so `OSF_FILES_FROM` carries them instead.
    pub files: &'a [String],
    /// Extra environment variables for the moon process, added to the
    /// inherited parent environment rather than replacing it.
    pub env: &'a [(String, String)],
    /// How long to wait for moon before killing it. `None` waits forever.
    pub timeout: Option<Duration>,
}

/// One task's outcome, read from moon's run report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskOutcome {
    pub target: String,
    pub status: TaskStatus,
    pub duration_ms: u64,
    pub cached: bool,
    /// True when moon's own raw state was `invalid`, never a genuine skip.
    pub invalid: bool,
    /// Moon's own explanation: its action's own `error` text for a
    /// [`TaskStatus::Failed`] task that has one (not every failure does —
    /// a plain `exit 1` carries none), or which task's failure stopped the
    /// run for a [`TaskStatus::NotRun`] one. `None` otherwise.
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskStatus {
    Passed,
    Failed,
    Skipped,
    /// This target was part of the run moon was asked for, but moon never
    /// started it and never gave it a state: a sibling's failure stopped
    /// the run first.
    NotRun,
}

pub enum Outcome {
    Ran {
        exit_code: i32,
        tasks: Vec<TaskOutcome>,
    },
    NothingAffected,
    TimedOut,
    CouldNotRun(String),
}

const NO_AFFECTED_MARKER: &str = "No tasks affected by changed files";

/// The moon binary to run: `OSF_MOON` overrides a `PATH` lookup.
fn moon_binary() -> PathBuf {
    std::env::var_os("OSF_MOON").map_or_else(|| PathBuf::from("moon"), PathBuf::from)
}

/// `osf` always starts a fresh moon for its own workspace, so a
/// nested moon must never inherit an outer moon task's own `MOON_*`
/// variables (`MOON_WORKSPACE_ROOT` and the rest) — moon honours an
/// inherited one over the `current_dir` this adapter passes, which would
/// run the nested moon against the outer task's workspace instead of this
/// one. Removes every `MOON_*` variable this process has from `command`'s
/// environment; explicit `env()` calls made after this still apply, since
/// this only removes what would otherwise be inherited.
fn strip_inherited_moon_vars(command: &mut Command) {
    for (key, _) in std::env::vars() {
        if key.starts_with("MOON_") {
            command.env_remove(key);
        }
    }
}

/// The installed moon's version, parsed from `moon --version`'s `moon
/// X.Y.Z` output.
///
/// # Errors
/// Returns an error when moon cannot run, or its version output does not
/// parse.
pub fn version(root: &Path) -> Result<(u64, u64, u64), String> {
    let mut command = Command::new(moon_binary());
    command.arg("--version").current_dir(root);
    strip_inherited_moon_vars(&mut command);
    let output = command
        .output()
        .map_err(|e| format!("cannot run moon: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "moon --version failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_version(text.trim())
        .ok_or_else(|| format!("cannot parse moon version from '{}'", text.trim()))
}

/// Parses `moon 2.5.5` into `(2, 5, 5)`.
fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let version = text.strip_prefix("moon ")?;
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// One line per file, in stdin order, using the platform's native
/// separator. Callers pass forward-slash repository-relative
/// paths; this is where they are converted.
fn stdin_payload(files: &[String]) -> Vec<u8> {
    let mut payload = String::new();
    for file in files {
        payload.push_str(&file.replace('/', std::path::MAIN_SEPARATOR_STR));
        payload.push('\n');
    }
    payload.into_bytes()
}

/// The run report's path under a workspace root.
fn report_path(root: &Path) -> PathBuf {
    root.join(".moon").join("cache").join("runReport.json")
}

/// Where moon 2.5.5 writes a task's own captured stdout and stderr:
/// `.moon/cache/states/<project>/<task>/{stdout,stderr}.log`, checked by
/// running a real failing task and inspecting the cache directory it left.
fn task_log_paths(root: &Path, target: &str) -> (PathBuf, PathBuf) {
    let (project, task) = target.rsplit_once(':').unwrap_or(("", target));
    let dir = root
        .join(".moon")
        .join("cache")
        .join("states")
        .join(project)
        .join(task);
    (dir.join("stdout.log"), dir.join("stderr.log"))
}

/// One log stream's tail, capped to its last `max_lines` lines: the text
/// shown, how many lines that is, and how many lines the file actually
/// held. `None` when the file is missing or holds no content, since there
/// is nothing to show.
fn read_log_tail(path: &Path, max_lines: usize) -> Option<(String, usize, usize)> {
    let text = std::fs::read_to_string(path).ok()?;
    if text.trim().is_empty() {
        return None;
    }
    let mut lines: Vec<&str> = text.lines().collect();
    let total = lines.len();
    let start = total.saturating_sub(max_lines);
    let shown = lines.split_off(start);
    Some((shown.join("\n"), shown.len(), total))
}

/// A task's own captured stdout and stderr, each independently capped to
/// its last `max_lines` lines. Kept as two separate tails
/// rather than one combined-and-capped text: a task's stderr alone can
/// exceed the cap (a compiler's warnings, say), which would silently push
/// every line of its stdout (a failing test's own name) out of a single
/// shared tail.
#[must_use]
pub fn task_output_tail(root: &Path, target: &str, max_lines: usize) -> TaskOutputTail {
    let (stdout_path, stderr_path) = task_log_paths(root, target);
    TaskOutputTail {
        stdout: read_log_tail(&stdout_path, max_lines),
        stderr: read_log_tail(&stderr_path, max_lines),
    }
}

/// One failed task's captured output, one entry per stream. Each entry is
/// `None` when moon wrote that log file empty or not at all.
pub struct TaskOutputTail {
    pub stdout: Option<(String, usize, usize)>,
    pub stderr: Option<(String, usize, usize)>,
}

/// Removes a stale run report before spawning moon, so a leftover report
/// from an earlier run is never read as this run's result.
fn clear_report(root: &Path) -> Result<(), String> {
    match std::fs::remove_file(report_path(root)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!(
            "cannot remove stale run report at {}: {e}",
            report_path(root).display()
        )),
    }
}

/// Runs `moon run <targets> --affected --stdin` for `inv`.
#[must_use]
pub fn run(inv: &Invocation) -> Outcome {
    match version(inv.root) {
        Ok(v) if v >= (MINIMUM.0, MINIMUM.1, 0) => {}
        Ok(v) => {
            return Outcome::CouldNotRun(format!(
                "moon {}.{}.{} is older than the minimum {}.{}",
                v.0, v.1, v.2, MINIMUM.0, MINIMUM.1
            ))
        }
        Err(e) => return Outcome::CouldNotRun(e),
    }
    if let Err(e) = clear_report(inv.root) {
        return Outcome::CouldNotRun(e);
    }

    let mut command = Command::new(moon_binary());
    command
        .arg("run")
        .args(inv.targets)
        .arg("--affected")
        .arg("--stdin")
        .current_dir(inv.root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    strip_inherited_moon_vars(&mut command);
    // GIT_* stays inherited here: a pre-commit task reads GIT_INDEX_FILE during `git commit -a`.
    for (key, value) in inv.env {
        command.env(key, value);
    }
    // On Unix, moon becomes its own process-group leader so a timeout can
    // kill the group; on Windows, `taskkill /T` walks the
    // parent-child tree instead, so no extra spawn setup is needed there.
    #[cfg(unix)]
    command.process_group(0);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => return Outcome::CouldNotRun(format!("cannot run moon: {e}")),
    };

    // Reader threads must start before the stdin write below: a large file
    // list can fill the stdout/stderr pipe buffers while moon is still
    // reading stdin, and nothing would be draining them yet otherwise.
    let stdout_reader = child
        .stdout
        .take()
        .map(|mut pipe| std::thread::spawn(move || read_all(&mut pipe)));
    let stderr_reader = child
        .stderr
        .take()
        .map(|mut pipe| std::thread::spawn(move || read_all(&mut pipe)));

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(&stdin_payload(inv.files)) {
            return Outcome::CouldNotRun(format!("cannot write to moon's stdin: {e}"));
        }
    }

    let status = match inv.timeout {
        Some(timeout) => match child.wait_timeout(timeout) {
            Ok(Some(status)) => status,
            Ok(None) => return timed_out(&mut child),
            Err(e) => return Outcome::CouldNotRun(format!("cannot wait for moon: {e}")),
        },
        None => match child.wait() {
            Ok(status) => status,
            Err(e) => return Outcome::CouldNotRun(format!("cannot wait for moon: {e}")),
        },
    };

    let stderr_text = stderr_reader.map(|h| h.join().unwrap_or_default());
    let _stdout_text = stdout_reader.map(|h| h.join().unwrap_or_default());

    if stderr_text.is_some_and(|text| text.contains(NO_AFFECTED_MARKER)) {
        return Outcome::NothingAffected;
    }

    let path = report_path(inv.root);
    let report_text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => return Outcome::CouldNotRun(format!("cannot read {}: {e}", path.display())),
    };
    match parse_report(&report_text) {
        Ok(tasks) => Outcome::Ran {
            exit_code: status.code().unwrap_or(-1),
            tasks,
        },
        Err(e) => Outcome::CouldNotRun(e),
    }
}

/// A timeout elapsed: kills moon's whole process tree, then reaps it.
/// A tree-kill command that fails becomes `CouldNotRun` naming
/// the failure, since the caller cannot otherwise know a task might still
/// be running; a direct kill of moon itself follows as a fallback so
/// `wait` below cannot hang on a tree-kill command that never started.
fn timed_out(child: &mut Child) -> Outcome {
    let killed = kill_tree(child);
    let _ = child.kill();
    let _ = child.wait();
    match killed {
        Ok(()) => Outcome::TimedOut,
        Err(e) => Outcome::CouldNotRun(format!("moon timed out and could not be killed: {e}")),
    }
}

/// Kills `child` and every process it spawned: `taskkill /PID <pid> /T /F`
/// on Windows, `kill -KILL` on the process group on Unix (no new crate).
fn kill_tree(child: &Child) -> Result<(), String> {
    let pid = child.id();
    #[cfg(windows)]
    let output = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
    #[cfg(unix)]
    let output = Command::new("kill")
        .args(["-KILL", "--", &format!("-{pid}")])
        .output();
    match output {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// Reads a pipe to the end, discarding a read error's content but not its
/// occurrence: a partial read is still worth the caller having.
fn read_all(pipe: &mut impl std::io::Read) -> String {
    let mut buf = String::new();
    let _ = std::io::Read::read_to_string(pipe, &mut buf);
    buf
}

/// The moon targets tagged `tag`, from `moon query tasks --tags <tag>`.
/// Moon itself decides tag membership across every project in the
/// workspace, so this reads exactly the set `moon run :#<tag>` is about to
/// select, rather than this crate guessing at it by re-reading a project's
/// own YAML.
///
/// # Errors
/// Returns an error when moon cannot run, exits non-zero, or its output
/// does not parse.
pub fn task_targets_for_tag(root: &Path, tag: &str) -> Result<Vec<String>, String> {
    let mut command = Command::new(moon_binary());
    command
        .args(["query", "tasks", "--tags", tag])
        .current_dir(root);
    strip_inherited_moon_vars(&mut command);
    let output = command
        .output()
        .map_err(|e| format!("cannot run moon query tasks: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "moon query tasks failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    parse_query_tasks(&String::from_utf8_lossy(&output.stdout))
}

/// Parses `moon query tasks`' JSON: one target per task, across every
/// project the query returned.
fn parse_query_tasks(json: &str) -> Result<Vec<String>, String> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| format!("moon query tasks output is not JSON: {e}"))?;
    let projects = value
        .get("tasks")
        .and_then(serde_json::Value::as_object)
        .ok_or("moon query tasks output has no tasks object")?;
    let mut targets = Vec::new();
    for tasks in projects.values() {
        let Some(tasks) = tasks.as_object() else {
            continue;
        };
        for task in tasks.values() {
            if let Some(target) = task.get("target").and_then(serde_json::Value::as_str) {
                targets.push(target.to_string());
            }
        }
    }
    Ok(targets)
}

/// Parses moon's `runReport.json`, one [`TaskOutcome`] per target moon was
/// asked to run: every entry in `context.primaryTargets`, plus any target
/// `context.targetStates` or the `actions` array names but `primaryTargets`
/// does not (older reports this adapter's tests build by hand omit
/// `primaryTargets` entirely, so those targets still surface).
///
/// A target's own `RunTask(...)` action, when moon ran one, is the
/// authoritative source for its status: `context.targetStates[*].state` can
/// still say "passed" for a task moon's action list marks `"failed"` with
/// its own `error` text (moon reported exactly this after a task exited 0
/// but left a declared output missing). A target with neither an action
/// nor a `targetStates` entry never started at all — moon stopped the run
/// before reaching it — and is reported as [`TaskStatus::NotRun`], naming
/// whichever sibling's action failed.
///
/// # Errors
/// Returns an error when `json` is not valid JSON, `context.targetStates`
/// is missing or not an object, or a task's `targetStates` status is not
/// one this adapter recognises.
pub fn parse_report(json: &str) -> Result<Vec<TaskOutcome>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("run report is not JSON: {e}"))?;
    let target_states = value
        .pointer("/context/targetStates")
        .and_then(serde_json::Value::as_object)
        .ok_or("run report has no context.targetStates object")?;
    let primary_targets = primary_targets(&value);
    let actions = ActionIndex::build(&value);
    let stopped_by = actions.failed_targets();

    let mut tasks = Vec::new();
    for target in ordered_targets(&primary_targets, target_states, &actions) {
        tasks.push(task_outcome(target, &actions, target_states, &stopped_by)?);
    }
    Ok(tasks)
}

/// `context.primaryTargets`: the targets moon was actually asked to run,
/// in the order it named them. Empty when the report has no such field —
/// this adapter's hand-built test fixtures never include it.
fn primary_targets(value: &serde_json::Value) -> Vec<String> {
    value
        .pointer("/context/primaryTargets")
        .and_then(serde_json::Value::as_array)
        .map(|targets| {
            targets
                .iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Every target named by `primary`, `target_states` or `actions`, each
/// listed once, in that order: `primaryTargets` first since it is moon's
/// own declaration of what it meant to run.
fn ordered_targets(
    primary: &[String],
    target_states: &serde_json::Map<String, serde_json::Value>,
    actions: &ActionIndex,
) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut targets = Vec::new();
    for target in primary
        .iter()
        .chain(target_states.keys())
        .chain(actions.results.keys())
    {
        if seen.insert(target.clone()) {
            targets.push(target.clone());
        }
    }
    targets
}

/// One target's [`TaskOutcome`]: from its own `RunTask(...)` action when
/// moon ran one (the ground truth for what happened, over
/// `targetStates`' possibly stale view of it), else from `targetStates`,
/// else [`TaskStatus::NotRun`] — moon never started it.
fn task_outcome(
    target: String,
    actions: &ActionIndex,
    target_states: &serde_json::Map<String, serde_json::Value>,
    stopped_by: &[&str],
) -> Result<TaskOutcome, String> {
    if let Some((status, error)) = actions.results.get(&target) {
        // Only a genuine `error` text from moon's own action becomes the
        // reason here: a plain task failure (a script's own `exit 1`, say)
        // carries none, and the caller's own SARIF-shaped reason already
        // covers that case.
        return Ok(TaskOutcome {
            duration_ms: actions.duration_ms(&target),
            cached: actions.cached.contains(&target),
            invalid: false,
            reason: error.clone(),
            status: *status,
            target,
        });
    }
    if let Some(state) = target_states.get(&target) {
        let raw_status = state
            .get("state")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("task '{target}' has no state field"))?;
        let status = map_status(raw_status)
            .ok_or_else(|| format!("task '{target}' has an unrecognised status '{raw_status}'"))?;
        return Ok(TaskOutcome {
            duration_ms: actions.duration_ms(&target),
            cached: actions.cached.contains(&target),
            invalid: raw_status == "invalid",
            reason: None,
            status,
            target,
        });
    }
    let reason = if stopped_by.is_empty() {
        "moon never started this task".to_string()
    } else {
        format!(
            "moon stopped the run after {} failed",
            stopped_by.join(", ")
        )
    };
    Ok(TaskOutcome {
        duration_ms: 0,
        cached: false,
        invalid: false,
        reason: Some(reason),
        status: TaskStatus::NotRun,
        target,
    })
}

/// What the `actions` array says about each `RunTask(...)` target: its own
/// duration, whether it was a cache hit, and — separately — its own
/// status and, for a failure, its own error text.
struct ActionIndex {
    durations: std::collections::HashMap<String, u64>,
    cached: std::collections::HashSet<String>,
    results: std::collections::HashMap<String, (TaskStatus, Option<String>)>,
}

impl ActionIndex {
    fn build(value: &serde_json::Value) -> Self {
        let mut durations = std::collections::HashMap::new();
        let mut cached = std::collections::HashSet::new();
        let mut results = std::collections::HashMap::new();
        if let Some(actions) = value.get("actions").and_then(serde_json::Value::as_array) {
            for action in actions {
                let Some(target) = action_target(action) else {
                    continue;
                };
                let ms = action
                    .get("duration")
                    .and_then(duration_ms)
                    .unwrap_or_default();
                let raw_status = action.get("status").and_then(serde_json::Value::as_str);
                // `context.targetStates[*].state` says "passed" on a cache
                // hit too; the action's own status is the only place
                // "cached" appears.
                if raw_status == Some("cached") {
                    cached.insert(target.clone());
                }
                durations.insert(target.clone(), ms);
                if let Some(status) = raw_status.and_then(map_status) {
                    let error = action
                        .get("error")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string);
                    results.insert(target, (status, error));
                }
            }
        }
        Self {
            durations,
            cached,
            results,
        }
    }

    fn duration_ms(&self, target: &str) -> u64 {
        self.durations.get(target).copied().unwrap_or_default()
    }

    /// Every target whose own action failed: named in a sibling's
    /// [`TaskStatus::NotRun`] reason as why it never started.
    fn failed_targets(&self) -> Vec<&str> {
        self.results
            .iter()
            .filter(|(_, (status, _))| *status == TaskStatus::Failed)
            .map(|(target, _)| target.as_str())
            .collect()
    }
}

/// The target an `actions` entry's label names, such as `osf:probe` from
/// `RunTask(osf:probe)`.
fn action_target(action: &serde_json::Value) -> Option<String> {
    let label = action.get("label")?.as_str()?;
    let inner = label.strip_prefix("RunTask(")?.strip_suffix(')')?;
    Some(inner.to_string())
}

/// A `{secs, nanos}` duration object, in whole milliseconds.
fn duration_ms(value: &serde_json::Value) -> Option<u64> {
    let secs = value.get("secs")?.as_u64()?;
    let nanos = value.get("nanos")?.as_u64()?;
    Some(secs.saturating_mul(1000) + nanos / 1_000_000)
}

/// Maps a moon target state's status to this adapter's status. A passed or
/// cached value is a pass; a failed value is a fail; a skipped or invalid
/// value is skipped. Any other value is unrecognised, so an unknown status
/// never reads as a pass. Whether the task was a cache hit is a separate
/// question, answered from the matching action's own status, not this one
/// (`targetStates[*].state` says "passed" on a cache hit too).
fn map_status(raw: &str) -> Option<TaskStatus> {
    match raw {
        "passed" | "cached" => Some(TaskStatus::Passed),
        "failed" => Some(TaskStatus::Failed),
        "skipped" | "invalid" => Some(TaskStatus::Skipped),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    const REPORT: &str = include_str!("../tests/fixtures/moon/run-report.json");
    const CACHED_REPORT: &str = include_str!("../tests/fixtures/moon/run-report-cached.json");
    const QUERY_TASKS: &str = include_str!("../tests/fixtures/moon/query-tasks.json");
    const REVIEW_OUTPUT_MISSING_REPORT: &str =
        include_str!("../tests/fixtures/moon/run-report-review-output-missing.json");

    #[test]
    fn the_captured_report_parses_into_one_outcome_per_task() {
        let tasks = parse_report(REPORT).expect("report parses");
        assert!(!tasks.is_empty());
        assert!(
            tasks.iter().any(|t| t.target.ends_with(":probe")),
            "{:?}",
            tasks.iter().map(|t| &t.target).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_report_that_is_not_json_is_an_error() {
        assert!(parse_report("not json").is_err());
    }

    #[test]
    fn the_probe_task_in_the_captured_report_passed_and_was_not_cached() {
        let tasks = parse_report(REPORT).expect("report parses");
        let probe = tasks
            .iter()
            .find(|t| t.target.ends_with(":probe"))
            .expect("probe task present");
        assert_eq!(probe.status, TaskStatus::Passed);
        assert!(!probe.cached);
        assert_eq!(probe.duration_ms, 400);
    }

    /// `context.targetStates[*].state` says "passed" on a cache hit
    /// too; only the `RunTask(<target>)` action's own `status` says
    /// "cached". This fixture captures exactly that shape.
    #[test]
    fn a_cached_run_task_action_reports_cached_true() {
        let tasks = parse_report(CACHED_REPORT).expect("cached report parses");
        let probe = tasks
            .iter()
            .find(|t| t.target.ends_with(":probe"))
            .expect("probe task present");
        assert_eq!(probe.status, TaskStatus::Passed);
        assert!(probe.cached, "{tasks:?}");
    }

    #[test]
    fn an_unrecognised_status_is_an_error() {
        let json = r#"{"actions":[],"context":{"targetStates":{"a:b":{"state":"mystery"}}}}"#;
        assert!(parse_report(json).is_err());
    }

    /// An `invalid` status still maps to skipped, but is flagged separately.
    #[test]
    fn an_invalid_status_maps_to_skipped_and_is_flagged_invalid() {
        let json = r#"{"actions":[],"context":{"targetStates":{"a:b":{"state":"invalid"}}}}"#;
        let tasks = parse_report(json).expect("report parses");
        let task = tasks.first().expect("one task");
        assert_eq!(task.status, TaskStatus::Skipped);
        assert!(task.invalid, "{task:?}");
    }

    #[test]
    fn a_genuine_skipped_status_is_not_flagged_invalid() {
        let json = r#"{"actions":[],"context":{"targetStates":{"a:b":{"state":"skipped"}}}}"#;
        let tasks = parse_report(json).expect("report parses");
        let task = tasks.first().expect("one task");
        assert_eq!(task.status, TaskStatus::Skipped);
        assert!(!task.invalid, "{task:?}");
    }

    /// A task that exits 0 but leaves a declared output missing is a
    /// captured, real moon report: its `RunTask(...)` action's own status
    /// is `"failed"` with moon's own `error` text, even though
    /// `targetStates` still says `"passed"`. The action's status must win.
    #[test]
    fn a_task_moon_failed_for_a_missing_output_is_reported_failed_with_moons_reason() {
        let tasks = parse_report(REVIEW_OUTPUT_MISSING_REPORT).expect("report parses");
        let review = tasks
            .iter()
            .find(|t| t.target == "osf:review")
            .expect("osf:review present");
        assert_eq!(review.status, TaskStatus::Failed, "{review:?}");
        let reason = review.reason.as_deref().expect("a reason");
        assert!(reason.contains("defines outputs"), "{reason}");
    }

    /// The same report's other primary targets never got an action or a
    /// `targetStates` entry at all: moon stopped the run after
    /// `osf:review` failed, before it reached them. Each is `NotRun`, not
    /// a failure with an empty reason.
    #[test]
    fn a_sibling_moon_never_started_is_reported_not_run_naming_the_task_that_stopped_it() {
        let tasks = parse_report(REVIEW_OUTPUT_MISSING_REPORT).expect("report parses");
        for target in ["osf:scan-commits", "osf:scan-pre-push", "osf:test"] {
            let sibling = tasks
                .iter()
                .find(|t| t.target == target)
                .unwrap_or_else(|| panic!("{target} present in {tasks:?}"));
            assert_eq!(sibling.status, TaskStatus::NotRun, "{sibling:?}");
            let reason = sibling.reason.as_deref().expect("a reason");
            assert!(reason.contains("osf:review"), "{reason}");
        }
    }

    /// `osf:clippy` and `osf:lint-writing-pre-push` both genuinely ran (as
    /// cache hits) before moon stopped: they stay passed, unaffected by
    /// `osf:review`'s failure.
    #[test]
    fn tasks_that_ran_before_the_stop_are_unaffected() {
        let tasks = parse_report(REVIEW_OUTPUT_MISSING_REPORT).expect("report parses");
        for target in ["osf:clippy", "osf:lint-writing-pre-push"] {
            let task = tasks
                .iter()
                .find(|t| t.target == target)
                .unwrap_or_else(|| panic!("{target} present in {tasks:?}"));
            assert_eq!(task.status, TaskStatus::Passed, "{task:?}");
            assert!(task.cached, "{task:?}");
        }
    }

    #[test]
    fn moon_s_own_version_output_parses() {
        assert_eq!(parse_version("moon 2.5.5"), Some((2, 5, 5)));
    }

    #[test]
    fn an_unparseable_version_string_is_none() {
        assert_eq!(parse_version("2.5.5"), None);
        assert_eq!(parse_version("moon two"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn the_captured_query_parses_into_one_target_per_task() {
        let targets = parse_query_tasks(QUERY_TASKS).expect("query parses");
        assert_eq!(targets, vec!["osf:probe".to_string()]);
    }

    #[test]
    fn a_query_with_no_matching_tasks_is_an_empty_list() {
        let targets = parse_query_tasks(r#"{"tasks":{}}"#).expect("empty query parses");
        assert!(targets.is_empty());
    }

    #[test]
    fn a_query_that_is_not_json_is_an_error() {
        assert!(parse_query_tasks("not json").is_err());
    }

    #[test]
    fn a_query_missing_the_tasks_object_is_an_error() {
        assert!(parse_query_tasks(r#"{"options":{}}"#).is_err());
    }

    #[test]
    fn no_log_files_at_all_is_two_empty_streams() {
        let root = TempDir::new("osf-moon-tail-test-missing");
        let tail = task_output_tail(&root, "proj:boom", 80);
        assert!(tail.stdout.is_none(), "{:?}", tail.stdout);
        assert!(tail.stderr.is_none(), "{:?}", tail.stderr);
    }

    #[test]
    fn each_stream_under_the_cap_is_returned_whole() {
        let root = TempDir::new("osf-moon-tail-test-small");
        let dir = root
            .join(".moon")
            .join("cache")
            .join("states")
            .join("proj")
            .join("boom");
        std::fs::create_dir_all(&dir).expect("dir creates");
        std::fs::write(dir.join("stdout.log"), "one\ntwo\n").expect("stdout writes");
        std::fs::write(dir.join("stderr.log"), "three\n").expect("stderr writes");
        let tail = task_output_tail(&root, "proj:boom", 80);
        let (text, shown, total) = tail.stdout.expect("stdout present");
        assert_eq!(text, "one\ntwo");
        assert_eq!(shown, 2);
        assert_eq!(total, 2);
        let (text, shown, total) = tail.stderr.expect("stderr present");
        assert_eq!(text, "three");
        assert_eq!(shown, 1);
        assert_eq!(total, 1);
    }

    #[test]
    fn a_long_stderr_does_not_push_stdout_out_of_its_own_tail() {
        use std::fmt::Write as _;
        let root = TempDir::new("osf-moon-tail-test-large");
        let dir = root
            .join(".moon")
            .join("cache")
            .join("states")
            .join("proj")
            .join("boom");
        std::fs::create_dir_all(&dir).expect("dir creates");
        std::fs::write(dir.join("stdout.log"), "stdout marker\n").expect("stdout writes");
        let mut content = String::new();
        for n in 1..=100 {
            writeln!(content, "line {n}").expect("writing to a string never fails");
        }
        std::fs::write(dir.join("stderr.log"), content).expect("stderr writes");
        let tail = task_output_tail(&root, "proj:boom", 10);
        let (text, shown, total) = tail.stdout.expect("stdout present");
        assert_eq!(text, "stdout marker");
        assert_eq!(shown, 1);
        assert_eq!(total, 1);
        let (text, shown, total) = tail.stderr.expect("stderr present");
        assert_eq!(shown, 10);
        assert_eq!(total, 100);
        assert!(text.starts_with("line 91"), "{text}");
        assert!(text.ends_with("line 100"), "{text}");
    }

    #[test]
    fn the_real_moon_reports_a_version_at_or_above_the_minimum() {
        let root = TempDir::new("osf-moon-version");
        let (major, minor, _) = version(&root).expect("moon --version runs and parses");
        assert!(
            (major, minor) >= MINIMUM,
            "moon {major}.{minor} is below the minimum {}.{}",
            MINIMUM.0,
            MINIMUM.1
        );
    }
}
