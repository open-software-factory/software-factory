//! The hash chain's digest: SHA-256 over the previous hash and the canonical
//! JSON of an event's replay-relevant parts, with wall-clock time excluded.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use super::event::{Actor, Change, Cost, Payload, Verification};

/// The all-zero hash a run's first event chains from.
pub(crate) fn genesis_hash() -> String {
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
/// neutralised.
fn replay_payload(payload: &Payload) -> Payload {
    match payload {
        // A verification's duration varies run to run even when every decision is identical, so it is zeroed here.
        // A first run misses the cache and a repeat run hits it with identical decisions, so the cache field is dropped.
        Payload::Verification(v) => Payload::Verification(Verification {
            duration_ms: 0,
            cache: None,
            ..v.clone()
        }),
        other => other.clone(),
    }
}

/// The replay-relevant parts of an event: everything the digest covers.
pub struct HashInput<'a> {
    pub prev_hash: &'a str,
    pub payload: &'a Payload,
    pub work_item: Option<&'a str>,
    pub change: Option<&'a Change>,
    pub actor: &'a Actor,
    pub cost: Option<&'a Cost>,
}

/// The event's SHA-256 hex digest over `prev_hash`, a newline, then the
/// canonical JSON of the [`HashInput`]'s `payload`, `work_item`, `change`,
/// `actor` and `cost`. "Canonical" here means `serde_json::to_string` of a
/// struct with fixed field order. `timestamp_ms`, `run` and `hash` are
/// excluded, as decision 0005 requires: two runs with identical inputs and
/// identical decisions must produce an identical head hash, including runs
/// whose own run identifiers and task durations necessarily differ.
///
/// `run` is excluded because it is built from wall-clock time and the
/// process id, so it names nothing about what the run decided.
///
/// # Panics
/// Never in practice: the payload holds no maps and no non-finite floats, so
/// `serde_json::to_string` cannot fail on it.
#[must_use]
pub fn event_hash(input: &HashInput) -> String {
    #[derive(Serialize)]
    struct Canonical<'a> {
        payload: &'a Payload,
        work_item: Option<&'a str>,
        change: Option<&'a Change>,
        actor: &'a Actor,
        cost: Option<&'a Cost>,
    }
    let neutral = replay_payload(input.payload);
    let canonical = Canonical {
        payload: &neutral,
        work_item: input.work_item,
        change: input.change,
        actor: input.actor,
        cost: input.cost,
    };
    let json =
        serde_json::to_string(&canonical).expect("the hashed parts always serialise to JSON");
    let mut hasher = Sha256::new();
    hasher.update(input.prev_hash.as_bytes());
    hasher.update(b"\n");
    hasher.update(json.as_bytes());
    to_hex(&hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{EvidenceGrade, RunStarted};

    fn actor() -> Actor {
        Actor::system("osf")
    }

    fn verification(duration_ms: u64, cache: &str) -> Payload {
        Payload::Verification(Verification {
            check: "osf:lint".into(),
            check_type: None,
            slot: None,
            checkpoint: "pre-commit".into(),
            result: crate::journal::CheckResult::Passed,
            duration_ms,
            cache: Some(cache.into()),
            findings: 0,
            summary: None,
            grade: EvidenceGrade::Observed,
            reason: None,
        })
    }

    fn run_started() -> Payload {
        Payload::RunStarted(RunStarted {
            title: None,
            repository: None,
            retry_of: None,
            transcript: None,
        })
    }

    fn digest(
        payload: &Payload,
        work_item: Option<&str>,
        change: Option<&Change>,
        cost: Option<&Cost>,
    ) -> String {
        event_hash(&HashInput {
            prev_hash: "0",
            payload,
            work_item,
            change,
            actor: &actor(),
            cost,
        })
    }

    #[test]
    fn a_different_work_item_change_and_cost_each_change_the_hash() {
        let payload = run_started();
        let plain = digest(&payload, None, None, None);
        assert_ne!(
            digest(
                &payload,
                Some("github:open-software-factory/example#1"),
                None,
                None
            ),
            plain
        );
        let change = Change {
            branch: "main".into(),
            commit: "abc123".into(),
        };
        assert_ne!(digest(&payload, None, Some(&change), None), plain);
        let cost = Cost {
            usd_micros: Some(1),
            input_tokens: None,
            output_tokens: None,
        };
        assert_ne!(digest(&payload, None, None, Some(&cost)), plain);
    }

    #[test]
    fn a_verification_s_duration_and_cache_do_not_change_the_hash() {
        let miss = verification(12, "miss");
        let hit = verification(999, "hit");
        assert_eq!(
            digest(&miss, None, None, None),
            digest(&hit, None, None, None)
        );
    }
}
