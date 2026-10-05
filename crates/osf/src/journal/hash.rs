//! The hash chain's input layout and its digest.
//!
//! The digest is SHA-256, written as 64 lower-case hex characters.
//!
//! The input is the previous hash as ASCII text, then one newline byte, then
//! the canonical JSON. For the first event of a run the previous hash is 64
//! zeros. The newline byte is 0x0A. The canonical JSON is UTF-8 with no
//! trailing newline.
//!
//! The canonical JSON is the compact output of `serde_json`, with no spaces
//! and no newlines. It is one object with exactly these keys in this order:
//! `payload`, `work_item`, `change`, `actor`, `cost`.
//!
//! `payload` holds the tagged payload object:
//! `{"event_type": <kebab-case name>, "payload": {...}}`. The fields inside
//! follow the declaration order of the Rust struct in `event.rs`. A payload
//! field that is `None` is left out. A top-level `work_item`, `change` or
//! `cost` that is absent is written as `null`.
//!
//! Inside `actor`, `cost` and `change` the same rule applies. Fields keep
//! declaration order and `None` fields are left out.
//!
//! Every field of every event type is hashed as it is, including
//! `duration_ms`, `cache` and `cost`.
//!
//! `timestamp_ms`, `run`, `schema_version` and `hash` are not hashed.
//!
//! The checkpoint-complete `slots` map is a `BTreeMap`. Its keys are sorted
//! by byte order.
//!
//! See the tests `a_fixed_attention_event_has_a_known_hash` and
//! `a_fixed_verification_event_has_a_known_hash` for pinned examples. The
//! first one hashes this canonical JSON:
//!
//! ```text
//! {"payload":{"event_type":"attention","payload":{"cause":"human","summary":"needs a person","grade":"unverified"}},"work_item":null,"change":null,"actor":{"kind":"system","name":"osf"},"cost":null}
//! ```

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use super::event::{Actor, Change, Cost, Payload};

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

/// The replay-relevant parts of an event: everything the digest covers.
pub struct HashInput<'a> {
    pub prev_hash: &'a str,
    pub payload: &'a Payload,
    pub work_item: Option<&'a str>,
    pub change: Option<&'a Change>,
    pub actor: &'a Actor,
    pub cost: Option<&'a Cost>,
}

/// The canonical JSON of the hashed parts.
fn canonical_json(input: &HashInput) -> String {
    #[derive(Serialize)]
    struct Canonical<'a> {
        payload: &'a Payload,
        work_item: Option<&'a str>,
        change: Option<&'a Change>,
        actor: &'a Actor,
        cost: Option<&'a Cost>,
    }
    let canonical = Canonical {
        payload: input.payload,
        work_item: input.work_item,
        change: input.change,
        actor: input.actor,
        cost: input.cost,
    };
    serde_json::to_string(&canonical).expect("the hashed parts always serialise to JSON")
}

/// The event's SHA-256 hex digest. The module docs give the exact byte
/// layout and point to pinned examples.
///
/// `run` is excluded, as decision 0005 requires, because it is built from
/// wall-clock time and the process id. It names nothing about what the run
/// decided.
///
/// # Panics
/// Never in practice: every map in the hashed parts has string keys and no
/// value is a non-finite float, so `serde_json::to_string` cannot fail.
#[must_use]
pub fn event_hash(input: &HashInput) -> String {
    let json = canonical_json(input);
    let mut hasher = Sha256::new();
    hasher.update(input.prev_hash.as_bytes());
    hasher.update(b"\n");
    hasher.update(json.as_bytes());
    to_hex(&hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{Attention, CheckResult, EvidenceGrade, RunStarted, Verification};

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
    fn a_changed_duration_alone_changes_the_hash() {
        let short = verification(12, "miss");
        let long = verification(999, "miss");
        assert_ne!(
            digest(&short, None, None, None),
            digest(&long, None, None, None)
        );
    }

    #[test]
    fn a_changed_cache_alone_changes_the_hash() {
        let miss = verification(12, "miss");
        let hit = verification(12, "hit");
        assert_ne!(
            digest(&miss, None, None, None),
            digest(&hit, None, None, None)
        );
        let mut no_cache = verification(12, "miss");
        if let Payload::Verification(v) = &mut no_cache {
            v.cache = None;
        }
        assert_ne!(
            digest(&no_cache, None, None, None),
            digest(&miss, None, None, None)
        );
    }

    #[test]
    fn a_fixed_attention_event_has_a_known_hash() {
        let actor = Actor::system("osf");
        let prev_hash = genesis_hash();
        let payload = Payload::Attention(Attention {
            cause: "human".into(),
            summary: "needs a person".into(),
            grade: EvidenceGrade::Unverified,
        });
        let input = HashInput {
            prev_hash: prev_hash.as_str(),
            payload: &payload,
            work_item: None,
            change: None,
            actor: &actor,
            cost: None,
        };
        assert_eq!(
            canonical_json(&input),
            r#"{"payload":{"event_type":"attention","payload":{"cause":"human","summary":"needs a person","grade":"unverified"}},"work_item":null,"change":null,"actor":{"kind":"system","name":"osf"},"cost":null}"#
        );
        assert_eq!(
            event_hash(&input),
            "c3b8594be772d0bd201d3ee2d6746f5b8b65d28b1d847c15725ca907da639b97"
        );
    }

    #[test]
    fn a_fixed_verification_event_has_a_known_hash() {
        let actor = Actor::system("osf");
        let payload = Payload::Verification(Verification {
            check: "osf:lint".into(),
            check_type: None,
            slot: None,
            checkpoint: "pre-commit".into(),
            result: CheckResult::Passed,
            duration_ms: 12,
            cache: Some("miss".into()),
            findings: 0,
            summary: None,
            grade: EvidenceGrade::Observed,
            reason: None,
        });
        let change = Change {
            branch: "main".into(),
            commit: "abc123".into(),
        };
        let cost = Cost {
            usd_micros: Some(7),
            input_tokens: None,
            output_tokens: None,
        };
        let input = HashInput {
            prev_hash: "c3b8594be772d0bd201d3ee2d6746f5b8b65d28b1d847c15725ca907da639b97",
            payload: &payload,
            work_item: Some("github:open-software-factory/example#1"),
            change: Some(&change),
            actor: &actor,
            cost: Some(&cost),
        };
        assert_eq!(
            canonical_json(&input),
            r#"{"payload":{"event_type":"verification","payload":{"check":"osf:lint","checkpoint":"pre-commit","result":"passed","duration_ms":12,"cache":"miss","findings":0,"grade":"observed"}},"work_item":"github:open-software-factory/example#1","change":{"branch":"main","commit":"abc123"},"actor":{"kind":"system","name":"osf"},"cost":{"usd_micros":7}}"#
        );
        assert_eq!(
            event_hash(&input),
            "2229bc1dfe606c1d0a8b25a7a38079ed42b58fa9b7d30a6c471dd8af9359867f"
        );
    }
}
