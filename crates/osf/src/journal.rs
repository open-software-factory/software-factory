//! Hash-chained journal events, appended one per line to a local buffer
//! file, so a checkpoint runner can later replay what happened without
//! trusting wall-clock time.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "kebab-case", tag = "event_type", content = "payload")]
pub enum Payload {
    Verification(Verification),
    CheckpointComplete(CheckpointComplete),
    ReviewAnswer(ReviewAnswer),
    ReviewDecision(ReviewDecision),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Verification {
    pub check: String,
    pub slot: Option<String>,
    pub checkpoint: String,
    pub result: CheckResult,
    pub duration_ms: u64,
    pub cache: Option<String>,
    pub findings: u32,
    pub grade: String,
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CheckResult {
    Passed,
    Failed,
    Skipped,
    CouldNotRun,
    NothingToCheck,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CheckpointComplete {
    pub checkpoint: String,
    pub commit: Option<String>,
    pub result: CheckResult,
    pub checks: u32,
    /// Slot name to that slot's result, decisions 0012-0014: a `BTreeMap`
    /// so its JSON key order, and so its place in the replay hash, never
    /// depends on the order tasks happened to run in.
    pub slots: std::collections::BTreeMap<String, CheckResult>,
}

/// One reviewer's outcome for one lens: its own scores and findings, or the
/// reason it counts as missing.
///
/// `transcript` is a path or nothing, never a prompt or a raw answer: the
/// journal carries no secret text, and a prompt is already redacted by
/// `review_context` before any reviewer ever sees it.
///
/// `grade` is always `"reported"`: a reviewer's judgment is an agent saying
/// so, unmeasured, exactly the grade decisions 0005 and 0016 give it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReviewAnswer {
    pub lens: String,
    pub reviewer: String,
    pub family: String,
    /// The model the reviewer's harness was pinned to, when its roster
    /// entry named one. `None` when the harness ran with its own default.
    pub model: Option<String>,
    pub result: String,
    pub scores: BTreeMap<String, f64>,
    pub findings_kept: u32,
    pub findings_dropped: u32,
    pub transcript: Option<String>,
    pub reason: Option<String>,
    pub grade: String,
    /// This reviewer's own attempt number for this lens, one-based: more
    /// than one when the quorum rule asks its family for extra rounds.
    pub round: u32,
}

/// The whole review's verdict, with each lens's own outcome alongside the
/// weighted score that decided it.
///
/// `score` and `threshold` are `None` exactly when `verdict` is
/// `"could-not-run"`: a could-not-run review was never scored against the
/// threshold, so there is nothing genuine to report next to it.
///
/// `grade` is always `"reported"`, the same grade as the [`ReviewAnswer`]s
/// it is decided from: a verdict reached by weighing reported judgments is
/// itself reported, not independently observed or measured.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReviewDecision {
    pub verdict: String,
    pub lenses: Vec<(String, String)>,
    pub score: Option<f64>,
    pub threshold: Option<f64>,
    /// The families that built this change, from every commit's own
    /// `Code-Generator:` trailer in the reviewed range, or the
    /// `--builder-family` flag when the caller named one. Read
    /// `["unknown"]` as no trailer naming a known family was found; the
    /// roster then ran with nothing left out.
    pub builder_families: Vec<String>,
    pub grade: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Event {
    pub schema_version: u32,
    pub run: String,
    pub actor: String,
    pub timestamp_ms: u64,
    #[serde(flatten)]
    pub payload: Payload,
    pub prev_hash: String,
    pub hash: String,
}

/// The all-zero hash a run's first event chains from.
fn genesis_hash() -> String {
    "0".repeat(64)
}

/// The lower-case SHA-256 hex digest of `bytes`, for anything outside this
/// module that needs a stable content fingerprint (the checkpoint runner's
/// `OSF_FILES_HASH`).
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    to_hex(&hasher.finalize())
}

/// Lower-case hex of `bytes`.
fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(out, "{byte:02x}").expect("writing to a string never fails");
    }
    out
}

/// `payload` with every value decision 0005 excludes from the replay hash
/// neutralised. The run identifier is not hashed at all, since it is built
/// from the wall-clock time the run started and the process id, and names
/// nothing about what the run decided. A verification's own `duration_ms`
/// varies run to run even when every decision is identical, so it is
/// zeroed here, and a review answer's `transcript` path, a per-run temporary
/// location, is dropped; the real values still reach the stored event untouched,
/// since this is only ever used to compute a hash. The `cache` field is
/// dropped for the same reason: a first run misses the cache and a repeat
/// run hits it, with identical decisions.
fn replay_payload(payload: &Payload) -> Payload {
    match payload {
        Payload::Verification(v) => Payload::Verification(Verification {
            duration_ms: 0,
            cache: None,
            ..v.clone()
        }),
        Payload::ReviewAnswer(a) => Payload::ReviewAnswer(ReviewAnswer {
            transcript: None,
            ..a.clone()
        }),
        Payload::CheckpointComplete(_) | Payload::ReviewDecision(_) => payload.clone(),
    }
}

/// The event's SHA-256 hex digest over `prev_hash`, `actor`, and the
/// canonical JSON of [`replay_payload`]'s neutralised `payload`.
/// "Canonical" here means `serde_json::to_string` of the `Payload` value:
/// field order is fixed by the struct declaration, so it is stable. Wall
/// clock time and timing variance are excluded, as decision 0005 requires:
/// two runs with identical inputs and identical decisions must produce an
/// identical head hash, including runs whose own run identifiers and task
/// durations necessarily differ.
///
/// # Panics
/// Never in practice: `Payload` holds no maps and no non-finite floats, so
/// `serde_json::to_string` cannot fail on it.
#[must_use]
pub fn event_hash(prev_hash: &str, actor: &str, payload: &Payload) -> String {
    let payload_json =
        serde_json::to_string(&replay_payload(payload)).expect("Payload always serialises to JSON");
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(b"\n");
    hasher.update(actor.as_bytes());
    hasher.update(b"\n");
    hasher.update(payload_json.as_bytes());
    to_hex(&hasher.finalize())
}

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
            last_hash: genesis_hash(),
        })
    }

    /// Computes the event's hash, appends it as one JSON line with a
    /// trailing newline, and returns the event that was written.
    ///
    /// # Errors
    /// Returns an error when the event cannot be serialised or the buffer
    /// file cannot be opened or written to.
    pub fn append(
        &mut self,
        actor: &str,
        timestamp_ms: u64,
        payload: Payload,
    ) -> Result<Event, String> {
        let hash = event_hash(&self.last_hash, actor, &payload);
        let event = Event {
            schema_version: SCHEMA_VERSION,
            run: self.run.clone(),
            actor: actor.to_string(),
            timestamp_ms,
            payload,
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

    fn verification(check: &str) -> Payload {
        Payload::Verification(Verification {
            check: check.into(),
            slot: Some("lint".into()),
            checkpoint: "pre-commit".into(),
            result: CheckResult::Passed,
            duration_ms: 12,
            cache: Some("miss".into()),
            findings: 0,
            grade: "observed".into(),
            reason: None,
        })
    }

    #[test]
    fn two_runs_with_the_same_events_have_the_same_head_hash_whatever_the_time() {
        let a_dir = TempDir::new("osf-journal-a");
        let b_dir = TempDir::new("osf-journal-b");
        let mut a = Journal::open(&a_dir, "run-1").expect("open");
        let mut b = Journal::open(&b_dir, "run-1").expect("open");
        a.append("osf", 1, verification("scan")).expect("append");
        let ha = a
            .append("osf", 2, verification("fmt"))
            .expect("append")
            .hash;
        b.append("osf", 900, verification("scan")).expect("append");
        let hb = b
            .append("osf", 901, verification("fmt"))
            .expect("append")
            .hash;
        assert_eq!(ha, hb);
    }

    fn verification_with_duration(check: &str, duration_ms: u64) -> Payload {
        match verification(check) {
            Payload::Verification(v) => Payload::Verification(Verification { duration_ms, ..v }),
            other @ (Payload::CheckpointComplete(_)
            | Payload::ReviewAnswer(_)
            | Payload::ReviewDecision(_)) => other,
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
        a.append("osf", 1, verification_with_duration("scan", 12))
            .expect("append");
        let ha = a
            .append("osf", 2, verification_with_duration("fmt", 34))
            .expect("append")
            .hash;
        b.append("osf", 900, verification_with_duration("scan", 56))
            .expect("append");
        let hb = b
            .append("osf", 901, verification_with_duration("fmt", 78))
            .expect("append")
            .hash;
        assert_eq!(ha, hb);
    }

    fn review_answer(transcript: Option<&str>) -> Payload {
        Payload::ReviewAnswer(ReviewAnswer {
            lens: "correctness".into(),
            reviewer: "reviewer-a".into(),
            family: "family-a".into(),
            model: None,
            result: "answered".into(),
            scores: BTreeMap::new(),
            findings_kept: 1,
            findings_dropped: 0,
            transcript: transcript.map(str::to_string),
            reason: None,
            grade: "reported".into(),
            round: 1,
        })
    }

    /// The transcript path names a temporary file that differs on every run,
    /// so it must not reach the replay hash; a decided value still does.
    #[test]
    fn a_review_answer_hashes_the_same_whatever_its_transcript_path() {
        let a = event_hash("0", "osf", &review_answer(Some("/tmp/run-1/a.log")));
        let b = event_hash("0", "osf", &review_answer(Some("/tmp/run-2/b.log")));
        let none = event_hash("0", "osf", &review_answer(None));
        assert_eq!(a, b);
        assert_eq!(a, none);
        let Payload::ReviewAnswer(mut changed) = review_answer(None) else {
            unreachable!("review_answer builds a ReviewAnswer");
        };
        changed.findings_kept = 2;
        assert_ne!(a, event_hash("0", "osf", &Payload::ReviewAnswer(changed)));
    }

    /// A first run misses the cache and a repeat run hits it, with the same decisions: decision 0005 requires the same head hash.
    #[test]
    fn a_cache_miss_and_a_cache_hit_have_the_same_head_hash() {
        let with_cache = |cache: &str| match verification("scan") {
            Payload::Verification(v) => Payload::Verification(Verification {
                cache: Some(cache.into()),
                ..v
            }),
            other @ (Payload::CheckpointComplete(_)
            | Payload::ReviewAnswer(_)
            | Payload::ReviewDecision(_)) => other,
        };
        let a_dir = TempDir::new("osf-journal-cache-miss");
        let b_dir = TempDir::new("osf-journal-cache-hit");
        let mut a = Journal::open(&a_dir, "run-1").expect("open");
        let mut b = Journal::open(&b_dir, "run-2").expect("open");
        let ha = a.append("osf", 1, with_cache("miss")).expect("append").hash;
        let hb = b.append("osf", 2, with_cache("hit")).expect("append").hash;
        assert_eq!(ha, hb);
    }

    #[test]
    fn each_event_carries_the_hash_of_the_one_before() {
        let dir = TempDir::new("osf-journal-chain");
        let mut j = Journal::open(&dir, "run-2").expect("open");
        let first = j.append("osf", 1, verification("scan")).expect("append");
        let second = j.append("osf", 2, verification("fmt")).expect("append");
        assert_eq!(first.prev_hash, "0".repeat(64));
        assert_eq!(second.prev_hash, first.hash);
        let text = std::fs::read_to_string(dir.join("buffer/run-2.jsonl")).expect("file");
        assert_eq!(text.lines().count(), 2);
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
            j.append("osf", 1, verification("scan")).expect("append");
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
