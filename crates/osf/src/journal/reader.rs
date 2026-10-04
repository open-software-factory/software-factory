//! Reads a run's hash-chained journal back, checking every link before it
//! returns any event: a broken chain is reported as broken, never as a
//! partial list.

use std::fmt;
use std::path::{Path, PathBuf};

use super::event::{Event, Payload, SCHEMA_VERSION};
use super::hash::{event_hash, genesis_hash, HashInput};
use super::{validate_event, validate_run_id};

/// A run's events read back in order, with the hash at the head of its chain.
#[derive(Clone, Debug, PartialEq)]
pub struct RunJournal {
    /// Every event in the run, in the order it was written.
    pub events: Vec<Event>,
    /// The last event's hash, or the genesis hash for an empty journal.
    pub head_hash: String,
    /// True when the last event is a run-complete event.
    pub complete: bool,
}

/// Every way reading a run's journal can fail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// No journal file exists for the run.
    NotFound {
        /// The path that was read.
        path: PathBuf,
    },
    /// The journal file exists but could not be read.
    Io {
        /// The path that was read.
        path: PathBuf,
        /// The operating system's reason.
        reason: String,
    },
    /// The run id is not a safe single path component.
    InvalidRunId {
        /// Why the run id was refused.
        reason: String,
    },
    /// A line is missing, not JSON, or not a journal event.
    Malformed {
        /// The 1-based line number.
        line: usize,
        /// Why the line is malformed.
        reason: String,
    },
    /// A line carries a schema version this build does not recognise.
    UnsupportedVersion {
        /// The 1-based line number.
        line: usize,
        /// The schema version the line names.
        version: u64,
    },
    /// A line does not satisfy the event schema.
    Invalid {
        /// The 1-based line number.
        line: usize,
        /// One string per schema problem.
        reasons: Vec<String>,
    },
    /// A line does not continue the run's hash chain.
    ChainBroken {
        /// The 1-based line number.
        line: usize,
        /// Why the link is broken.
        reason: String,
    },
    /// A line follows a run-complete event.
    EventAfterRunComplete {
        /// The 1-based line number.
        line: usize,
    },
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::NotFound { path } => {
                write!(f, "the journal file {} was not found", path.display())
            }
            ReadError::Io { path, reason } => {
                write!(
                    f,
                    "the journal file {} could not be read: {reason}",
                    path.display()
                )
            }
            ReadError::InvalidRunId { reason } => write!(f, "the run id is invalid: {reason}"),
            ReadError::Malformed { line, reason } => {
                write!(f, "line {line} is malformed: {reason}")
            }
            ReadError::UnsupportedVersion { line, version } => {
                write!(
                    f,
                    "line {line} carries unsupported schema version {version}"
                )
            }
            ReadError::Invalid { line, reasons } => {
                write!(f, "line {line} is invalid: {}", reasons.join("; "))
            }
            ReadError::ChainBroken { line, reason } => {
                write!(f, "line {line} breaks the hash chain: {reason}")
            }
            ReadError::EventAfterRunComplete { line } => {
                write!(
                    f,
                    "line {line} follows a run-complete event: the run was already complete"
                )
            }
        }
    }
}

impl std::error::Error for ReadError {}

/// Reads the journal for `run` under `state_dir`, checking every line and
/// every link in order. Any failure returns that line's error and no events
/// at all, so a caller never sees a partial chain.
///
/// `Ok` means every line present is valid and linked. The `complete` field
/// says whether the run finished. `Ok` does not mean no lines were removed
/// from the end. It also does not detect a rewrite in which every hash was
/// recomputed, because the hash is unkeyed. Copy the head hash somewhere the
/// writer cannot reach, such as a sink or the pull request, to detect such a
/// rewrite.
///
/// # Errors
/// Returns [`ReadError::InvalidRunId`] when `run` is not a safe single path
/// component, [`ReadError::NotFound`] when the file is missing,
/// [`ReadError::Io`] when it cannot be read, and a line error when a line is
/// malformed, schema-invalid, at an unsupported version, off the chain or
/// after a run-complete event.
pub fn read_run(state_dir: &Path, run: &str) -> Result<RunJournal, ReadError> {
    validate_run_id(run).map_err(|reason| ReadError::InvalidRunId { reason })?;
    let path = state_dir.join("runs").join(format!("{run}.jsonl"));
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ReadError::NotFound { path });
        }
        Err(error) => {
            return Err(ReadError::Io {
                path,
                reason: error.to_string(),
            });
        }
    };
    read_text(&text, run)
}

/// Splits `text` into newline-terminated lines and checks them in order.
fn read_text(text: &str, run: &str) -> Result<RunJournal, ReadError> {
    if text.is_empty() {
        return Ok(RunJournal {
            events: Vec::new(),
            head_hash: genesis_hash(),
            complete: false,
        });
    }
    let missing_newline = !text.ends_with('\n');
    let lines: Vec<&str> = text.split_terminator('\n').collect();
    let last = lines.len().saturating_sub(1);
    let mut events = Vec::with_capacity(lines.len());
    let mut head = genesis_hash();
    let mut complete = false;
    for (index, line) in lines.iter().enumerate() {
        let number = index + 1;
        if complete {
            return Err(ReadError::EventAfterRunComplete { line: number });
        }
        if missing_newline && index == last {
            return Err(ReadError::Malformed {
                line: number,
                reason: "line has no terminating newline".to_string(),
            });
        }
        let event = check_line(line, number, run, &head)?;
        head.clone_from(&event.hash);
        complete = matches!(&event.payload, Payload::RunComplete(_));
        events.push(event);
    }
    Ok(RunJournal {
        events,
        head_hash: head,
        complete,
    })
}

/// Checks one line against the schema and the chain, returning its event.
fn check_line(
    line: &str,
    number: usize,
    run: &str,
    expected_prev: &str,
) -> Result<Event, ReadError> {
    if line.trim().is_empty() {
        return Err(ReadError::Malformed {
            line: number,
            reason: "blank line".to_string(),
        });
    }
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|error| ReadError::Malformed {
            line: number,
            reason: format!("line is not JSON: {error}"),
        })?;
    if let Some(version) = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
    {
        if version != u64::from(SCHEMA_VERSION) {
            return Err(ReadError::UnsupportedVersion {
                line: number,
                version,
            });
        }
    }
    if let Err(reasons) = validate_event(&value) {
        return Err(ReadError::Invalid {
            line: number,
            reasons,
        });
    }
    let event: Event = serde_json::from_value(value).map_err(|error| ReadError::Malformed {
        line: number,
        reason: format!("line is not a journal event: {error}"),
    })?;
    if event.run != run {
        return Err(ReadError::ChainBroken {
            line: number,
            reason: format!(
                "event run '{}' does not match the journal's run '{run}'",
                event.run
            ),
        });
    }
    if event.prev_hash != expected_prev {
        return Err(ReadError::ChainBroken {
            line: number,
            reason: format!(
                "prev_hash {} does not match the expected head {expected_prev}",
                event.prev_hash
            ),
        });
    }
    let recomputed = event_hash(&HashInput {
        prev_hash: &event.prev_hash,
        payload: &event.payload,
        work_item: event.work_item.as_deref(),
        change: event.change.as_ref(),
        actor: &event.actor,
        cost: event.cost.as_ref(),
    });
    if recomputed != event.hash {
        return Err(ReadError::ChainBroken {
            line: number,
            reason: format!(
                "event hash {} does not match the recomputed hash {recomputed}",
                event.hash
            ),
        });
    }
    if let Payload::RunComplete(complete) = &event.payload {
        if complete.head_hash != event.prev_hash {
            return Err(ReadError::ChainBroken {
                line: number,
                reason: format!(
                    "run-complete head_hash {} does not equal its prev_hash {}",
                    complete.head_hash, event.prev_hash
                ),
            });
        }
    }
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{
        Actor, Attention, CheckResult, CheckpointComplete, EventDraft, EvidenceGrade, Finding,
        Journal, Review, RunComplete, RunOutcome, RunStarted, Severity, StateChange, Verification,
        WorkItemState,
    };
    use crate::test_support::TempDir;

    fn actor() -> Actor {
        Actor::system("osf")
    }

    fn run_started() -> Payload {
        Payload::RunStarted(RunStarted {
            title: Some("Ship the fix".into()),
            repository: Some("github:open-software-factory/example".into()),
            retry_of: None,
            transcript: Some("runs/run-1/transcript.jsonl".into()),
        })
    }

    fn verification() -> Payload {
        Payload::Verification(Verification {
            check: "osf:lint".into(),
            check_type: Some("lint".into()),
            slot: Some("lint".into()),
            checkpoint: "pre-commit".into(),
            result: CheckResult::Passed,
            duration_ms: 12,
            cache: Some("miss".into()),
            findings: 0,
            summary: Some("clean".into()),
            grade: EvidenceGrade::Observed,
            reason: None,
        })
    }

    fn review() -> Payload {
        Payload::Review(Review {
            round: 1,
            scope: "the change".into(),
            findings: 1,
            summary: "looked at the diff".into(),
            grade: EvidenceGrade::Derived,
        })
    }

    fn finding(grade: EvidenceGrade, verified_by: Option<&str>) -> Payload {
        Payload::Finding(Finding {
            rule: "example-rule".into(),
            severity: Severity::Warning,
            action: "fix".into(),
            path: Some("src/lib.rs".into()),
            line: Some(3),
            column: None,
            message: "something to fix".into(),
            grade,
            verified_by: verified_by.map(str::to_string),
        })
    }

    fn state_change() -> Payload {
        Payload::StateChange(StateChange {
            from: Some(WorkItemState::Ready),
            to: WorkItemState::InProgress,
            cause: None,
        })
    }

    fn attention() -> Payload {
        Payload::Attention(Attention {
            cause: "human".into(),
            summary: "needs a person".into(),
            grade: EvidenceGrade::Unverified,
        })
    }

    fn checkpoint_complete() -> Payload {
        Payload::CheckpointComplete(CheckpointComplete {
            checkpoint: "pre-push".into(),
            commit: Some("abc123".into()),
            result: CheckResult::Passed,
            checks: 3,
            slots: std::collections::BTreeMap::from([("lint".to_string(), CheckResult::Passed)]),
        })
    }

    fn run_complete(head_hash: String) -> Payload {
        Payload::RunComplete(RunComplete {
            outcome: RunOutcome::Completed,
            head_hash,
            summary: "all checks passed".into(),
            grade: EvidenceGrade::Derived,
            transcript_hash: None,
        })
    }

    /// Writes one of each event type through the writer, run-complete last.
    fn write_all_eight(dir: &Path, run: &str) -> Vec<Event> {
        let mut journal = Journal::open(dir, run).expect("open");
        let mut events = vec![journal
            .append(&actor(), 1, run_started())
            .expect("run-started")];
        events.push(
            journal
                .append(&actor(), 2, verification())
                .expect("verification"),
        );
        events.push(journal.append(&actor(), 3, review()).expect("review"));
        events.push(
            journal
                .append(
                    &actor(),
                    4,
                    finding(EvidenceGrade::Observed, Some("osf:lint")),
                )
                .expect("finding"),
        );
        events.push(
            journal
                .append(&actor(), 5, state_change())
                .expect("state-change"),
        );
        events.push(journal.append(&actor(), 6, attention()).expect("attention"));
        events.push(
            journal
                .append(&actor(), 7, checkpoint_complete())
                .expect("checkpoint-complete"),
        );
        let head = events.last().expect("events").hash.clone();
        events.push(
            journal
                .append(&actor(), 8, run_complete(head))
                .expect("run-complete"),
        );
        events
    }

    fn journal_lines(dir: &Path, run: &str) -> Vec<String> {
        let text = std::fs::read_to_string(dir.join("runs").join(format!("{run}.jsonl")))
            .expect("read journal");
        text.lines().map(str::to_string).collect()
    }

    /// Writes `lines`, each terminated by a newline; empty input writes an
    /// empty file.
    fn write_lines(dir: &Path, run: &str, lines: &[String]) {
        let runs = dir.join("runs");
        std::fs::create_dir_all(&runs).expect("runs dir");
        let text = if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        };
        std::fs::write(runs.join(format!("{run}.jsonl")), text).expect("write journal");
    }

    /// Parses an event line, lets `edit` change it, and serialises it back.
    fn edit_event(line: &str, edit: impl FnOnce(&mut Event)) -> String {
        let mut event: Event = serde_json::from_str(line).expect("line is an event");
        edit(&mut event);
        serde_json::to_string(&event).expect("event serialises")
    }

    /// Replaces `key` in a JSON object line, leaving every other field alone.
    fn set_field(line: &str, key: &str, value: serde_json::Value) -> String {
        let mut parsed: serde_json::Value = serde_json::from_str(line).expect("line is JSON");
        parsed
            .as_object_mut()
            .expect("event is an object")
            .insert(key.to_string(), value);
        serde_json::to_string(&parsed).expect("line serialises")
    }

    /// Recomputes the event's hash from its own fields after `edit` changed it.
    fn rehash(event: &mut Event) {
        event.hash = event_hash(&HashInput {
            prev_hash: &event.prev_hash,
            payload: &event.payload,
            work_item: event.work_item.as_deref(),
            change: event.change.as_ref(),
            actor: &event.actor,
            cost: event.cost.as_ref(),
        });
    }

    #[test]
    fn all_eight_event_types_round_trip_through_the_writer_and_reader() {
        let dir = TempDir::new("osf-reader-eight");
        let written = write_all_eight(&dir, "run-8");
        let journal = read_run(&dir, "run-8").expect("read");
        assert_eq!(journal.events, written);
        let last = written.last().expect("at least one event");
        assert_eq!(journal.head_hash, last.hash);
    }

    #[test]
    fn two_journals_with_different_run_ids_and_times_share_a_head_hash() {
        let a_dir = TempDir::new("osf-reader-same-a");
        let b_dir = TempDir::new("osf-reader-same-b");
        {
            let mut a = Journal::open(&a_dir, "run-a").expect("open");
            a.append(&actor(), 1, run_started()).expect("run-started");
            a.append(&actor(), 2, verification()).expect("verification");
            let head = a.append(&actor(), 3, review()).expect("review").hash;
            a.append(&actor(), 4, run_complete(head))
                .expect("run-complete");
        }
        {
            let mut b = Journal::open(&b_dir, "run-b").expect("open");
            b.append(&actor(), 900, run_started()).expect("run-started");
            b.append(&actor(), 901, verification())
                .expect("verification");
            let head = b.append(&actor(), 902, review()).expect("review").hash;
            b.append(&actor(), 903, run_complete(head))
                .expect("run-complete");
        }
        let a = read_run(&a_dir, "run-a").expect("read a");
        let b = read_run(&b_dir, "run-b").expect("read b");
        assert_eq!(a.head_hash, b.head_hash);
        assert_eq!(a.events.len(), b.events.len());
    }

    #[test]
    fn a_tampered_payload_breaks_the_chain_at_its_line() {
        let dir = TempDir::new("osf-reader-tamper");
        write_all_eight(&dir, "run-tamper");
        let mut lines = journal_lines(&dir, "run-tamper");
        let tampered = edit_event(lines.get(2).expect("the review line"), |event| {
            if let Payload::Review(review) = &mut event.payload {
                review.summary = "tampered summary".to_string();
            }
        });
        *lines.get_mut(2).expect("the review line") = tampered;
        write_lines(&dir, "run-tamper", &lines);
        let error = read_run(&dir, "run-tamper").expect_err("tampered chain");
        assert!(
            matches!(error, ReadError::ChainBroken { line: 3, .. }),
            "{error:?}"
        );
    }

    #[test]
    fn a_deleted_middle_line_breaks_the_chain() {
        let dir = TempDir::new("osf-reader-deleted");
        write_all_eight(&dir, "run-deleted");
        let mut lines = journal_lines(&dir, "run-deleted");
        lines.remove(1);
        write_lines(&dir, "run-deleted", &lines);
        let error = read_run(&dir, "run-deleted").expect_err("deleted line");
        assert!(
            matches!(error, ReadError::ChainBroken { line: 2, .. }),
            "{error:?}"
        );
    }

    #[test]
    fn two_swapped_lines_break_the_chain() {
        let dir = TempDir::new("osf-reader-swapped");
        write_all_eight(&dir, "run-swapped");
        let mut lines = journal_lines(&dir, "run-swapped");
        lines.swap(1, 2);
        write_lines(&dir, "run-swapped", &lines);
        let error = read_run(&dir, "run-swapped").expect_err("swapped lines");
        assert!(
            matches!(error, ReadError::ChainBroken { line: 2, .. }),
            "{error:?}"
        );
    }

    #[test]
    fn an_edited_timestamp_is_outside_the_chain() {
        let dir = TempDir::new("osf-reader-timestamp");
        write_all_eight(&dir, "run-time");
        let mut lines = journal_lines(&dir, "run-time");
        let edited = edit_event(lines.get(1).expect("the verification line"), |event| {
            event.timestamp_ms = 999_999;
        });
        *lines.get_mut(1).expect("the verification line") = edited;
        write_lines(&dir, "run-time", &lines);
        let journal = read_run(&dir, "run-time").expect("an edited timestamp is not a break");
        assert_eq!(
            journal.events.get(1).expect("verification").timestamp_ms,
            999_999
        );
    }

    #[test]
    fn an_unsupported_schema_version_names_the_version_and_line() {
        let dir = TempDir::new("osf-reader-version");
        write_all_eight(&dir, "run-version");
        let mut lines = journal_lines(&dir, "run-version");
        let changed = set_field(
            lines.first().expect("line"),
            "schema_version",
            serde_json::json!(2),
        );
        *lines.first_mut().expect("line") = changed;
        write_lines(&dir, "run-version", &lines);
        let error = read_run(&dir, "run-version").expect_err("version 2");
        match error {
            ReadError::UnsupportedVersion { line, version } => {
                assert_eq!(line, 1);
                assert_eq!(version, 2);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn a_line_that_is_not_json_is_malformed() {
        let dir = TempDir::new("osf-reader-not-json");
        write_lines(&dir, "run-bad-json", &["this is not JSON".to_string()]);
        let error = read_run(&dir, "run-bad-json").expect_err("not JSON");
        assert!(
            matches!(error, ReadError::Malformed { line: 1, .. }),
            "{error:?}"
        );
    }

    #[test]
    fn a_line_error_names_the_line_in_its_message() {
        let dir = TempDir::new("osf-reader-display");
        write_lines(&dir, "run-display", &["this is not JSON".to_string()]);
        let error = read_run(&dir, "run-display").expect_err("not JSON");
        let message = error.to_string();
        assert!(message.contains("line 1"), "{message}");
    }

    #[test]
    fn a_blank_line_is_malformed() {
        let dir = TempDir::new("osf-reader-blank");
        write_lines(&dir, "run-blank", &[String::new()]);
        let error = read_run(&dir, "run-blank").expect_err("blank line");
        assert!(
            matches!(error, ReadError::Malformed { line: 1, .. }),
            "{error:?}"
        );
    }

    #[test]
    fn a_schema_invalid_line_is_invalid_even_with_a_correct_hash() {
        let dir = TempDir::new("osf-reader-invalid");
        {
            let mut journal = Journal::open(&dir, "run-invalid").expect("open");
            journal
                .append(
                    &actor(),
                    1,
                    finding(EvidenceGrade::Observed, Some("osf:lint")),
                )
                .expect("finding");
        }
        let mut lines = journal_lines(&dir, "run-invalid");
        let edited = edit_event(lines.first().expect("line"), |event| {
            if let Payload::Finding(finding) = &mut event.payload {
                finding.grade = EvidenceGrade::Reported;
            }
            rehash(event);
        });
        *lines.first_mut().expect("line") = edited;
        write_lines(&dir, "run-invalid", &lines);
        let error = read_run(&dir, "run-invalid").expect_err("schema invalid");
        assert!(
            matches!(error, ReadError::Invalid { line: 1, .. }),
            "{error:?}"
        );
    }

    #[test]
    fn an_event_from_another_run_is_refused() {
        let dir = TempDir::new("osf-reader-run-mismatch");
        write_all_eight(&dir, "run-a");
        let runs = dir.join("runs");
        std::fs::copy(runs.join("run-a.jsonl"), runs.join("run-b.jsonl")).expect("copy");
        let error = read_run(&dir, "run-b").expect_err("run mismatch");
        assert!(
            matches!(error, ReadError::ChainBroken { line: 1, .. }),
            "{error:?}"
        );
    }

    #[test]
    fn a_run_complete_head_hash_off_its_prev_hash_breaks_the_chain() {
        let dir = TempDir::new("osf-reader-run-complete");
        {
            let mut journal = Journal::open(&dir, "run-complete").expect("open");
            let head = journal
                .append(&actor(), 1, verification())
                .expect("verification")
                .hash;
            journal
                .append(&actor(), 2, run_complete(head))
                .expect("run-complete");
        }
        let mut lines = journal_lines(&dir, "run-complete");
        let edited = edit_event(lines.get(1).expect("the run-complete line"), |event| {
            if let Payload::RunComplete(complete) = &mut event.payload {
                complete.head_hash = "a".repeat(64);
            }
            rehash(event);
        });
        *lines.get_mut(1).expect("the run-complete line") = edited;
        write_lines(&dir, "run-complete", &lines);
        let error = read_run(&dir, "run-complete").expect_err("head hash mismatch");
        assert!(
            matches!(error, ReadError::ChainBroken { line: 2, .. }),
            "{error:?}"
        );
    }

    #[test]
    fn a_missing_journal_is_not_found() {
        let dir = TempDir::new("osf-reader-missing");
        let error = read_run(&dir, "run-missing").expect_err("missing file");
        match error {
            ReadError::NotFound { path } => {
                assert!(
                    path.ends_with("runs/run-missing.jsonl"),
                    "{}",
                    path.display()
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn an_empty_journal_reads_as_zero_events_from_genesis() {
        let dir = TempDir::new("osf-reader-empty");
        write_lines(&dir, "run-empty", &[]);
        let journal = read_run(&dir, "run-empty").expect("read empty");
        assert!(journal.events.is_empty());
        assert_eq!(journal.head_hash, "0".repeat(64));
    }

    #[test]
    fn a_traversal_run_id_is_refused() {
        let dir = TempDir::new("osf-reader-traversal");
        let error = read_run(&dir, "../escape").expect_err("traversal");
        assert!(matches!(error, ReadError::InvalidRunId { .. }), "{error:?}");
    }

    #[test]
    fn a_last_line_without_a_newline_is_malformed() {
        let dir = TempDir::new("osf-reader-no-newline");
        write_all_eight(&dir, "run-no-newline");
        let path = dir.join("runs/run-no-newline.jsonl");
        let text = std::fs::read_to_string(&path).expect("read");
        std::fs::write(&path, text.trim_end_matches('\n')).expect("write");
        let error = read_run(&dir, "run-no-newline").expect_err("no newline");
        assert!(
            matches!(error, ReadError::Malformed { line: 8, .. }),
            "{error:?}"
        );
    }

    /// `EventDraft` is re-exported from the writer for callers that add a cost.
    #[test]
    fn a_draft_with_a_cost_is_read_back_unchanged() {
        let dir = TempDir::new("osf-reader-cost");
        let cost = crate::journal::Cost {
            usd_micros: Some(7),
            input_tokens: Some(11),
            output_tokens: None,
        };
        let mut journal = Journal::open(&dir, "run-cost").expect("open");
        journal
            .append_draft(EventDraft {
                actor: actor(),
                timestamp_ms: 5,
                cost: Some(cost.clone()),
                payload: verification(),
            })
            .expect("append draft");
        let read = read_run(&dir, "run-cost").expect("read");
        assert_eq!(read.events.first().expect("event").cost, Some(cost));
    }

    #[test]
    fn a_complete_journal_reads_as_complete() {
        let dir = TempDir::new("osf-reader-complete");
        write_all_eight(&dir, "run-done");
        let journal = read_run(&dir, "run-done").expect("read");
        assert!(journal.complete);
    }

    #[test]
    fn deleting_the_last_line_reads_as_incomplete() {
        let dir = TempDir::new("osf-reader-cut");
        let written = write_all_eight(&dir, "run-cut");
        let mut lines = journal_lines(&dir, "run-cut");
        lines.pop();
        write_lines(&dir, "run-cut", &lines);
        let journal = read_run(&dir, "run-cut").expect("read");
        assert_eq!(journal.events.len(), written.len() - 1);
        assert!(!journal.complete);
    }

    #[test]
    fn keeping_only_two_lines_reads_as_incomplete() {
        let dir = TempDir::new("osf-reader-two");
        write_all_eight(&dir, "run-two");
        let mut lines = journal_lines(&dir, "run-two");
        lines.truncate(2);
        write_lines(&dir, "run-two", &lines);
        let journal = read_run(&dir, "run-two").expect("read");
        assert_eq!(journal.events.len(), 2);
        assert!(!journal.complete);
    }

    #[test]
    fn an_empty_journal_file_reads_as_incomplete() {
        let dir = TempDir::new("osf-reader-none");
        write_lines(&dir, "run-none", &[]);
        let journal = read_run(&dir, "run-none").expect("read");
        assert!(journal.events.is_empty());
        assert!(!journal.complete);
    }

    #[test]
    fn an_event_after_run_complete_is_refused_by_the_reader() {
        let dir = TempDir::new("osf-reader-after-complete");
        write_all_eight(&dir, "run-after-complete");
        let mut lines = journal_lines(&dir, "run-after-complete");
        let last: Event = serde_json::from_str(lines.last().expect("run-complete")).expect("event");
        let mut event = Event {
            schema_version: last.schema_version,
            run: last.run.clone(),
            work_item: last.work_item.clone(),
            change: last.change.clone(),
            actor: actor(),
            timestamp_ms: 9,
            cost: None,
            payload: verification(),
            prev_hash: last.hash.clone(),
            hash: String::new(),
        };
        rehash(&mut event);
        lines.push(serde_json::to_string(&event).expect("event serialises"));
        write_lines(&dir, "run-after-complete", &lines);
        let error = read_run(&dir, "run-after-complete").expect_err("event after run-complete");
        assert!(
            matches!(error, ReadError::EventAfterRunComplete { line: 9 }),
            "{error:?}"
        );
    }
}
