//! The journal's data model: one versioned event envelope, the actor, cost
//! and evidence types it carries, and every payload decisions 0005 and 0014
//! define.

use serde::{Deserialize, Serialize};

/// The envelope version every event carries.
pub const SCHEMA_VERSION: u32 = 1;

/// One event in a run's hash-chained journal, with the payload flattened
/// into the envelope under `event_type` and `payload`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Event {
    pub schema_version: u32,
    pub run: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_item: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<Change>,
    pub actor: Actor,
    pub timestamp_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
    #[serde(flatten)]
    pub payload: Payload,
    pub prev_hash: String,
    pub hash: String,
}

/// The repository change an event belongs to, provider-qualified.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Change {
    pub branch: String,
    pub commit: String,
}

/// Who produced an event: a harness, a person, or the engine itself.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Actor {
    pub kind: ActorKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_family: Option<String>,
}

/// The three kinds of actor decision 0005 lists (harness, person, system).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ActorKind {
    Harness,
    Person,
    System,
}

impl Actor {
    /// The engine itself, acting under `name`.
    #[must_use]
    pub fn system(name: impl Into<String>) -> Self {
        Self {
            kind: ActorKind::System,
            name: name.into(),
            model: None,
            model_family: None,
        }
    }

    /// A person acting under `name`.
    #[must_use]
    pub fn person(name: impl Into<String>) -> Self {
        Self {
            kind: ActorKind::Person,
            name: name.into(),
            model: None,
            model_family: None,
        }
    }

    /// A coding-agent harness `name` running `model` from `family`.
    #[must_use]
    pub fn harness(
        name: impl Into<String>,
        model: impl Into<String>,
        family: impl Into<String>,
    ) -> Self {
        Self {
            kind: ActorKind::Harness,
            name: name.into(),
            model: Some(model.into()),
            model_family: Some(family.into()),
        }
    }
}

/// What an event cost, when it is known.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Cost {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usd_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
}

/// The evidence grade every finding and summary carries, decision 0005.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceGrade {
    Observed,
    Derived,
    Reported,
    Unverified,
}

/// The severity of a finding.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// A work item's lifecycle state, decision 0005.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum WorkItemState {
    Ready,
    InProgress,
    Verifying,
    InReview,
    Deploying,
    Deployed,
    SignedOff,
    Blocked,
    Failed,
    Recovering,
    Aborted,
    RolledBack,
    Paused,
}

/// Why a work item is blocked, decision 0005.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BlockedCause {
    Dependency,
    Human,
    Clarification,
    Ambiguous,
    Capacity,
}

/// How a run ended.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RunOutcome {
    Completed,
    Blocked,
    Failed,
}

/// One event type's own fields, tagged by `event_type` and keyed under
/// `payload`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "kebab-case", tag = "event_type", content = "payload")]
pub enum Payload {
    RunStarted(RunStarted),
    Verification(Verification),
    Review(Review),
    Finding(Finding),
    StateChange(StateChange),
    RunComplete(RunComplete),
    Attention(Attention),
    CheckpointComplete(CheckpointComplete),
    ReviewAnswer(ReviewAnswer),
    ReviewDecision(ReviewDecision),
}

/// A run beginning against a work item.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RunStarted {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_of: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
}

/// One deterministic check's result inside a run.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Verification {
    pub check: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    pub checkpoint: String,
    pub result: CheckResult,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<String>,
    pub findings: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub grade: EvidenceGrade,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A deterministic check's outcome.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CheckResult {
    Passed,
    Failed,
    Skipped,
    CouldNotRun,
    NothingToCheck,
}

/// One judgment pass over a change.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Review {
    pub round: u32,
    pub scope: String,
    pub findings: u32,
    pub summary: String,
    pub grade: EvidenceGrade,
}

/// A located claim from a verifier or a review.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Finding {
    pub rule: String,
    pub severity: Severity,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    pub message: String,
    pub grade: EvidenceGrade,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_by: Option<String>,
}

/// A work item moving between lifecycle states.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StateChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<WorkItemState>,
    pub to: WorkItemState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<BlockedCause>,
}

/// A run ending, carrying its own head hash.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RunComplete {
    pub outcome: RunOutcome,
    pub head_hash: String,
    pub summary: String,
    pub grade: EvidenceGrade,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_hash: Option<String>,
}

/// A run asking for a person, rather than proceeding.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Attention {
    pub cause: String,
    pub summary: String,
    pub grade: EvidenceGrade,
}

/// A checkpoint finishing, with one slot result per slot decisions
/// 0012-0014 define.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CheckpointComplete {
    pub checkpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub result: CheckResult,
    pub checks: u32,
    /// Slot name to that slot's result, decisions 0012-0014: a `BTreeMap`
    /// so its JSON key order, and so its place in the replay hash, never
    /// depends on the order tasks happened to run in.
    pub slots: std::collections::BTreeMap<String, CheckResult>,
}

/// One reviewer's outcome for one lens: its own scores and findings.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReviewAnswer {
    pub lens: String,
    pub reviewer: String,
    pub family: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub result: String,
    pub scores: std::collections::BTreeMap<String, f64>,
    pub findings_kept: u32,
    pub findings_dropped: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub grade: String,
    pub round: u32,
}

/// The whole review's verdict, each lens's outcome and the weighted score.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReviewDecision {
    pub verdict: String,
    pub lenses: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    pub builder_families: Vec<String>,
    pub grade: String,
}

/// The parts of a new event a caller supplies; the journal fills in the
/// run, the previous hash and the hash it computes.
#[derive(Clone, Debug, PartialEq)]
pub struct EventDraft {
    pub actor: Actor,
    pub timestamp_ms: u64,
    pub cost: Option<Cost>,
    pub payload: Payload,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verification() -> Verification {
        Verification {
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
        }
    }

    fn attention() -> Payload {
        Payload::Attention(Attention {
            cause: "human".into(),
            summary: "needs a person".into(),
            grade: EvidenceGrade::Unverified,
        })
    }

    /// Serialises `payload`, checks its `event_type`, and round-trips it.
    fn round_trip(name: &str, payload: &Payload) {
        let json = serde_json::to_string(payload).expect("payload serialises");
        let value: serde_json::Value = serde_json::from_str(&json).expect("payload is JSON");
        assert_eq!(
            value.get("event_type").and_then(serde_json::Value::as_str),
            Some(name),
            "{json}"
        );
        let back: Payload = serde_json::from_str(&json).expect("payload round-trips");
        assert_eq!(&back, payload, "{json}");
    }

    /// The review-answer payload used by the round-trip test.
    fn review_answer() -> Payload {
        Payload::ReviewAnswer(ReviewAnswer {
            lens: "correctness".into(),
            reviewer: "reviewer-a".into(),
            family: "family-a".into(),
            model: Some("model-a".into()),
            result: "answered".into(),
            scores: std::collections::BTreeMap::from([
                ("accuracy".to_string(), 0.75),
                ("clarity".to_string(), 0.5),
            ]),
            findings_kept: 1,
            findings_dropped: 0,
            transcript: Some("runs/review-1/a.log".into()),
            reason: None,
            grade: "reported".into(),
            round: 1,
        })
    }

    /// The review-decision payload used by the round-trip test.
    fn review_decision() -> Payload {
        Payload::ReviewDecision(ReviewDecision {
            verdict: "pass".into(),
            lenses: vec![
                ("correctness".into(), "pass".into()),
                ("security".into(), "pass".into()),
            ],
            score: Some(0.75),
            threshold: Some(0.7),
            builder_families: vec!["family-b".into()],
            grade: "reported".into(),
        })
    }

    #[test]
    fn every_payload_round_trips_and_names_its_event_type() {
        let cases = [
            (
                "run-started",
                Payload::RunStarted(RunStarted {
                    title: Some("Ship the fix".into()),
                    repository: Some("github:open-software-factory/example".into()),
                    retry_of: None,
                    transcript: Some("runs/run-1/transcript.jsonl".into()),
                }),
            ),
            ("verification", Payload::Verification(verification())),
            (
                "review",
                Payload::Review(Review {
                    round: 1,
                    scope: "the change".into(),
                    findings: 2,
                    summary: "looked at the diff".into(),
                    grade: EvidenceGrade::Reported,
                }),
            ),
            (
                "finding",
                Payload::Finding(Finding {
                    rule: "example-rule".into(),
                    severity: Severity::Warning,
                    action: "fix".into(),
                    path: Some("src/lib.rs".into()),
                    line: Some(3),
                    column: None,
                    message: "something to fix".into(),
                    grade: EvidenceGrade::Observed,
                    verified_by: Some("osf:lint".into()),
                }),
            ),
            (
                "state-change",
                Payload::StateChange(StateChange {
                    from: Some(WorkItemState::Ready),
                    to: WorkItemState::InProgress,
                    cause: None,
                }),
            ),
            (
                "run-complete",
                Payload::RunComplete(RunComplete {
                    outcome: RunOutcome::Completed,
                    head_hash: "a".repeat(64),
                    summary: "all checks passed".into(),
                    grade: EvidenceGrade::Derived,
                    transcript_hash: Some("b".repeat(64)),
                }),
            ),
            ("attention", attention()),
            (
                "checkpoint-complete",
                Payload::CheckpointComplete(CheckpointComplete {
                    checkpoint: "pre-push".into(),
                    commit: Some("abc123".into()),
                    result: CheckResult::Passed,
                    checks: 3,
                    slots: std::collections::BTreeMap::new(),
                }),
            ),
            ("review-answer", review_answer()),
            ("review-decision", review_decision()),
        ];
        for (name, payload) in &cases {
            round_trip(name, payload);
        }
    }

    #[test]
    fn an_event_with_a_work_item_change_and_cost_round_trips() {
        let event = Event {
            schema_version: SCHEMA_VERSION,
            run: "run-1".into(),
            work_item: Some("github:open-software-factory/example#1".into()),
            change: Some(Change {
                branch: "main".into(),
                commit: "abc123".into(),
            }),
            actor: Actor::harness("demo-harness", "demo-model", "demo-family"),
            timestamp_ms: 7,
            cost: Some(Cost {
                usd_micros: Some(10),
                input_tokens: Some(20),
                output_tokens: Some(30),
            }),
            payload: attention(),
            prev_hash: "0".repeat(64),
            hash: "f".repeat(64),
        };
        let json = serde_json::to_string(&event).expect("event serialises");
        let back: Event = serde_json::from_str(&json).expect("event round-trips");
        assert_eq!(back, event);
    }

    #[test]
    fn optional_fields_are_omitted_when_none() {
        let payload = Payload::RunStarted(RunStarted {
            title: None,
            repository: None,
            retry_of: None,
            transcript: None,
        });
        let json = serde_json::to_string(&payload).expect("payload serialises");
        assert_eq!(json, r#"{"event_type":"run-started","payload":{}}"#);

        let event = Event {
            schema_version: SCHEMA_VERSION,
            run: "run-1".into(),
            work_item: None,
            change: None,
            actor: Actor::system("osf"),
            timestamp_ms: 1,
            cost: None,
            payload: attention(),
            prev_hash: "0".repeat(64),
            hash: "f".repeat(64),
        };
        let json = serde_json::to_string(&event).expect("event serialises");
        assert!(!json.contains("work_item"), "{json}");
        assert!(!json.contains("\"change\""), "{json}");
        assert!(!json.contains("\"cost\""), "{json}");
        assert!(!json.contains("model"), "{json}");
    }

    #[test]
    fn actor_kinds_serialise_to_their_lower_case_names() {
        let cases = [
            (Actor::system("osf"), "system"),
            (Actor::person("demo-person"), "person"),
            (
                Actor::harness("demo-harness", "demo-model", "demo-family"),
                "harness",
            ),
        ];
        for (actor, name) in cases {
            let value = serde_json::to_value(&actor).expect("actor serialises");
            assert_eq!(
                value.get("kind").and_then(serde_json::Value::as_str),
                Some(name)
            );
        }
    }

    #[test]
    fn evidence_grades_serialise_to_their_lower_case_names() {
        let cases = [
            (EvidenceGrade::Observed, "observed"),
            (EvidenceGrade::Derived, "derived"),
            (EvidenceGrade::Reported, "reported"),
            (EvidenceGrade::Unverified, "unverified"),
        ];
        for (grade, name) in cases {
            let json = serde_json::to_string(&grade).expect("grade serialises");
            assert_eq!(json, format!("\"{name}\""));
        }
    }

    #[test]
    fn work_item_states_and_blocked_causes_match_the_decision_names() {
        let states = [
            (WorkItemState::Ready, "ready"),
            (WorkItemState::InProgress, "in-progress"),
            (WorkItemState::Verifying, "verifying"),
            (WorkItemState::InReview, "in-review"),
            (WorkItemState::Deploying, "deploying"),
            (WorkItemState::Deployed, "deployed"),
            (WorkItemState::SignedOff, "signed-off"),
            (WorkItemState::Blocked, "blocked"),
            (WorkItemState::Failed, "failed"),
            (WorkItemState::Recovering, "recovering"),
            (WorkItemState::Aborted, "aborted"),
            (WorkItemState::RolledBack, "rolled-back"),
            (WorkItemState::Paused, "paused"),
        ];
        for (state, name) in states {
            let json = serde_json::to_string(&state).expect("state serialises");
            assert_eq!(json, format!("\"{name}\""));
        }
        let causes = [
            (BlockedCause::Dependency, "dependency"),
            (BlockedCause::Human, "human"),
            (BlockedCause::Clarification, "clarification"),
            (BlockedCause::Ambiguous, "ambiguous"),
            (BlockedCause::Capacity, "capacity"),
        ];
        for (cause, name) in causes {
            let json = serde_json::to_string(&cause).expect("cause serialises");
            assert_eq!(json, format!("\"{name}\""));
        }
    }
}
