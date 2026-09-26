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
}

/// One reviewer's outcome for one lens: its own scores and findings, or the
/// reason it counts as missing.
///
/// `transcript` is a path or nothing, never a prompt or a raw answer: the
/// journal carries no secret text, and a prompt is already redacted by
/// `review_context` before any reviewer ever sees it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReviewAnswer {
    pub lens: String,
    pub reviewer: String,
    pub family: String,
    pub result: String,
    pub scores: BTreeMap<String, f64>,
    pub findings_kept: u32,
    pub findings_dropped: u32,
    pub transcript: Option<String>,
    pub reason: Option<String>,
}

/// The whole review's verdict, with each lens's own outcome alongside the
/// weighted score that decided it.
///
/// `score` and `threshold` are `None` exactly when `verdict` is
/// `"could-not-run"`: a could-not-run review was never scored against the
/// threshold, so there is nothing genuine to report next to it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReviewDecision {
    pub verdict: String,
    pub lenses: Vec<(String, String)>,
    pub score: Option<f64>,
    pub threshold: Option<f64>,
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

/// The event's SHA-256 hex digest over `prev_hash`, `run`, `actor`, and the
/// canonical JSON of `payload`. "Canonical" here means `serde_json::to_string`
/// of the `Payload` value: field order is fixed by the struct declaration, so
/// it is stable. Wall-clock time is excluded, as decision 0005 requires.
///
/// # Panics
/// Never in practice: `Payload` holds no maps and no non-finite floats, so
/// `serde_json::to_string` cannot fail on it.
#[must_use]
pub fn event_hash(prev_hash: &str, run: &str, actor: &str, payload: &Payload) -> String {
    let payload_json = serde_json::to_string(payload).expect("Payload always serialises to JSON");
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(b"\n");
    hasher.update(run.as_bytes());
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
        let hash = event_hash(&self.last_hash, &self.run, actor, &payload);
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
