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
/// author is told which files that applies to. A git error here is printed
/// as a notice rather than swallowed (M10); the check itself still runs.
fn partially_staged(root: &Path, staged: &[String]) -> Vec<String> {
    let unstaged = match crate::git::unstaged_files(root) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("osf verify: cannot check for files with unstaged edits: {e}");
            return Vec::new();
        }
    };
    let unstaged: std::collections::BTreeSet<&str> = unstaged.iter().map(String::as_str).collect();
    staged
        .iter()
        .filter(|f| unstaged.contains(f.as_str()))
        .cloned()
        .collect()
}

/// Writes `files`, one per line, to a fresh file under the OS temp
/// directory, for `OSF_FILES_FROM` (ruling R7). Moon itself cannot pass a
/// changed-file list to a task, so this is how each task's `osf check`
/// learns it. Ruling R21: this lives outside the journal's own state
/// directory, so an unwritable state dir never blocks it, and the caller
/// deletes it once moon has run, on every path.
fn write_files_from(run: &str, files: &[String]) -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join(format!("osf-checkpoint-files-{run}.txt"));
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

/// Best-effort removal of the `OSF_FILES_FROM` list `write_files_from`
/// wrote: a leftover temp file is not evidence anything depends on, so a
/// removal failure is not worth reporting.
fn cleanup_files_from(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// The task id a moon target names: the part after its last `:`.
fn task_id(target: &str) -> &str {
    target.rsplit(':').next().unwrap_or(target)
}

/// One SARIF result worth counting: never a suppressed one (M2), since a
/// suppressed finding is a decision already made, not something to count
/// or show again.
struct SarifFinding {
    rule: String,
    message: String,
    level: String,
}

/// What reading a task's SARIF output found: every non-suppressed finding;
/// the file was never written; or the file exists but could not be read or
/// parsed.
enum SarifOutcome {
    Findings(Vec<SarifFinding>),
    Missing,
    Unreadable(String),
}

/// The forward-slash, repository-relative label for a task's own SARIF
/// file (M3): a reason built from this reads the same on every operating
/// system, unlike one built from a joined, platform-separated `PathBuf`.
fn sarif_rel_label(target: &str) -> String {
    format!(".osf/out/{}.sarif", task_id(target))
}

/// True when a SARIF result carries a non-empty `suppressions` array.
fn is_suppressed(result: &serde_json::Value) -> bool {
    result
        .get("suppressions")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|s| !s.is_empty())
}

/// Reads the SARIF an `osf check` task wrote to `.osf/out/<task-id>.sarif`.
fn sarif_outcome(root: &Path, target: &str) -> SarifOutcome {
    let path = root
        .join(".osf")
        .join("out")
        .join(format!("{}.sarif", task_id(target)));
    if !path.is_file() {
        return SarifOutcome::Missing;
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => return SarifOutcome::Unreadable(e.to_string()),
    };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return SarifOutcome::Unreadable(e.to_string()),
    };
    let mut findings = Vec::new();
    let Some(runs) = value.get("runs").and_then(serde_json::Value::as_array) else {
        return SarifOutcome::Findings(findings);
    };
    for run in runs {
        let Some(results) = run.get("results").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for result in results {
            if is_suppressed(result) {
                continue;
            }
            let rule = result
                .get("ruleId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            let message = result
                .get("message")
                .and_then(|m| m.get("text"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let level = result
                .get("level")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("warning");
            findings.push(SarifFinding {
                rule: rule.to_string(),
                message: message.to_string(),
                level: level.to_string(),
            });
        }
    }
    SarifOutcome::Findings(findings)
}

/// Ruling R13: a SARIF file left over from an earlier run must never be
/// read as this run's result. Deletes `.osf/out/<task-id>.sarif` for every
/// task tagged `tag` before moon runs, reading the task set from moon's own
/// `query tasks` rather than this crate's guess at what a project file's
/// tags resolve to.
fn clear_stale_sarif(root: &Path, tag: &str) -> Result<(), String> {
    let targets = moon::task_targets_for_tag(root, tag)?;
    for target in &targets {
        let path = root
            .join(".osf")
            .join("out")
            .join(format!("{}.sarif", task_id(target)));
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(format!(
                    "cannot remove the stale SARIF at {}: {e}",
                    path.display()
                ))
            }
        }
    }
    Ok(())
}

/// Ruling R16: a failed task's own output must be visible, capped so one
/// runaway task cannot flood a hook's refusal text.
const FAILED_TASK_OUTPUT_LINES: usize = 80;

/// One block for `error_findings`: `target`'s captured `stream` (`stdout`
/// or `stderr`), under a header naming the task and the stream. A stream
/// moon wrote empty or not at all says so in one line, rather than being
/// silently skipped — its absence is itself worth knowing when diagnosing
/// a failure.
fn output_stream_block(target: &str, stream: &str, tail: Option<(String, usize, usize)>) -> String {
    match tail {
        Some((text, shown, total)) if shown < total => {
            format!("--- {target} {stream} (last {shown} of {total} lines) ---\n{text}")
        }
        Some((text, _, total)) => {
            format!("--- {target} {stream} ({total} line(s)) ---\n{text}")
        }
        None => format!("--- {target} {stream}: empty or missing ---"),
    }
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

/// One task's outcome: its journal result, its rendered summary line, its
/// SARIF finding lines (kept separately so the caller only surfaces them
/// for a task that failed), the finding count the journal records, and a
/// reason when that count is not a genuine observation (ruling R12).
struct TaskLine {
    result: CheckResult,
    line: String,
    findings: Vec<String>,
    findings_count: u32,
    reason: Option<String>,
}

/// Ruling R12: a failed task with no SARIF file has unknown findings, not
/// zero — the check never ran to completion, so there is nothing to count.
/// A SARIF file that exists but will not parse is the same problem for any
/// task, whatever its status: the count cannot be trusted either way.
///
/// M2: a suppressed result is already excluded by [`sarif_outcome`], so the
/// count here is every other result; the lines returned are error-level
/// only, since those are the ones worth putting in a hook's refusal text.
///
/// M3: a reason naming the SARIF path uses the forward-slash,
/// repository-relative label, never a platform-separated `PathBuf`.
fn findings_from_sarif(
    root: &Path,
    target: &str,
    status: TaskStatus,
) -> (u32, Option<String>, Vec<String>, String) {
    match sarif_outcome(root, target) {
        SarifOutcome::Findings(findings) => {
            let count = u32::try_from(findings.len()).unwrap_or(u32::MAX);
            let word = format!("{count} finding(s)");
            let error_lines: Vec<String> = findings
                .iter()
                .filter(|f| f.level == "error")
                .map(|f| format!("{}: {}", f.rule, f.message))
                .collect();
            (count, None, error_lines, word)
        }
        SarifOutcome::Missing if status == TaskStatus::Failed => {
            let reason = format!("no findings file: {}", sarif_rel_label(target));
            (0, Some(reason), Vec::new(), "findings unknown".to_string())
        }
        SarifOutcome::Missing => (0, None, Vec::new(), "0 finding(s)".to_string()),
        SarifOutcome::Unreadable(err) => {
            let reason = format!("cannot read SARIF at {}: {err}", sarif_rel_label(target));
            (
                0,
                Some(reason),
                Vec::new(),
                "findings unreadable".to_string(),
            )
        }
    }
}

fn task_line(root: &Path, task: &moon::TaskOutcome) -> TaskLine {
    let result = match task.status {
        TaskStatus::Passed => CheckResult::Passed,
        TaskStatus::Failed => CheckResult::Failed,
        TaskStatus::Skipped => CheckResult::Skipped,
    };
    let (findings_count, reason, findings, findings_word) =
        findings_from_sarif(root, &task.target, task.status);
    let cache = if task.cached { "hit" } else { "miss" };
    let word = match task.status {
        TaskStatus::Passed => "passed",
        TaskStatus::Failed => "failed",
        TaskStatus::Skipped => "skipped",
    };
    let line = format!(
        "{}: {word} ({findings_word}, {}ms, cache {cache})",
        task.target, task.duration_ms
    );
    TaskLine {
        result,
        line,
        findings,
        findings_count,
        reason,
    }
}

/// `Some(reason)` when nothing genuinely ran at `checkpoint`, so it must not pass.
fn ran_could_not_run(checkpoint: Checkpoint, tasks: &[moon::TaskOutcome]) -> Option<String> {
    if checkpoint == Checkpoint::Hook {
        return None;
    }
    if tasks.is_empty() {
        return Some(
            "moon's run report named no task, though it did not report nothing affected"
                .to_string(),
        );
    }
    let mut reasons = Vec::new();
    let invalid: Vec<&str> = tasks
        .iter()
        .filter(|t| t.invalid)
        .map(|t| t.target.as_str())
        .collect();
    if !invalid.is_empty() {
        reasons.push(format!(
            "moon reported an invalid status for: {}",
            invalid.join(", ")
        ));
    }
    if tasks.iter().all(|t| t.status == TaskStatus::Skipped) {
        reasons.push("every selected task was skipped".to_string());
    }
    (!reasons.is_empty()).then(|| reasons.join("; "))
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
                let output =
                    moon::task_output_tail(req.root, &task.target, FAILED_TASK_OUTPUT_LINES);
                error_findings.push(output_stream_block(&task.target, "stdout", output.stdout));
                error_findings.push(output_stream_block(&task.target, "stderr", output.stderr));
            }
            CheckResult::Skipped => skipped += 1,
            CheckResult::CouldNotRun | CheckResult::NothingToCheck => {}
        }
        if let Some(reason) = &outcome.reason {
            error_findings.push(reason.clone());
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
                    reason: outcome.reason.clone(),
                }),
            ) {
                journal_error.get_or_insert(e);
            }
        }
    }
    let ran_reason = ran_could_not_run(req.checkpoint, tasks);
    let overall = if ran_reason.is_some() {
        CheckResult::CouldNotRun
    } else if failed > 0 {
        CheckResult::Failed
    } else if passed == 0 && skipped > 0 {
        CheckResult::Skipped
    } else {
        CheckResult::Passed
    };
    match &ran_reason {
        Some(reason) => {
            error_findings.push(reason.clone());
            lines.push(format!("{label}: could not run: {reason}"));
        }
        None => lines.push(format!(
            "{label}: {passed} passed, {failed} failed, {skipped} skipped, {total_findings} finding(s) total"
        )),
    }
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
    files_path: PathBuf,
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

    let files_path = match write_files_from(&run_id, &files) {
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
            cleanup_files_from(&files_path);
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
        files_path,
    })
}

/// Writes one verification event, so [`append_unset_verifications`] does
/// not repeat the `Payload::Verification` shape at each of its two sites.
fn append_unset_verification(
    journal: &mut Journal,
    label: &str,
    check: String,
    result: CheckResult,
    reason: &str,
    journal_error: &mut Option<String>,
) {
    if let Err(e) = journal.append(
        ACTOR,
        now_millis(),
        Payload::Verification(Verification {
            check,
            slot: None,
            checkpoint: label.to_string(),
            result,
            duration_ms: 0,
            cache: None,
            findings: 0,
            grade: "observed".to_string(),
            reason: Some(reason.to_string()),
        }),
    ) {
        journal_error.get_or_insert(e);
    }
}

/// I2: moon never got to report on any task — a timeout, a could-not-run
/// from `moon::run`, or a failure clearing the stale SARIF before it ran —
/// so one verification event per target the checkpoint was about to run
/// records why, instead of leaving the journal silent about work that
/// never happened. When even the target list cannot be read, records one
/// event naming `moon` itself instead.
fn append_unset_verifications(
    root: &Path,
    tag: &str,
    label: &str,
    journal: &mut Option<Journal>,
    result: CheckResult,
    reason: &str,
    journal_error: &mut Option<String>,
) {
    let Some(j) = journal.as_mut() else {
        return;
    };
    match moon::task_targets_for_tag(root, tag) {
        Ok(targets) => {
            for target in targets {
                append_unset_verification(j, label, target, result, reason, journal_error);
            }
        }
        Err(e) => {
            append_unset_verification(
                j,
                label,
                "moon".to_string(),
                CheckResult::CouldNotRun,
                &e,
                journal_error,
            );
        }
    }
}

/// Builds the [`Summary`] for a `clear_stale_sarif` failure: I2's
/// per-target could-not-run events, then the one-line summary every
/// outcome with no per-task detail shares.
#[allow(clippy::too_many_arguments)]
fn stale_sarif_could_not_run(
    req: &Request,
    journal: &mut Option<Journal>,
    commit: Option<String>,
    label: &str,
    mut error_findings: Vec<String>,
    mut journal_error: Option<String>,
    reason: &str,
) -> Summary {
    append_unset_verifications(
        req.root,
        req.checkpoint.tag(),
        label,
        journal,
        CheckResult::CouldNotRun,
        reason,
        &mut journal_error,
    );
    error_findings.push(reason.to_string());
    let line = format!("{label}: could not run: cannot clear stale findings");
    finish(
        journal,
        label,
        commit,
        CheckResult::CouldNotRun,
        line,
        error_findings,
        journal_error,
    )
}

/// Builds the [`Summary`] for `Outcome::TimedOut` (I2): a hook checkpoint
/// reports skipped, decision 0011; every other checkpoint could not run.
/// Either way, one verification event per target names the time limit.
#[allow(clippy::too_many_arguments)]
fn timed_out_summary(
    req: &Request,
    journal: &mut Option<Journal>,
    commit: Option<String>,
    label: &str,
    mut error_findings: Vec<String>,
    mut journal_error: Option<String>,
) -> Summary {
    let result = if req.checkpoint == Checkpoint::Hook {
        CheckResult::Skipped
    } else {
        CheckResult::CouldNotRun
    };
    let timeout_reason = format!(
        "hook time limit {}s",
        req.timeout.unwrap_or_default().as_secs()
    );
    append_unset_verifications(
        req.root,
        req.checkpoint.tag(),
        label,
        journal,
        CheckResult::Skipped,
        &timeout_reason,
        &mut journal_error,
    );
    error_findings.push("moon timed out".to_string());
    finish(
        journal,
        label,
        commit,
        result,
        format!("{label}: moon timed out"),
        error_findings,
        journal_error,
    )
}

/// Builds the [`Summary`] for `Outcome::CouldNotRun` (I2): one
/// verification event per target names moon's own error as the reason.
#[allow(clippy::too_many_arguments)]
fn moon_could_not_run_summary(
    req: &Request,
    journal: &mut Option<Journal>,
    commit: Option<String>,
    label: &str,
    mut error_findings: Vec<String>,
    mut journal_error: Option<String>,
    reason: &str,
) -> Summary {
    append_unset_verifications(
        req.root,
        req.checkpoint.tag(),
        label,
        journal,
        CheckResult::CouldNotRun,
        reason,
        &mut journal_error,
    );
    error_findings.push(reason.to_string());
    let line = format!("{label}: could not run: {reason}");
    finish(
        journal,
        label,
        commit,
        CheckResult::CouldNotRun,
        line,
        error_findings,
        journal_error,
    )
}

/// Appends a checkpoint-complete event and builds the one-line [`Summary`]
/// that goes with it, for every outcome that carries no per-task detail.
fn finish(
    journal: &mut Option<Journal>,
    label: &str,
    commit: Option<String>,
    result: CheckResult,
    line: String,
    error_findings: Vec<String>,
    mut journal_error: Option<String>,
) -> Summary {
    append_checkpoint_complete(journal, label, commit, result, 0, &mut journal_error);
    Summary {
        result,
        lines: vec![line],
        error_findings,
        journal_error,
    }
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
        error_findings,
        journal_error,
        files_path,
    } = match prepare(req, state_dir) {
        Ok(p) => p,
        Err(summary) => return summary,
    };

    let commit = crate::git::head_short_sha(req.root).ok();
    let label = req.checkpoint.label();

    // Moon's own `--stdin` reads a genuinely empty file list as no filter
    // at all, and runs every task as if everything were affected — the
    // opposite of what an empty change means here. Report it directly,
    // the same as `Outcome::NothingAffected`, without asking moon at all.
    if files.is_empty() {
        cleanup_files_from(&files_path);
        let line = format!("{label}: nothing to check");
        return finish(
            &mut journal,
            label,
            commit,
            CheckResult::NothingToCheck,
            line,
            error_findings,
            journal_error,
        );
    }

    if let Err(e) = clear_stale_sarif(req.root, req.checkpoint.tag()) {
        cleanup_files_from(&files_path);
        return stale_sarif_could_not_run(
            req,
            &mut journal,
            commit,
            label,
            error_findings,
            journal_error,
            &e,
        );
    }

    let target = format!(":#{}", req.checkpoint.tag());
    let outcome = moon::run(&Invocation {
        root: req.root,
        targets: std::slice::from_ref(&target),
        files: &files,
        env: &env,
        timeout: req.timeout,
    });
    cleanup_files_from(&files_path);

    match outcome {
        Outcome::NothingAffected => finish(
            &mut journal,
            label,
            commit,
            CheckResult::NothingToCheck,
            format!("{label}: nothing to check"),
            error_findings,
            journal_error,
        ),
        Outcome::Ran { tasks, .. } => handle_ran(
            req,
            journal,
            commit,
            label,
            &tasks,
            error_findings,
            journal_error,
        ),
        Outcome::TimedOut => timed_out_summary(
            req,
            &mut journal,
            commit,
            label,
            error_findings,
            journal_error,
        ),
        Outcome::CouldNotRun(reason) => moon_could_not_run_summary(
            req,
            &mut journal,
            commit,
            label,
            error_findings,
            journal_error,
            &reason,
        ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moon::TaskOutcome;

    fn task(target: &str, status: TaskStatus, invalid: bool) -> TaskOutcome {
        TaskOutcome {
            target: target.to_string(),
            status,
            duration_ms: 1,
            cached: false,
            invalid,
        }
    }

    /// Fixes-135 task 1, bullet 1: every selected task skipped is could-not-run.
    #[test]
    fn an_all_skipped_run_at_pre_push_is_could_not_run() {
        let tasks = vec![
            task("a:one", TaskStatus::Skipped, false),
            task("a:two", TaskStatus::Skipped, false),
        ];
        let reason = ran_could_not_run(Checkpoint::PrePush, &tasks).expect("a reason");
        assert!(reason.contains("skipped"), "{reason}");
    }

    /// Bullet 3: one invalid task is could-not-run even when others pass.
    #[test]
    fn an_invalid_task_is_could_not_run_even_when_others_pass() {
        let tasks = vec![
            task("a:one", TaskStatus::Passed, false),
            task("a:two", TaskStatus::Skipped, true),
        ];
        let reason = ran_could_not_run(Checkpoint::PullRequest, &tasks).expect("a reason");
        assert!(reason.contains("a:two"), "{reason}");
    }

    /// Bullet 2: a report with no task at all is could-not-run.
    #[test]
    fn an_empty_task_list_from_a_genuine_run_is_could_not_run() {
        let tasks: Vec<TaskOutcome> = Vec::new();
        assert!(ran_could_not_run(Checkpoint::Schedule, &tasks).is_some());
    }

    #[test]
    fn a_passing_run_is_not_could_not_run() {
        let tasks = vec![task("a:one", TaskStatus::Passed, false)];
        assert!(ran_could_not_run(Checkpoint::PreCommit, &tasks).is_none());
    }

    /// A genuine mix of passed and skipped tasks is a pass, not could-not-run.
    #[test]
    fn a_mixed_pass_and_skip_run_is_not_could_not_run() {
        let tasks = vec![
            task("a:one", TaskStatus::Passed, false),
            task("a:two", TaskStatus::Skipped, false),
        ];
        assert!(ran_could_not_run(Checkpoint::PreCommit, &tasks).is_none());
    }

    /// The hook checkpoint keeps its current behaviour under this rule.
    #[test]
    fn the_hook_checkpoint_is_never_forced_could_not_run_by_this_rule() {
        let tasks = vec![task("a:one", TaskStatus::Skipped, false)];
        assert!(ran_could_not_run(Checkpoint::Hook, &tasks).is_none());
        let empty: Vec<TaskOutcome> = Vec::new();
        assert!(ran_could_not_run(Checkpoint::Hook, &empty).is_none());
    }
}
