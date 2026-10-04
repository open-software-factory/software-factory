//! Hash-chained journal events, appended one per line to a local buffer
//! file, so a checkpoint runner can later replay what happened without
//! trusting wall-clock time.

mod event;
mod hash;

pub use event::{
    Actor, ActorKind, Attention, BlockedCause, Change, CheckResult, CheckpointComplete, Cost,
    Event, EventDraft, EvidenceGrade, Finding, Payload, Review, RunComplete, RunOutcome,
    RunStarted, Severity, StateChange, Verification, WorkItemState, SCHEMA_VERSION,
};
use hash::genesis_hash;
pub(crate) use hash::sha256_hex;
pub use hash::{event_hash, HashInput};

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

/// A run's hash-chained event log, appended to
/// `<state_dir>/buffer/<run>.jsonl`.
#[derive(Debug)]
pub struct Journal {
    path: PathBuf,
    run: String,
    work_item: Option<String>,
    change: Option<Change>,
    last_hash: String,
}

impl Journal {
    /// Opens the buffer file for `run` under `state_dir`, creating the
    /// `buffer` directory if needed. Each run is its own hash chain starting
    /// from genesis, so a run whose buffer file already holds events is
    /// refused rather than resumed: resuming would let two processes
    /// interleave one chain, and a real caller only ever reuses a run id by
    /// mistake.
    ///
    /// # Errors
    /// Returns the path and the underlying error when the buffer directory
    /// cannot be created, or when the buffer file cannot be inspected.
    /// Returns an error naming the file when it already holds events.
    pub fn open(state_dir: &Path, run: &str) -> Result<Journal, String> {
        let buffer_dir = state_dir.join("buffer");
        std::fs::create_dir_all(&buffer_dir).map_err(|e| {
            format!(
                "cannot create the journal buffer directory {}: {e}",
                buffer_dir.display()
            )
        })?;
        let path = buffer_dir.join(format!("{run}.jsonl"));
        let has_events = match std::fs::metadata(&path) {
            Ok(meta) => meta.len() > 0,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => {
                return Err(format!(
                    "cannot check the journal buffer {}: {e}",
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
            run: run.to_string(),
            work_item: None,
            change: None,
            last_hash: genesis_hash(),
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
    /// Returns an error when the event cannot be serialised or the buffer
    /// file cannot be opened or written to.
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

    /// Computes the draft's hash, appends it as one JSON line with a
    /// trailing newline, and returns the event that was written. The event
    /// carries this journal's run, work item and change.
    ///
    /// # Errors
    /// Returns an error when the event cannot be serialised or the buffer
    /// file cannot be opened or written to.
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
        let line = serde_json::to_string(&event)
            .map_err(|e| format!("cannot serialise journal event: {e}"))?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| format!("cannot open journal buffer {}: {e}", self.path.display()))?;
        writeln!(file, "{line}").map_err(|e| {
            format!(
                "cannot write to journal buffer {}: {e}",
                self.path.display()
            )
        })?;
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
        let text = std::fs::read_to_string(dir.join("buffer/run-2.jsonl")).expect("file");
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
    fn an_existing_empty_buffer_file_opens_fine() {
        let dir = TempDir::new("osf-journal-empty");
        let buffer_dir = dir.join("buffer");
        std::fs::create_dir_all(&buffer_dir).expect("buffer dir");
        std::fs::write(buffer_dir.join("run-5.jsonl"), "").expect("empty file");
        Journal::open(&dir, "run-5").expect("open");
    }
}
