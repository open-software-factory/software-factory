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

/// The file whose presence means a repository has adopted osf.
const MOON_YML_LABEL: &str = ".osf/moon.yml";

/// Whether a folder has adopted osf: `.osf/moon.yml` exists at its root.
pub enum Adoption {
    /// `.osf/moon.yml` exists at this root.
    Adopted(PathBuf),
    /// `osf.toml` exists at this root, but the named file does not.
    AdoptedButBroken(PathBuf, String),
    /// No git repository, or neither file present: the path and why.
    NotAdopted(PathBuf, String),
    /// The check itself could not run: git failed, or a file could not be read.
    CouldNotRun(String),
}

/// `dir` canonicalised, with Windows' verbatim prefix stripped.
fn display_path(dir: &Path) -> PathBuf {
    let Ok(canon) = std::fs::canonicalize(dir) else {
        return dir.to_path_buf();
    };
    match canon.to_string_lossy().strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => canon,
    }
}

/// Whether `path` exists as a file. `Ok(false)` only for a plain "not found".
fn file_presence(path: &Path) -> Result<bool, String> {
    match std::fs::metadata(path) {
        Ok(meta) => Ok(meta.is_file()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("cannot check {}: {e}", path.display())),
    }
}

/// The one adoption check `verify` and `hook post-tool` both call first.
#[must_use]
pub fn detect_adoption(dir: &Path) -> Adoption {
    let root = match crate::git::repo_root_if_any(dir) {
        Ok(Some(r)) => r,
        Ok(None) => {
            return Adoption::NotAdopted(display_path(dir), "no git repository here".to_string())
        }
        Err(e) => return Adoption::CouldNotRun(format!("cannot resolve the repository root: {e}")),
    };
    match file_presence(&root.join(".osf").join("moon.yml")) {
        Ok(true) => return Adoption::Adopted(root),
        Ok(false) => {}
        Err(e) => return Adoption::CouldNotRun(e),
    }
    match file_presence(&root.join("osf.toml")) {
        Ok(true) => return Adoption::AdoptedButBroken(root, MOON_YML_LABEL.to_string()),
        Ok(false) => {}
        Err(e) => return Adoption::CouldNotRun(e),
    }
    Adoption::NotAdopted(
        root,
        "neither .osf/moon.yml nor osf.toml exists here".to_string(),
    )
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
/// as a notice rather than swallowed; the check itself still runs.
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

/// `files`' `OSF_FILES_FROM` content: one repository-relative path per
/// line. Shared by [`write_files_from`], which writes it, and
/// [`files_hash`], which fingerprints it for `OSF_FILES_HASH`.
fn files_from_content(files: &[String]) -> String {
    let mut content = String::new();
    for file in files {
        content.push_str(file);
        content.push('\n');
    }
    content
}

/// Writes `files`, one per line, to a fresh file under the OS temp
/// directory, for `OSF_FILES_FROM`. Moon itself cannot pass a
/// changed-file list to a task, so this is how each task's `osf check`
/// learns it. This lives outside the journal's own state
/// directory, so an unwritable state dir never blocks it, and the caller
/// deletes it once moon has run, on every path. `run` makes the path
/// unique to this one invocation: two overlapping runs of the same
/// checkpoint — an agent writing two files in quick succession at the hook
/// checkpoint, say — each get their own list, rather than one overwriting
/// or deleting the other's file out from under it. A fixed, checkpoint-shared
/// path was tried and reverted for exactly that race.
fn write_files_from(run: &str, files: &[String]) -> Result<PathBuf, String> {
    let base = std::env::temp_dir(); // osf: temp-dir allowed, unique per checkpoint run
    let path = base.join(format!("osf-checkpoint-files-{run}.txt"));
    std::fs::write(&path, files_from_content(files)).map_err(|e| {
        format!(
            "cannot write the checkpoint file list {}: {e}",
            path.display()
        )
    })?;
    Ok(path)
}

/// The lower-case SHA-256 hex digest of `files`' `OSF_FILES_FROM` content,
/// for `OSF_FILES_HASH`. The per-run `OSF_FILES_FROM` file itself is not
/// something moon can see or safely share across overlapping runs, so each
/// osf task instead declares `$OSF_FILES_HASH` as an input: the hash
/// carries only a fingerprint of the list into moon's own cache, and
/// changes no behaviour. The file list itself still reaches the check as
/// `OSF_FILES_FROM` always has.
fn files_hash(files: &[String]) -> String {
    crate::journal::sha256_hex(files_from_content(files).as_bytes())
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

/// One SARIF result worth counting: never a suppressed one, since a
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
/// file: a reason built from this reads the same on every operating
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

/// A SARIF file left over from an earlier run must never be
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

/// A failed task's own output must be visible, capped so one
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
/// reason when that count is not a genuine observation.
struct TaskLine {
    result: CheckResult,
    line: String,
    findings: Vec<String>,
    findings_count: u32,
    reason: Option<String>,
}

/// A failed task with no SARIF file has unknown findings, not
/// zero — the check never ran to completion, so there is nothing to count.
/// A SARIF file that exists but will not parse is the same problem for any
/// task, whatever its status: the count cannot be trusted either way.
///
/// A suppressed result is already excluded by [`sarif_outcome`], so the
/// count here is every other result; the lines returned are error-level
/// only, since those are the ones worth putting in a hook's refusal text.
///
/// A reason naming the SARIF path uses the forward-slash,
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
        SarifOutcome::Missing if status == TaskStatus::Failed || status == TaskStatus::NotRun => {
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
        TaskStatus::Skipped | TaskStatus::NotRun => CheckResult::Skipped,
    };
    let (findings_count, sarif_reason, findings, findings_word) =
        findings_from_sarif(root, &task.target, task.status);
    // Moon's own reason — why it failed a task, or which sibling's
    // failure stopped it before this one ran — always wins over a
    // SARIF-file-shaped reason: it names the real cause, not just the
    // absence of a file that was never going to be written.
    let reason = task.reason.clone().or(sarif_reason);
    let cache = if task.cached { "hit" } else { "miss" };
    let word = match task.status {
        TaskStatus::Passed => "passed",
        TaskStatus::Failed => "failed",
        TaskStatus::Skipped => "skipped",
        TaskStatus::NotRun => "not run",
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
/// writes the `OSF_FILES_FROM` list, and opens the journal.
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
        "OSF_FILES_FROM".to_string(),
        files_path.to_string_lossy().into_owned(),
    ));
    env.push(("OSF_FILES_HASH".to_string(), files_hash(&files)));

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

/// Moon never got to report on any task — a timeout, a could-not-run
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

/// Builds the [`Summary`] for a `clear_stale_sarif` failure:
/// `append_unset_verifications`'s per-target could-not-run events, then the one-line summary every
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

/// Builds the [`Summary`] for `Outcome::TimedOut`: a hook checkpoint
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

/// Builds the [`Summary`] for `Outcome::CouldNotRun`: one
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
/// (decision 0014).
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
    use crate::test_support::TempDir;

    fn task(target: &str, status: TaskStatus, invalid: bool) -> TaskOutcome {
        TaskOutcome {
            target: target.to_string(),
            status,
            duration_ms: 1,
            cached: false,
            invalid,
            reason: None,
        }
    }

    /// Every selected task skipped is could-not-run.
    #[test]
    fn an_all_skipped_run_at_pre_push_is_could_not_run() {
        let tasks = vec![
            task("a:one", TaskStatus::Skipped, false),
            task("a:two", TaskStatus::Skipped, false),
        ];
        let reason = ran_could_not_run(Checkpoint::PrePush, &tasks).expect("a reason");
        assert!(reason.contains("skipped"), "{reason}");
    }

    /// One invalid task is could-not-run even when others pass.
    #[test]
    fn an_invalid_task_is_could_not_run_even_when_others_pass() {
        let tasks = vec![
            task("a:one", TaskStatus::Passed, false),
            task("a:two", TaskStatus::Skipped, true),
        ];
        let reason = ran_could_not_run(Checkpoint::PullRequest, &tasks).expect("a reason");
        assert!(reason.contains("a:two"), "{reason}");
    }

    /// A report with no task at all is could-not-run.
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

    /// Runs `git init --quiet` in `dir`.
    fn init_repo(dir: &Path) {
        let mut command = std::process::Command::new("git");
        command.arg("init").arg("--quiet").arg(dir);
        crate::git::scrub_git_env(&mut command);
        let status = command.status().expect("git init runs");
        assert!(status.success(), "git init failed for {}", dir.display());
    }

    /// Compares two paths after canonicalising both.
    fn assert_same_dir(a: &Path, b: &Path) {
        let ca = std::fs::canonicalize(a).expect("a canonicalises");
        let cb = std::fs::canonicalize(b).expect("b canonicalises");
        assert_eq!(ca, cb, "{a:?} vs {b:?}");
    }

    /// No git repository at all is not adopted.
    #[test]
    fn no_git_repository_at_all_is_not_adopted() {
        let dir = TempDir::new("osf-adoption-test-no-git");
        assert!(
            crate::git::repo_root(&dir).is_err(),
            "git rev-parse must fail with no repository present"
        );
        match detect_adoption(&dir) {
            Adoption::NotAdopted(_, reason) => assert!(reason.contains("git"), "{reason}"),
            _ => panic!("expected NotAdopted"),
        }
    }

    /// A repository with neither file is not adopted.
    #[test]
    fn a_repository_with_neither_file_is_not_adopted() {
        let dir = TempDir::new("osf-adoption-test-neither");
        init_repo(&dir);
        match detect_adoption(&dir) {
            Adoption::NotAdopted(path, _) => assert_same_dir(&path, &dir),
            _ => panic!("expected NotAdopted"),
        }
    }

    /// `osf.toml` without `.osf/moon.yml` is adopted-but-broken.
    #[test]
    fn osf_toml_without_moon_yml_is_adopted_but_broken() {
        let dir = TempDir::new("osf-adoption-test-broken");
        init_repo(&dir);
        std::fs::write(dir.join("osf.toml"), "").expect("osf.toml writes");
        match detect_adoption(&dir) {
            Adoption::AdoptedButBroken(path, missing) => {
                assert_same_dir(&path, &dir);
                assert!(missing.contains("moon.yml"), "{missing}");
            }
            _ => panic!("expected AdoptedButBroken"),
        }
    }

    /// `.osf/moon.yml` present is adopted.
    #[test]
    fn moon_yml_present_is_adopted() {
        let dir = TempDir::new("osf-adoption-test-adopted");
        init_repo(&dir);
        std::fs::create_dir_all(dir.join(".osf")).expect(".osf dir creates");
        std::fs::write(dir.join(".osf").join("moon.yml"), "").expect("moon.yml writes");
        match detect_adoption(&dir) {
            Adoption::Adopted(path) => assert_same_dir(&path, &dir),
            _ => panic!("expected Adopted"),
        }
    }

    /// Two runs, even of the same checkpoint, each get their own
    /// `OSF_FILES_FROM` file rather than sharing one fixed path — the race
    /// a shared path had (one run's write or cleanup clobbering another's
    /// file) is exactly what a unique-per-run name rules out. Cleanup
    /// still removes the file on every path.
    #[test]
    fn two_runs_of_the_same_checkpoint_never_share_a_files_from_path() {
        let run_a = "pre-commit-1000-111";
        let run_b = "pre-commit-1000-222";
        let path_a =
            write_files_from(run_a, &["a.md".to_string()]).expect("first run's write succeeds");
        let path_b =
            write_files_from(run_b, &["b.md".to_string()]).expect("second run's write succeeds");
        assert_ne!(
            path_a, path_b,
            "two overlapping runs must never share one list file"
        );
        assert_eq!(std::fs::read_to_string(&path_a).expect("read"), "a.md\n");
        assert_eq!(std::fs::read_to_string(&path_b).expect("read"), "b.md\n");

        cleanup_files_from(&path_a);
        assert!(!path_a.exists(), "cleanup must remove run a's own file");
        assert!(
            path_b.exists(),
            "run a's cleanup must never delete run b's still-live file"
        );
        cleanup_files_from(&path_b);
    }

    /// `OSF_FILES_HASH` is a stable fingerprint of the same content
    /// `OSF_FILES_FROM` carries, so the same file list always hashes the
    /// same, and a different one never collides by accident in this
    /// test's small inputs.
    #[test]
    fn the_files_hash_is_stable_for_the_same_list_and_differs_for_a_different_one() {
        let a = vec!["a.md".to_string()];
        let b = vec!["a.md".to_string(), "b.md".to_string()];
        assert_eq!(files_hash(&a), files_hash(&a), "same list, same hash");
        assert_ne!(
            files_hash(&a),
            files_hash(&b),
            "different list, different hash"
        );
        assert_eq!(
            files_hash(&a).len(),
            64,
            "a SHA-256 hex digest is 64 hex characters"
        );
    }

    /// A task moon's own action failed (exited 0 but left a declared
    /// output missing, say) is reported failed with moon's own reason, not
    /// a generic "no findings file" reason and not passed.
    #[test]
    fn a_task_moon_failed_is_reported_failed_with_moons_own_reason() {
        let root = TempDir::new("osf-checkpoint-task-line-failed");
        let outcome = TaskOutcome {
            target: "osf:review".to_string(),
            status: TaskStatus::Failed,
            duration_ms: 76,
            cached: false,
            invalid: false,
            reason: Some(
                "Task osf:review defines outputs but after being ran, either none or not all \
                 of them exist."
                    .to_string(),
            ),
        };
        let line = task_line(&root, &outcome);
        assert_eq!(line.result, CheckResult::Failed);
        assert!(line.line.contains("failed"), "{}", line.line);
        let reason = line.reason.expect("a reason");
        assert!(reason.contains("defines outputs"), "{reason}");
    }

    /// A sibling moon never started because an earlier task in the same
    /// run failed is reported not run, naming that task — never a failure
    /// with no reason.
    #[test]
    fn a_sibling_moon_never_started_is_reported_not_run_with_a_named_reason() {
        let root = TempDir::new("osf-checkpoint-task-line-not-run");
        let outcome = TaskOutcome {
            target: "osf:scan-commits".to_string(),
            status: TaskStatus::NotRun,
            duration_ms: 0,
            cached: false,
            invalid: false,
            reason: Some("moon stopped the run after osf:review failed".to_string()),
        };
        let line = task_line(&root, &outcome);
        assert_eq!(line.result, CheckResult::Skipped);
        assert!(line.line.contains("not run"), "{}", line.line);
        assert!(!line.line.contains("failed"), "{}", line.line);
        let reason = line.reason.expect("a reason");
        assert!(reason.contains("osf:review"), "{reason}");
    }
}
