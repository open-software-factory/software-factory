//! `osf verify --checkpoint <name>`: selects this repository's moon tasks
//! tagged for a checkpoint, runs them on the affected files, and writes one
//! verification event per task plus one checkpoint-complete event to the
//! local journal buffer.

use crate::journal::{CheckResult, CheckpointComplete, Journal, Payload, Verification};
use crate::moon::{self, Invocation, Outcome, TaskStatus};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A point where checks run. There are five: the harness hook, pre-commit,
/// pre-push, the pull request and the schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Checkpoint {
    Hook,
    PreCommit,
    PrePush,
    PullRequest,
    Schedule,
}

impl Checkpoint {
    /// The moon task tag this checkpoint selects.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Checkpoint::Hook => "osf-hook",
            Checkpoint::PreCommit => "osf-pre-commit",
            Checkpoint::PrePush => "osf-pre-push",
            Checkpoint::PullRequest => "osf-pull-request",
            Checkpoint::Schedule => "osf-schedule",
        }
    }

    /// The name this checkpoint answers to in output and journal events.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Checkpoint::Hook => "hook",
            Checkpoint::PreCommit => "pre-commit",
            Checkpoint::PrePush => "pre-push",
            Checkpoint::PullRequest => "pull-request",
            Checkpoint::Schedule => "schedule",
        }
    }
}

/// One checkpoint invocation: where the repository lives, which checkpoint
/// is calling, what to diff against, the caller's own file list (the hook
/// checkpoint's only source of files), and how long to let moon run.
pub struct Request<'a> {
    pub root: &'a Path,
    pub checkpoint: Checkpoint,
    pub base: Option<String>,
    pub files: Option<Vec<String>>,
    pub timeout: Option<Duration>,
}

/// What one checkpoint run produced: the overall result, the lines to print
/// (one per task, then a total), the findings text for a hook's reply (and
/// any other problem worth a line on standard error), and a journal error
/// when the evidence itself could not be kept.
pub struct Summary {
    pub result: CheckResult,
    pub lines: Vec<String>,
    pub error_findings: Vec<String>,
    pub journal_error: Option<String>,
}

fn now_millis() -> u64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}

fn could_not_run(detail: String) -> Summary {
    Summary {
        result: CheckResult::CouldNotRun,
        lines: Vec::new(),
        error_findings: vec![detail],
        journal_error: None,
    }
}

/// The base to diff against: given verbatim for pre-push and pull-request,
/// else the repository's default branch. Every other checkpoint keeps
/// whatever base the caller passed, untouched, since it plays no part in
/// choosing their files.
fn resolve_base(req: &Request) -> Result<Option<String>, String> {
    match req.checkpoint {
        Checkpoint::PrePush | Checkpoint::PullRequest => match &req.base {
            Some(b) => Ok(Some(b.clone())),
            None => crate::git::default_branch(req.root)
                .map(Some)
                .map_err(|e| e.to_string()),
        },
        Checkpoint::Hook | Checkpoint::PreCommit | Checkpoint::Schedule => Ok(req.base.clone()),
    }
}

/// The files this checkpoint runs over, normalised to forward slashes.
fn resolve_files(req: &Request, base: Option<&str>) -> Result<Vec<String>, String> {
    let files = match req.checkpoint {
        Checkpoint::Hook => req.files.clone().unwrap_or_default(),
        Checkpoint::PreCommit => crate::git::staged_files(req.root).map_err(|e| e.to_string())?,
        Checkpoint::PrePush | Checkpoint::PullRequest => {
            let base = base.ok_or_else(|| {
                "a base is required to select pre-push or pull-request files".to_string()
            })?;
            crate::git::changed_files(req.root, base).map_err(|e| e.to_string())?
        }
        Checkpoint::Schedule => {
            crate::git::tracked_files(req.root, None).map_err(|e| e.to_string())?
        }
    };
    Ok(files.iter().map(|p| p.replace('\\', "/")).collect())
}

/// Staged files that also have edits in the working tree beyond what is
/// staged: the scan checks the index, a moon task's own tooling reads the
/// working tree, so both versions of such a file were checked, and the
/// author is told which files that applies to.
fn partially_staged(root: &Path, staged: &[String]) -> Vec<String> {
    let Ok(unstaged) = crate::git::unstaged_files(root) else {
        return Vec::new();
    };
    let unstaged: std::collections::BTreeSet<&str> = unstaged.iter().map(String::as_str).collect();
    staged
        .iter()
        .filter(|f| unstaged.contains(f.as_str()))
        .cloned()
        .collect()
}

/// Writes `files`, one per line, to a fresh file under `state_dir/files`,
/// for `OSF_FILES_FROM` (ruling R7). Moon itself cannot pass a changed-file
/// list to a task, so this is how each task's `osf check` learns it.
fn write_files_from(state_dir: &Path, run: &str, files: &[String]) -> Result<PathBuf, String> {
    let dir = state_dir.join("files");
    std::fs::create_dir_all(&dir).map_err(|e| {
        format!(
            "cannot create the checkpoint files directory {}: {e}",
            dir.display()
        )
    })?;
    let path = dir.join(format!("{run}.txt"));
    let mut content = String::new();
    for file in files {
        content.push_str(file);
        content.push('\n');
    }
    std::fs::write(&path, content).map_err(|e| {
        format!(
            "cannot write the checkpoint file list {}: {e}",
            path.display()
        )
    })?;
    Ok(path)
}

/// The task id a moon target names: the part after its last `:`.
fn task_id(target: &str) -> &str {
    target.rsplit(':').next().unwrap_or(target)
}

/// The findings an `osf check` task wrote as SARIF, rendered one line each
/// as `<rule>: <message>`. Missing or unreadable SARIF reads as no
/// findings: that is correct for a task that passed, and the only signal
/// available for one that did not.
fn sarif_findings(root: &Path, target: &str) -> Vec<String> {
    let path = root
        .join(".osf")
        .join("out")
        .join(format!("{}.sarif", task_id(target)));
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    let Some(runs) = value.get("runs").and_then(serde_json::Value::as_array) else {
        return lines;
    };
    for run in runs {
        let Some(results) = run.get("results").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for result in results {
            let rule = result
                .get("ruleId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            let message = result
                .get("message")
                .and_then(|m| m.get("text"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            lines.push(format!("{rule}: {message}"));
        }
    }
    lines
}

const ACTOR: &str = "osf";

/// Appends a checkpoint-complete event when a journal is open, folding any
/// append error into `journal_error` without ever losing an earlier one.
fn append_checkpoint_complete(
    journal: &mut Option<Journal>,
    label: &str,
    commit: Option<String>,
    result: CheckResult,
    checks: u32,
    journal_error: &mut Option<String>,
) {
    let Some(j) = journal.as_mut() else {
        return;
    };
    if let Err(e) = j.append(
        ACTOR,
        now_millis(),
        Payload::CheckpointComplete(CheckpointComplete {
            checkpoint: label.to_string(),
            commit,
            result,
            checks,
        }),
    ) {
        journal_error.get_or_insert(e);
    }
}

/// One task's outcome: its journal result, its rendered summary line, and
/// its SARIF finding lines (kept separately so the caller only surfaces
/// them for a task that failed).
struct TaskLine {
    result: CheckResult,
    line: String,
    findings: Vec<String>,
    findings_count: u32,
}

fn task_line(root: &Path, task: &moon::TaskOutcome) -> TaskLine {
    let result = match task.status {
        TaskStatus::Passed => CheckResult::Passed,
        TaskStatus::Failed => CheckResult::Failed,
        TaskStatus::Skipped => CheckResult::Skipped,
    };
    let findings = sarif_findings(root, &task.target);
    let findings_count = u32::try_from(findings.len()).unwrap_or(u32::MAX);
    let cache = if task.cached { "hit" } else { "miss" };
    let word = match task.status {
        TaskStatus::Passed => "passed",
        TaskStatus::Failed => "failed",
        TaskStatus::Skipped => "skipped",
    };
    let line = format!(
        "{}: {word} ({findings_count} finding(s), {}ms, cache {cache})",
        task.target, task.duration_ms
    );
    TaskLine {
        result,
        line,
        findings,
        findings_count,
    }
}

/// The `Outcome::Ran` branch of [`run`]: one line and one verification
/// event per task, then the total line and the checkpoint-complete event.
#[allow(clippy::too_many_arguments)]
fn handle_ran(
    req: &Request,
    mut journal: Option<Journal>,
    commit: Option<String>,
    label: &str,
    tasks: &[moon::TaskOutcome],
    mut error_findings: Vec<String>,
    mut journal_error: Option<String>,
) -> Summary {
    let mut lines = Vec::new();
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut skipped = 0u32;
    let mut total_findings = 0u32;
    for task in tasks {
        let outcome = task_line(req.root, task);
        match outcome.result {
            CheckResult::Passed => passed += 1,
            CheckResult::Failed => {
                failed += 1;
                error_findings.extend(outcome.findings.iter().cloned());
            }
            CheckResult::Skipped => skipped += 1,
            CheckResult::CouldNotRun | CheckResult::NothingToCheck => {}
        }
        total_findings += outcome.findings_count;
        lines.push(outcome.line);
        let cache = if task.cached { "hit" } else { "miss" };
        if let Some(j) = journal.as_mut() {
            if let Err(e) = j.append(
                ACTOR,
                now_millis(),
                Payload::Verification(Verification {
                    check: task.target.clone(),
                    slot: None,
                    checkpoint: label.to_string(),
                    result: outcome.result,
                    duration_ms: task.duration_ms,
                    cache: Some(cache.to_string()),
                    findings: outcome.findings_count,
                    grade: "observed".to_string(),
                    reason: None,
                }),
            ) {
                journal_error.get_or_insert(e);
            }
        }
    }
    let overall = if failed > 0 {
        CheckResult::Failed
    } else if passed == 0 && skipped > 0 {
        CheckResult::Skipped
    } else {
        CheckResult::Passed
    };
    lines.push(format!(
        "{label}: {passed} passed, {failed} failed, {skipped} skipped, {total_findings} finding(s) total"
    ));
    let checks = u32::try_from(tasks.len()).unwrap_or(u32::MAX);
    append_checkpoint_complete(
        &mut journal,
        label,
        commit,
        overall,
        checks,
        &mut journal_error,
    );
    Summary {
        result: overall,
        lines,
        error_findings,
        journal_error,
    }
}

/// Everything [`run`] needs before it can call moon: the resolved files,
/// the open journal (`None` only when the hook checkpoint's own journal
/// could not be opened), the task environment, and whatever is already
/// worth reporting.
struct Prepared {
    files: Vec<String>,
    journal: Option<Journal>,
    env: Vec<(String, String)>,
    error_findings: Vec<String>,
    journal_error: Option<String>,
}

/// Resolves `req`'s base and files, notes any partially staged file,
/// writes the `OSF_FILES_FROM` list (ruling R7), and opens the journal.
/// Returns `Err` with the final [`Summary`] when any of that fails badly
/// enough that moon must not run at all.
fn prepare(req: &Request, state_dir: &Path) -> Result<Prepared, Summary> {
    let base = resolve_base(req).map_err(could_not_run)?;
    let files = resolve_files(req, base.as_deref()).map_err(could_not_run)?;

    let mut error_findings = Vec::new();
    if req.checkpoint == Checkpoint::PreCommit {
        let partial = partially_staged(req.root, &files);
        if !partial.is_empty() {
            error_findings.push(format!(
                "staged with unstaged edits, so both versions were checked: {}",
                partial.join(", ")
            ));
        }
    }

    let run_id = format!(
        "{}-{}-{}",
        req.checkpoint.label(),
        now_millis(),
        std::process::id()
    );

    let files_path = match write_files_from(state_dir, &run_id, &files) {
        Ok(p) => p,
        Err(e) => {
            error_findings.push(e);
            return Err(Summary {
                result: CheckResult::CouldNotRun,
                lines: Vec::new(),
                error_findings,
                journal_error: None,
            });
        }
    };

    let journal = match Journal::open(state_dir, &run_id) {
        Ok(j) => Some(j),
        Err(e) if req.checkpoint == Checkpoint::Hook => {
            error_findings.push(e);
            None
        }
        Err(e) => {
            return Err(Summary {
                result: CheckResult::CouldNotRun,
                lines: Vec::new(),
                error_findings,
                journal_error: Some(e),
            });
        }
    };
    let journal_error = if journal.is_none() {
        error_findings.last().cloned()
    } else {
        None
    };

    let mut env: Vec<(String, String)> = Vec::new();
    if let Some(b) = &base {
        env.push(("OSF_BASE".to_string(), b.clone()));
    }
    env.push((
        "OSF_CHECKPOINT".to_string(),
        req.checkpoint.label().to_string(),
    ));
    env.push((
        "OSF_FILES_FROM".to_string(),
        files_path.to_string_lossy().into_owned(),
    ));

    Ok(Prepared {
        files,
        journal,
        env,
        error_findings,
        journal_error,
    })
}

/// Runs `req`'s checkpoint: resolves its files, hands the tagged moon
/// target to `moon::run`, and journals one verification event per task
/// plus one checkpoint-complete event under `state_dir`.
#[must_use]
pub fn run(req: &Request, state_dir: &Path) -> Summary {
    let Prepared {
        files,
        mut journal,
        env,
        mut error_findings,
        mut journal_error,
    } = match prepare(req, state_dir) {
        Ok(p) => p,
        Err(summary) => return summary,
    };

    let target = format!(":#{}", req.checkpoint.tag());
    let outcome = moon::run(&Invocation {
        root: req.root,
        targets: std::slice::from_ref(&target),
        files: &files,
        env: &env,
        timeout: req.timeout,
    });

    let commit = crate::git::head_short_sha(req.root).ok();
    let label = req.checkpoint.label();

    match outcome {
        Outcome::NothingAffected => {
            append_checkpoint_complete(
                &mut journal,
                label,
                commit,
                CheckResult::NothingToCheck,
                0,
                &mut journal_error,
            );
            Summary {
                result: CheckResult::NothingToCheck,
                lines: vec![format!("{label}: nothing to check")],
                error_findings,
                journal_error,
            }
        }
        Outcome::Ran { tasks, .. } => handle_ran(
            req,
            journal,
            commit,
            label,
            &tasks,
            error_findings,
            journal_error,
        ),
        Outcome::TimedOut => {
            // Decision 0011: a check that cannot finish in the hook's time
            // reports skipped with a reason; the pre-commit checkpoint runs
            // it in full. Every other checkpoint treats a timeout as a
            // failure to run at all.
            let result = if req.checkpoint == Checkpoint::Hook {
                CheckResult::Skipped
            } else {
                CheckResult::CouldNotRun
            };
            error_findings.push("moon timed out".to_string());
            append_checkpoint_complete(&mut journal, label, commit, result, 0, &mut journal_error);
            Summary {
                result,
                lines: vec![format!("{label}: moon timed out")],
                error_findings,
                journal_error,
            }
        }
        Outcome::CouldNotRun(reason) => {
            error_findings.push(reason.clone());
            append_checkpoint_complete(
                &mut journal,
                label,
                commit,
                CheckResult::CouldNotRun,
                0,
                &mut journal_error,
            );
            Summary {
                result: CheckResult::CouldNotRun,
                lines: vec![format!("{label}: could not run: {reason}")],
                error_findings,
                journal_error,
            }
        }
    }
}

/// The process exit code for `summary` at `checkpoint`: 0 ran clean or
/// nothing to check, 1 at least one task failed, 2 could not run. A
/// journal error forces 2 everywhere except the hook checkpoint, which
/// still reports its findings and names the failure on standard error
/// (decision 0014; review focus item 5).
#[must_use]
pub fn exit_code(summary: &Summary, checkpoint: Checkpoint) -> u8 {
    if checkpoint != Checkpoint::Hook && summary.journal_error.is_some() {
        return 2;
    }
    match summary.result {
        CheckResult::Failed => 1,
        CheckResult::CouldNotRun => 2,
        CheckResult::Passed | CheckResult::Skipped | CheckResult::NothingToCheck => 0,
    }
}
