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
//!
//! ## Replay digest
//!
//! The replay digest is SHA-256 over the concatenation, for each event in
//! file order, of one decision line followed by one 0x0A byte. An empty run
//! hashes zero bytes, so its digest is
//! `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.
//!
//! A decision line is compact UTF-8 JSON: one object with exactly the keys
//! `actor`, `change`, `payload` and `work_item`, in that order. `change` is
//! written as an object with only the `commit` (the branch name is a label
//! chosen by whoever starts the run, so a replay on a renamed branch shares
//! the digest), and as `null` when absent; `work_item` is written as `null`
//! when absent. Every JSON object in the line has its keys in byte order
//! (sorted). The `serde_json` `preserve_order` feature is on in this
//! workspace, so the code sorts each object explicitly with a small recursive
//! function that rebuilds it with its keys inserted in sorted order, rather
//! than relying on the map type.
//!
//! `payload` is the tagged payload `{"event_type": ..., "payload": {...}}`
//! with these fields removed from the inner object: for verification
//! `duration_ms` and `cache`; for run-started `retry_of` and `transcript`;
//! for run-complete `head_hash` and `transcript_hash`. A field that is `None`
//! is left out, as in the chain.
//!
//! `timestamp_ms`, `cost`, `run`, `schema_version`, `prev_hash`, `hash` and
//! the branch name are never in the line: they hold timings, cache outcomes,
//! cost, run ids, the chain head, transcripts or the label chosen by whoever
//! starts the run, none of which is a decision.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use super::event::{Actor, Change, Cost, Event, Payload};

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

/// A streaming SHA-256 over each event's replay decision line. The module
/// docs give the exact byte layout.
pub(crate) struct ReplayDigest {
    hasher: Sha256,
}

impl ReplayDigest {
    /// A digest over no events: the SHA-256 of zero bytes.
    pub(crate) fn new() -> Self {
        Self {
            hasher: Sha256::new(),
        }
    }

    /// Mixes one event's decision line and its trailing newline into the
    /// digest.
    pub(crate) fn update(&mut self, event: &Event) {
        self.hasher.update(decision_line(event).as_bytes());
        self.hasher.update(b"\n");
    }

    /// The 64 lower-case hex characters of every event seen so far.
    #[must_use]
    pub(crate) fn finish(self) -> String {
        to_hex(&self.hasher.finalize())
    }
}

/// One event's decision line, with no trailing newline. The module docs give
/// the exact layout.
fn decision_line(event: &Event) -> String {
    let mut payload =
        serde_json::to_value(&event.payload).expect("a payload always serialises to JSON");
    if let Some(inner) = payload
        .get_mut("payload")
        .and_then(serde_json::Value::as_object_mut)
    {
        for key in excluded_payload_keys(&event.payload) {
            inner.remove(*key);
        }
    }
    let mut line = serde_json::Map::new();
    line.insert(
        "actor".to_string(),
        serde_json::to_value(&event.actor).expect("an actor always serialises to JSON"),
    );
    line.insert(
        "change".to_string(),
        match &event.change {
            Some(change) => {
                let mut commit = serde_json::Map::new();
                commit.insert(
                    "commit".to_string(),
                    serde_json::Value::String(change.commit.clone()),
                );
                serde_json::Value::Object(commit)
            }
            None => serde_json::Value::Null,
        },
    );
    line.insert("payload".to_string(), payload);
    line.insert(
        "work_item".to_string(),
        match &event.work_item {
            Some(work_item) => serde_json::Value::String(work_item.clone()),
            None => serde_json::Value::Null,
        },
    );
    let sorted = sort_json_keys(serde_json::Value::Object(line));
    serde_json::to_string(&sorted).expect("the decision line always serialises to JSON")
}

/// The payload fields the replay digest leaves out, by event type: timings,
/// cache outcomes, run identity and transcripts are not decisions.
fn excluded_payload_keys(payload: &Payload) -> &'static [&'static str] {
    match payload {
        Payload::Verification(_) => &["duration_ms", "cache"],
        Payload::RunStarted(_) => &["retry_of", "transcript"],
        Payload::RunComplete(_) => &["head_hash", "transcript_hash"],
        _ => &[],
    }
}

/// Rebuilds `value` with every object's keys in byte order. The `serde_json`
/// `preserve_order` feature keeps insertion order, so sorting is explicit.
fn sort_json_keys(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(object) => {
            let sorted: std::collections::BTreeMap<_, _> = object.into_iter().collect();
            let mut rebuilt = serde_json::Map::new();
            for (key, value) in sorted {
                rebuilt.insert(key, sort_json_keys(value));
            }
            serde_json::Value::Object(rebuilt)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(sort_json_keys).collect())
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{
        Attention, CheckResult, EvidenceGrade, RunComplete, RunOutcome, RunStarted, Verification,
        SCHEMA_VERSION,
    };

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

    /// The pinned replay run: run-started, verification, run-complete.
    fn fixed_run() -> [Event; 3] {
        let actor = Actor::system("osf");
        let work_item = Some("github:open-software-factory/example#1".to_string());
        let change = Some(Change {
            branch: "main".into(),
            commit: "abc123".into(),
        });
        [
            Event {
                schema_version: SCHEMA_VERSION,
                run: "run-1".into(),
                work_item: work_item.clone(),
                change: None,
                actor: actor.clone(),
                timestamp_ms: 1,
                cost: None,
                payload: Payload::RunStarted(RunStarted {
                    title: Some("Ship the fix".into()),
                    repository: Some("github:open-software-factory/example".into()),
                    retry_of: Some("run-0".into()),
                    transcript: Some("runs/run-1/transcript.jsonl".into()),
                }),
                prev_hash: String::new(),
                hash: String::new(),
            },
            Event {
                schema_version: SCHEMA_VERSION,
                run: "run-1".into(),
                work_item: work_item.clone(),
                change: change.clone(),
                actor: actor.clone(),
                timestamp_ms: 2,
                cost: Some(Cost {
                    usd_micros: Some(7),
                    input_tokens: None,
                    output_tokens: None,
                }),
                payload: Payload::Verification(Verification {
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
                }),
                prev_hash: String::new(),
                hash: String::new(),
            },
            Event {
                schema_version: SCHEMA_VERSION,
                run: "run-1".into(),
                work_item,
                change,
                actor,
                timestamp_ms: 3,
                cost: None,
                payload: Payload::RunComplete(RunComplete {
                    outcome: RunOutcome::Completed,
                    head_hash: "a".repeat(64),
                    summary: "all checks passed".into(),
                    grade: EvidenceGrade::Derived,
                    transcript_hash: Some("b".repeat(64)),
                }),
                prev_hash: String::new(),
                hash: String::new(),
            },
        ]
    }

    /// The replay digest of one event on its own.
    fn one_event_digest(event: &Event) -> String {
        let mut replay = ReplayDigest::new();
        replay.update(event);
        replay.finish()
    }

    #[test]
    fn a_fixed_run_has_a_known_replay_digest() {
        let events = fixed_run();
        let lines: Vec<String> = events.iter().map(decision_line).collect();
        let expected = [
            r#"{"actor":{"kind":"system","name":"osf"},"change":null,"payload":{"event_type":"run-started","payload":{"repository":"github:open-software-factory/example","title":"Ship the fix"}},"work_item":"github:open-software-factory/example#1"}"#.to_string(),
            r#"{"actor":{"kind":"system","name":"osf"},"change":{"commit":"abc123"},"payload":{"event_type":"verification","payload":{"check":"osf:lint","checkpoint":"pre-commit","findings":0,"grade":"observed","result":"passed"}},"work_item":"github:open-software-factory/example#1"}"#.to_string(),
            r#"{"actor":{"kind":"system","name":"osf"},"change":{"commit":"abc123"},"payload":{"event_type":"run-complete","payload":{"grade":"derived","outcome":"completed","summary":"all checks passed"}},"work_item":"github:open-software-factory/example#1"}"#.to_string(),
        ];
        assert_eq!(lines, expected);
        let mut replay = ReplayDigest::new();
        for event in &events {
            replay.update(event);
        }
        // The digest was computed with sha256sum outside Rust over the three lines each followed by a newline.
        assert_eq!(
            replay.finish(),
            "5a89bc420aeb3a8b4e3adbae902235b03159430f9a84ef55d6bfa0654a921aa3"
        );
    }

    #[test]
    fn a_renamed_branch_with_the_same_commit_keeps_the_replay_digest() {
        let events = fixed_run();
        let original = events.get(1).expect("the verification event").clone();
        let baseline = one_event_digest(&original);

        let mut renamed = original.clone();
        renamed.change = Some(Change {
            branch: "renamed".into(),
            commit: "abc123".into(),
        });
        assert_eq!(one_event_digest(&renamed), baseline);

        let mut recommitted = original.clone();
        recommitted.change = Some(Change {
            branch: "main".into(),
            commit: "def456".into(),
        });
        assert_ne!(one_event_digest(&recommitted), baseline);
    }

    #[test]
    fn an_empty_run_has_the_digest_of_zero_bytes() {
        assert_eq!(
            ReplayDigest::new().finish(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn timings_cache_cost_run_id_and_timestamp_do_not_change_the_replay_digest() {
        let events = fixed_run();
        let original = events.get(1).expect("the verification event").clone();
        let mut changed = original.clone();
        changed.run = "another-run".into();
        changed.timestamp_ms = 999_999;
        changed.cost = None;
        if let Payload::Verification(verification) = &mut changed.payload {
            verification.duration_ms = 900;
            verification.cache = None;
        }
        assert_eq!(one_event_digest(&original), one_event_digest(&changed));
    }

    #[test]
    fn a_changed_verdict_work_item_actor_or_change_changes_the_replay_digest() {
        let events = fixed_run();
        let original = events.get(1).expect("the verification event").clone();
        let baseline = one_event_digest(&original);

        let mut failed = original.clone();
        if let Payload::Verification(verification) = &mut failed.payload {
            verification.result = CheckResult::Failed;
        }
        assert_ne!(one_event_digest(&failed), baseline);

        let mut moved = original.clone();
        moved.work_item = Some("github:open-software-factory/example#2".into());
        assert_ne!(one_event_digest(&moved), baseline);

        let mut renamed = original.clone();
        renamed.actor = Actor::system("other");
        assert_ne!(one_event_digest(&renamed), baseline);

        let mut recommitted = original.clone();
        recommitted.change = Some(Change {
            branch: "main".into(),
            commit: "def456".into(),
        });
        assert_ne!(one_event_digest(&recommitted), baseline);
    }
}
