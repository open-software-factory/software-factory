//! Hash-chained journal events, appended one per line to a run's journal
//! file under the state directory, so a checkpoint runner can later replay
//! what happened without trusting wall-clock time.

mod event;
mod hash;
mod index;
mod reader;
mod schema;

pub use event::{
    Actor, ActorKind, Attention, BlockedCause, Change, CheckResult, CheckpointComplete, Cost,
    Event, EventDraft, EvidenceGrade, Finding, Payload, Review, RunComplete, RunOutcome,
    RunStarted, Severity, StateChange, Verification, WorkItemState, SCHEMA_VERSION,
};
use hash::genesis_hash;
pub(crate) use hash::sha256_hex;
pub use hash::{event_hash, HashInput};
pub use index::{record_run, runs_for_work_item};
pub use reader::{read_run, ReadError, RunJournal};
pub use schema::validate as validate_event;

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Reads `OSF_STATE_DIR`; otherwise `<home>/.osf/state`, where home comes
/// from `USERPROFILE` on Windows and `HOME` elsewhere. Neither being set is
/// an error, never a silent default.
///
/// # Errors
/// Returns an error when `OSF_STATE_DIR` is unset and the platform's home
/// variable is also unset.
pub fn state_dir() -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("OSF_STATE_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home_var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let home = std::env::var(home_var).map_err(|_| {
        format!("cannot find the state directory: neither OSF_STATE_DIR nor {home_var} is set")
    })?;
    Ok(PathBuf::from(home).join(".osf").join("state"))
}

/// Refuses a run id that is not a safe single path component: an empty id,
/// one carrying a path separator, or one carrying `..`.
pub(super) fn validate_run_id(run: &str) -> Result<(), String> {
    if run.is_empty() {
        return Err("run id must not be empty".to_string());
    }
    if run.contains('/') || run.contains('\\') {
        return Err(format!("run id '{run}' must not contain a path separator"));
    }
    if run.contains("..") {
        return Err(format!("run id '{run}' must not contain '..'"));
    }
    Ok(())
}

/// A run's hash-chained event log, appended to
/// `<state_dir>/runs/<run>.jsonl`.
#[derive(Debug)]
pub struct Journal {
    path: PathBuf,
    state_dir: PathBuf,
    run: String,
    work_item: Option<String>,
    change: Option<Change>,
    last_hash: String,
    index_recorded: bool,
}

impl Journal {
    /// Opens the journal file for `run` under `state_dir`, creating the
    /// `runs` directory if needed. Each run is its own hash chain starting
    /// from genesis, so a run whose journal file already holds events is
    /// refused rather than resumed: resuming would let two processes
    /// interleave one chain, and a real caller only ever reuses a run id by
    /// mistake.
    ///
    /// # Errors
    /// Returns an error when `run` is not a safe single path component, when
    /// the `runs` directory cannot be created, or when the journal file
    /// cannot be inspected. Returns an error naming the file when it already
    /// holds events.
    pub fn open(state_dir: &Path, run: &str) -> Result<Journal, String> {
        validate_run_id(run)?;
        let runs_dir = state_dir.join("runs");
        std::fs::create_dir_all(&runs_dir).map_err(|e| {
            format!(
                "cannot create the journal runs directory {}: {e}",
                runs_dir.display()
            )
        })?;
        let path = runs_dir.join(format!("{run}.jsonl"));
        let has_events = match std::fs::metadata(&path) {
            Ok(meta) => meta.len() > 0,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => {
                return Err(format!(
                    "cannot check the journal file {}: {e}",
                    path.display()
                ))
            }
        };
        if has_events {
            return Err(format!(
                "cannot open run '{run}': {} already has events; each run must use a unique id",
                path.display()
            ));
        }
        Ok(Journal {
            path,
            state_dir: state_dir.to_path_buf(),
            run: run.to_string(),
            work_item: None,
            change: None,
            last_hash: genesis_hash(),
            index_recorded: false,
        })
    }

    /// Sets the provider-qualified work item every appended event carries.
    #[must_use]
    pub fn with_work_item(mut self, work_item: String) -> Self {
        self.work_item = Some(work_item);
        self
    }

    /// Sets the change every appended event carries.
    #[must_use]
    pub fn with_change(mut self, change: Change) -> Self {
        self.change = Some(change);
        self
    }

    /// Appends one event with no cost, using `actor` at `timestamp_ms`.
    ///
    /// # Errors
    /// Returns an error when the event is invalid, cannot be serialised, or
    /// the journal file cannot be opened or written to.
    pub fn append(
        &mut self,
        actor: &Actor,
        timestamp_ms: u64,
        payload: Payload,
    ) -> Result<Event, String> {
        self.append_draft(EventDraft {
            actor: actor.clone(),
            timestamp_ms,
            cost: None,
            payload,
        })
    }

    /// Validates the draft's event against the schema, appends it as one
    /// JSON line with a trailing newline, and returns the event that was
    /// written. The event carries this journal's run, work item and change.
    /// A run-complete event must name the journal's current head hash. An
    /// invalid event writes no byte and leaves the chain's head unchanged.
    ///
    /// # Errors
    /// Returns an error when the event is invalid, cannot be serialised, or
    /// the journal file cannot be opened or written to.
    pub fn append_draft(&mut self, draft: EventDraft) -> Result<Event, String> {
        let hash = event_hash(&HashInput {
            prev_hash: &self.last_hash,
            payload: &draft.payload,
            work_item: self.work_item.as_deref(),
            change: self.change.as_ref(),
            actor: &draft.actor,
            cost: draft.cost.as_ref(),
        });
        let event = Event {
            schema_version: SCHEMA_VERSION,
            run: self.run.clone(),
            work_item: self.work_item.clone(),
            change: self.change.clone(),
            actor: draft.actor,
            timestamp_ms: draft.timestamp_ms,
            cost: draft.cost,
            payload: draft.payload,
            prev_hash: self.last_hash.clone(),
            hash: hash.clone(),
        };
        let value = serde_json::to_value(&event)
            .map_err(|e| format!("cannot serialise journal event: {e}"))?;
        let mut reasons = match schema::validate(&value) {
            Ok(()) => Vec::new(),
            Err(reasons) => reasons,
        };
        if let Payload::RunComplete(complete) = &event.payload {
            if complete.head_hash != self.last_hash {
                reasons.push(format!(
                    "run-complete head_hash {} does not match the journal head {}",
                    complete.head_hash, self.last_hash
                ));
            }
        }
        if !reasons.is_empty() {
            return Err(format!("invalid journal event: {}", reasons.join("; ")));
        }
        // The first accepted event records the run in its work item's index before any line is written.
        if !self.index_recorded {
            if let Some(work_item) = self.work_item.as_deref() {
                index::record_run(&self.state_dir, work_item, &self.run)?;
                self.index_recorded = true;
            }
        }
        let line = serde_json::to_string(&event)
            .map_err(|e| format!("cannot serialise journal event: {e}"))?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| format!("cannot open journal file {}: {e}", self.path.display()))?;
        writeln!(file, "{line}")
            .map_err(|e| format!("cannot write to journal file {}: {e}", self.path.display()))?;
        self.last_hash = hash;
        Ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    fn actor() -> Actor {
        Actor::system("osf")
    }

    fn verification(check: &str) -> Payload {
        Payload::Verification(Verification {
            check: check.into(),
            check_type: None,
            slot: Some("lint".into()),
            checkpoint: "pre-commit".into(),
            result: CheckResult::Passed,
            duration_ms: 12,
            cache: Some("miss".into()),
            findings: 0,
            summary: None,
            grade: EvidenceGrade::Observed,
            reason: None,
        })
    }

    #[test]
    fn two_runs_with_the_same_events_have_the_same_head_hash_whatever_the_time() {
        let a_dir = TempDir::new("osf-journal-a");
        let b_dir = TempDir::new("osf-journal-b");
        let mut a = Journal::open(&a_dir, "run-1").expect("open");
        let mut b = Journal::open(&b_dir, "run-1").expect("open");
        a.append(&actor(), 1, verification("scan")).expect("append");
        let ha = a
            .append(&actor(), 2, verification("fmt"))
            .expect("append")
            .hash;
        b.append(&actor(), 900, verification("scan"))
            .expect("append");
        let hb = b
            .append(&actor(), 901, verification("fmt"))
            .expect("append")
            .hash;
        assert_eq!(ha, hb);
    }

    fn verification_with_duration(check: &str, duration_ms: u64) -> Payload {
        match verification(check) {
            Payload::Verification(v) => Payload::Verification(Verification { duration_ms, ..v }),
            other => other,
        }
    }

    /// `checkpoint.rs` builds a run id from the checkpoint label, the
    /// wall-clock start time and the process id, so two runs of the same
    /// unchanged checkpoint never share one: decision 0005 requires their
    /// head hashes to still match, which the run id being left out of
    /// [`event_hash`] is what makes possible. A different task duration
    /// each run, as a real rerun always has, must not break the match
    /// either.
    #[test]
    fn two_runs_with_different_run_ids_and_durations_have_the_same_head_hash() {
        let a_dir = TempDir::new("osf-journal-run-id-a");
        let b_dir = TempDir::new("osf-journal-run-id-b");
        let mut a = Journal::open(&a_dir, "pre-push-1000-111").expect("open");
        let mut b = Journal::open(&b_dir, "pre-push-2000-222").expect("open");
        a.append(&actor(), 1, verification_with_duration("scan", 12))
            .expect("append");
        let ha = a
            .append(&actor(), 2, verification_with_duration("fmt", 34))
            .expect("append")
            .hash;
        b.append(&actor(), 900, verification_with_duration("scan", 56))
            .expect("append");
        let hb = b
            .append(&actor(), 901, verification_with_duration("fmt", 78))
            .expect("append")
            .hash;
        assert_eq!(ha, hb);
    }

    /// A first run misses the cache and a repeat run hits it, with the same decisions: decision 0005 requires the same head hash.
    #[test]
    fn a_cache_miss_and_a_cache_hit_have_the_same_head_hash() {
        let with_cache = |cache: &str| match verification("scan") {
            Payload::Verification(v) => Payload::Verification(Verification {
                cache: Some(cache.into()),
                ..v
            }),
            other => other,
        };
        let a_dir = TempDir::new("osf-journal-cache-miss");
        let b_dir = TempDir::new("osf-journal-cache-hit");
        let mut a = Journal::open(&a_dir, "run-1").expect("open");
        let mut b = Journal::open(&b_dir, "run-2").expect("open");
        let ha = a
            .append(&actor(), 1, with_cache("miss"))
            .expect("append")
            .hash;
        let hb = b
            .append(&actor(), 2, with_cache("hit"))
            .expect("append")
            .hash;
        assert_eq!(ha, hb);
    }

    /// The same event at a different wall-clock time hashes the same, since
    /// decision 0005 excludes `timestamp_ms` from the digest.
    #[test]
    fn the_same_event_at_a_different_timestamp_hashes_the_same() {
        let a_dir = TempDir::new("osf-journal-time-a");
        let b_dir = TempDir::new("osf-journal-time-b");
        let mut a = Journal::open(&a_dir, "run-1").expect("open");
        let mut b = Journal::open(&b_dir, "run-1").expect("open");
        let ha = a
            .append(&actor(), 1, verification("scan"))
            .expect("append")
            .hash;
        let hb = b
            .append(&actor(), 9_999_999, verification("scan"))
            .expect("append")
            .hash;
        assert_eq!(ha, hb);
    }

    #[test]
    fn each_event_carries_the_hash_of_the_one_before() {
        let dir = TempDir::new("osf-journal-chain");
        let mut j = Journal::open(&dir, "run-2").expect("open");
        let first = j.append(&actor(), 1, verification("scan")).expect("append");
        let second = j.append(&actor(), 2, verification("fmt")).expect("append");
        assert_eq!(first.prev_hash, "0".repeat(64));
        assert_eq!(second.prev_hash, first.hash);
        let text = std::fs::read_to_string(dir.join("runs/run-2.jsonl")).expect("file");
        assert_eq!(text.lines().count(), 2);
    }

    #[test]
    fn work_item_and_change_are_carried_into_every_event() {
        let dir = TempDir::new("osf-journal-work-item");
        let change = Change {
            branch: "main".into(),
            commit: "abc123".into(),
        };
        let mut j = Journal::open(&dir, "run-3")
            .expect("open")
            .with_work_item("github:open-software-factory/example#1".to_string())
            .with_change(change.clone());
        let event = j.append(&actor(), 1, verification("scan")).expect("append");
        assert_eq!(
            event.work_item.as_deref(),
            Some("github:open-software-factory/example#1")
        );
        assert_eq!(event.change, Some(change));
    }

    #[test]
    fn append_draft_records_the_cost_it_is_given() {
        let dir = TempDir::new("osf-journal-draft");
        let mut j = Journal::open(&dir, "run-4").expect("open");
        let cost = Cost {
            usd_micros: Some(7),
            input_tokens: Some(11),
            output_tokens: None,
        };
        let event = j
            .append_draft(EventDraft {
                actor: actor(),
                timestamp_ms: 5,
                cost: Some(cost.clone()),
                payload: verification("scan"),
            })
            .expect("append draft");
        assert_eq!(event.cost, Some(cost));
    }

    #[test]
    fn an_unwritable_state_dir_is_an_error() {
        let base = TempDir::new("osf-journal-blocked");
        let file = base.join("not-a-dir");
        std::fs::write(&file, "x").expect("file");
        assert!(Journal::open(&file, "run-3").is_err());
    }

    #[test]
    fn opening_a_run_that_already_has_events_is_an_error() {
        let dir = TempDir::new("osf-journal-existing");
        {
            let mut j = Journal::open(&dir, "run-4").expect("open");
            j.append(&actor(), 1, verification("scan")).expect("append");
        }
        let err = Journal::open(&dir, "run-4").expect_err("run already has events");
        assert!(err.contains("run-4.jsonl"), "{err}");
    }

    #[test]
    fn an_existing_empty_journal_file_opens_fine() {
        let dir = TempDir::new("osf-journal-empty");
        let runs_dir = dir.join("runs");
        std::fs::create_dir_all(&runs_dir).expect("runs dir");
        std::fs::write(runs_dir.join("run-5.jsonl"), "").expect("empty file");
        Journal::open(&dir, "run-5").expect("open");
    }

    /// A validator built in the test directly from the schema file, never
    /// through `schema::validate`.
    fn test_validator() -> jsonschema::Validator {
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("journal/event.schema.json")).expect("schema JSON");
        jsonschema::validator_for(&schema).expect("schema compiles")
    }

    fn draft(payload: Payload) -> EventDraft {
        EventDraft {
            actor: actor(),
            timestamp_ms: 1,
            cost: None,
            payload,
        }
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
    fn write_eight_events(j: &mut Journal) -> Vec<Event> {
        let mut events = vec![j
            .append_draft(draft(Payload::RunStarted(RunStarted {
                title: Some("Ship the fix".into()),
                repository: Some("github:open-software-factory/example".into()),
                retry_of: None,
                transcript: Some("runs/run-1/transcript.jsonl".into()),
            })))
            .expect("run-started")];
        for payload in [
            verification("scan"),
            Payload::Review(Review {
                round: 1,
                scope: "the change".into(),
                findings: 0,
                summary: "looked at the diff".into(),
                grade: EvidenceGrade::Derived,
            }),
            finding(EvidenceGrade::Observed, Some("osf:lint")),
            Payload::StateChange(StateChange {
                from: Some(WorkItemState::Ready),
                to: WorkItemState::InProgress,
                cause: None,
            }),
            Payload::Attention(Attention {
                cause: "human".into(),
                summary: "needs a person".into(),
                grade: EvidenceGrade::Unverified,
            }),
            Payload::CheckpointComplete(CheckpointComplete {
                checkpoint: "pre-push".into(),
                commit: Some("abc123".into()),
                result: CheckResult::Passed,
                checks: 3,
                slots: std::collections::BTreeMap::from([(
                    "lint".to_string(),
                    CheckResult::Passed,
                )]),
            }),
        ] {
            events.push(j.append_draft(draft(payload)).expect("append"));
        }
        let head = events.last().expect("at least one event").hash.clone();
        events.push(
            j.append_draft(draft(run_complete(head)))
                .expect("run-complete"),
        );
        events
    }

    #[test]
    fn the_writer_accepts_every_event_type_and_each_line_validates() {
        let dir = TempDir::new("osf-journal-eight-types");
        let mut j = Journal::open(&dir, "run-8").expect("open");
        let events = write_eight_events(&mut j);
        assert_eq!(events.len(), 8);
        let text = std::fs::read_to_string(dir.join("runs/run-8.jsonl")).expect("file");
        let validator = test_validator();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 8);
        for line in lines {
            let value: serde_json::Value = serde_json::from_str(line).expect("line is JSON");
            assert!(validator.is_valid(&value), "{line}");
        }
    }

    /// Recomputes the chain from the reader's events and a hand-built genesis,
    /// and validates every written line directly from the schema file.
    #[test]
    fn the_reader_reproduces_the_written_chain_and_every_line_validates() {
        let dir = TempDir::new("osf-journal-replay");
        let mut j = Journal::open(&dir, "run-replay").expect("open");
        let written = write_eight_events(&mut j);
        let journal = super::read_run(&dir, "run-replay").expect("read");
        assert_eq!(journal.events, written);
        let validator = test_validator();
        let text = std::fs::read_to_string(dir.join("runs/run-replay.jsonl")).expect("file");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), journal.events.len());
        let mut head = "0".repeat(64);
        for (line, event) in lines.iter().zip(&journal.events) {
            let value: serde_json::Value = serde_json::from_str(line).expect("line is JSON");
            assert!(validator.is_valid(&value), "{line}");
            assert_eq!(event.prev_hash, head, "{line}");
            let recomputed = event_hash(&HashInput {
                prev_hash: &head,
                payload: &event.payload,
                work_item: event.work_item.as_deref(),
                change: event.change.as_ref(),
                actor: &event.actor,
                cost: event.cost.as_ref(),
            });
            assert_eq!(event.hash, recomputed, "{line}");
            head = recomputed;
        }
        assert_eq!(journal.head_hash, head);
    }

    #[test]
    fn a_finding_with_a_reported_grade_and_verified_by_is_refused() {
        let dir = TempDir::new("osf-journal-finding-reported");
        let mut j = Journal::open(&dir, "run-finding").expect("open");
        let err = j
            .append_draft(draft(finding(EvidenceGrade::Reported, Some("osf:lint"))))
            .expect_err("reported evidence may not claim verification");
        assert!(err.contains("verified_by"), "{err}");
    }

    #[test]
    fn a_finding_with_an_observed_grade_and_verified_by_is_accepted() {
        let dir = TempDir::new("osf-journal-finding-observed");
        let mut j = Journal::open(&dir, "run-finding").expect("open");
        j.append_draft(draft(finding(EvidenceGrade::Observed, Some("osf:lint"))))
            .expect("observed evidence may name its verifier");
    }

    #[test]
    fn a_state_change_to_blocked_requires_a_cause() {
        let dir = TempDir::new("osf-journal-blocked-no-cause");
        let mut j = Journal::open(&dir, "run-state").expect("open");
        let err = j
            .append_draft(draft(Payload::StateChange(StateChange {
                from: Some(WorkItemState::InProgress),
                to: WorkItemState::Blocked,
                cause: None,
            })))
            .expect_err("blocked without a cause");
        assert!(err.contains("cause"), "{err}");
        j.append_draft(draft(Payload::StateChange(StateChange {
            from: Some(WorkItemState::InProgress),
            to: WorkItemState::Blocked,
            cause: Some(BlockedCause::Human),
        })))
        .expect("blocked with a cause");
    }

    #[test]
    fn a_state_change_to_another_state_must_not_carry_a_cause() {
        let dir = TempDir::new("osf-journal-cause-elsewhere");
        let mut j = Journal::open(&dir, "run-state").expect("open");
        let err = j
            .append_draft(draft(Payload::StateChange(StateChange {
                from: Some(WorkItemState::Ready),
                to: WorkItemState::InProgress,
                cause: Some(BlockedCause::Human),
            })))
            .expect_err("a cause belongs only to blocked");
        assert!(err.contains("cause"), "{err}");
    }

    #[test]
    fn a_harness_actor_without_a_model_is_refused() {
        let dir = TempDir::new("osf-journal-harness-no-model");
        let mut j = Journal::open(&dir, "run-harness").expect("open");
        let err = j
            .append_draft(EventDraft {
                actor: Actor {
                    kind: ActorKind::Harness,
                    name: "demo-harness".into(),
                    model: None,
                    model_family: None,
                },
                timestamp_ms: 1,
                cost: None,
                payload: verification("scan"),
            })
            .expect_err("a harness must name its model");
        assert!(err.contains("model"), "{err}");
    }

    #[test]
    fn a_refused_event_leaves_the_file_and_the_chain_head_unchanged() {
        let dir = TempDir::new("osf-journal-refused-unchanged");
        let mut j = Journal::open(&dir, "run-unchanged").expect("open");
        let head = j
            .append(&actor(), 1, verification("scan"))
            .expect("append")
            .hash;
        let path = dir.join("runs/run-unchanged.jsonl");
        let before = std::fs::read(&path).expect("read");
        let err = j
            .append_draft(draft(Payload::StateChange(StateChange {
                from: Some(WorkItemState::Ready),
                to: WorkItemState::Blocked,
                cause: None,
            })))
            .expect_err("refused");
        assert!(err.starts_with("invalid journal event: "), "{err}");
        assert_eq!(std::fs::read(&path).expect("read"), before);
        let next = j.append(&actor(), 2, verification("fmt")).expect("append");
        assert_eq!(next.prev_hash, head);
    }

    #[test]
    fn a_run_complete_with_the_wrong_head_hash_is_refused() {
        let dir = TempDir::new("osf-journal-run-complete-wrong");
        let mut j = Journal::open(&dir, "run-complete").expect("open");
        j.append(&actor(), 1, verification("scan")).expect("append");
        let wrong = "a".repeat(64);
        let err = j
            .append_draft(draft(run_complete(wrong.clone())))
            .expect_err("a mismatched head hash");
        assert!(err.contains("head_hash"), "{err}");
        assert!(err.contains(&wrong), "{err}");
    }

    #[test]
    fn a_run_complete_with_the_current_head_hash_is_accepted() {
        let dir = TempDir::new("osf-journal-run-complete-right");
        let mut j = Journal::open(&dir, "run-complete").expect("open");
        let head = j
            .append(&actor(), 1, verification("scan"))
            .expect("append")
            .hash;
        j.append_draft(draft(run_complete(head.clone())))
            .expect("the current head hash");
        let text = std::fs::read_to_string(dir.join("runs/run-complete.jsonl")).expect("file");
        let value: serde_json::Value =
            serde_json::from_str(text.lines().last().expect("lines")).expect("json");
        let head_hash = value
            .get("payload")
            .and_then(|payload| payload.get("head_hash"))
            .and_then(serde_json::Value::as_str);
        assert_eq!(head_hash, Some(head.as_str()));
    }

    #[test]
    fn an_empty_run_id_is_refused() {
        let dir = TempDir::new("osf-journal-run-id-empty");
        let err = Journal::open(&dir, "").expect_err("empty run id");
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn a_run_id_with_a_path_separator_is_refused() {
        let dir = TempDir::new("osf-journal-run-id-separator");
        let err = Journal::open(&dir, "a/b").expect_err("path separator");
        assert!(err.contains("separator"), "{err}");
    }

    #[test]
    fn a_run_id_with_a_parent_component_is_refused() {
        let dir = TempDir::new("osf-journal-run-id-parent");
        let err = Journal::open(&dir, "..").expect_err("parent component");
        assert!(err.contains(".."), "{err}");
    }

    #[test]
    fn the_journal_file_is_under_the_runs_directory() {
        let dir = TempDir::new("osf-journal-path");
        let mut j = Journal::open(&dir, "run-path").expect("open");
        j.append(&actor(), 1, verification("scan")).expect("append");
        assert!(dir.join("runs/run-path.jsonl").is_file());
        assert!(!dir.join("buffer").exists());
    }

    /// A work item with no journal still lists no runs and makes no index dir.
    #[test]
    fn a_journal_without_a_work_item_creates_no_index_directory() {
        let dir = TempDir::new("osf-journal-no-index");
        let mut j = Journal::open(&dir, "run-plain").expect("open");
        j.append(&actor(), 1, verification("scan")).expect("append");
        assert!(!dir.join("index").exists());
    }

    /// The first accepted event records the run before the journal line lands.
    #[test]
    fn the_first_event_records_the_run_in_the_index() {
        let dir = TempDir::new("osf-journal-index-first");
        let work_item = "github:open-software-factory/example#1";
        let mut j = Journal::open(&dir, "run-one")
            .expect("open")
            .with_work_item(work_item.to_string());
        j.append(&actor(), 1, verification("scan")).expect("append");
        assert_eq!(
            runs_for_work_item(&dir, work_item).expect("runs"),
            ["run-one"].map(str::to_string)
        );
        assert!(dir.join("runs/run-one.jsonl").is_file());
    }

    /// Two journals for one work item list both runs in order, once each.
    #[test]
    fn two_journals_for_one_work_item_list_both_runs_in_order() {
        let dir = TempDir::new("osf-journal-index-order");
        let work_item = "github:open-software-factory/example#1".to_string();
        let mut a = Journal::open(&dir, "run-a")
            .expect("open")
            .with_work_item(work_item.clone());
        a.append(&actor(), 1, verification("scan")).expect("append");
        let mut b = Journal::open(&dir, "run-b")
            .expect("open")
            .with_work_item(work_item.clone());
        b.append(&actor(), 1, verification("scan")).expect("append");
        a.append(&actor(), 2, verification("fmt")).expect("append");
        assert_eq!(
            runs_for_work_item(&dir, &work_item).expect("runs"),
            ["run-a", "run-b"].map(str::to_string)
        );
    }

    /// A failed index update refuses the event before any journal line is written.
    #[test]
    fn a_failed_index_update_writes_no_journal_line() {
        let dir = TempDir::new("osf-journal-index-failed");
        let work_item = "github:open-software-factory/example#1";
        let path = index::index_path(&dir, work_item);
        std::fs::create_dir_all(path.parent().expect("index dir")).expect("index dir");
        std::fs::write(&path, "not JSON").expect("write index");
        let mut j = Journal::open(&dir, "run-fail")
            .expect("open")
            .with_work_item(work_item.to_string());
        let err = j
            .append(&actor(), 1, verification("scan"))
            .expect_err("malformed index");
        assert!(err.contains("index"), "{err}");
        assert!(!dir.join("runs/run-fail.jsonl").exists());
    }
}
