//! Validates one serialised journal event against the event schema that sits
//! beside the types that emit it, decision 0005: a reporter validates every
//! event and rejects an invalid one loudly.

use std::sync::OnceLock;

/// The draft 2020-12 schema for one event envelope, kept beside `event.rs`.
const SCHEMA: &str = include_str!("event.schema.json");

/// The schema compiled once for the life of the process.
static VALIDATOR: OnceLock<jsonschema::Validator> = OnceLock::new();

/// The compiled schema, built on first use.
fn validator() -> &'static jsonschema::Validator {
    VALIDATOR.get_or_init(|| {
        let schema: serde_json::Value =
            serde_json::from_str(SCHEMA).expect("the journal schema is valid JSON");
        jsonschema::validator_for(&schema).expect("the journal schema compiles")
    })
}

/// Checks `event` against the journal schema. On failure, returns one string
/// per problem, each naming the JSON path and the problem.
///
/// # Errors
/// Returns the list of reasons when `event` does not satisfy the schema.
pub fn validate(event: &serde_json::Value) -> Result<(), Vec<String>> {
    let reasons: Vec<String> = validator()
        .iter_errors(event)
        .map(|error| {
            let path = error.instance_path();
            let label = if path.is_empty() {
                "(root)"
            } else {
                path.as_str()
            };
            format!("{label}: {error}")
        })
        .collect();
    if reasons.is_empty() {
        Ok(())
    } else {
        Err(reasons)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-built valid event, never through the writer's types.
    fn valid_event() -> serde_json::Value {
        serde_json::json!({
            "schema_version": 1,
            "run": "run-1",
            "actor": { "kind": "system", "name": "osf" },
            "timestamp_ms": 7,
            "event_type": "attention",
            "payload": {
                "cause": "human",
                "summary": "needs a person",
                "grade": "unverified"
            },
            "prev_hash": "0".repeat(64),
            "hash": "f".repeat(64)
        })
    }

    /// Replaces `key` in `event`, which is always an object.
    fn set(event: &mut serde_json::Value, key: &str, value: serde_json::Value) {
        event
            .as_object_mut()
            .expect("event is an object")
            .insert(key.to_string(), value);
    }

    #[test]
    fn a_hand_built_valid_event_passes() {
        assert_eq!(validate(&valid_event()), Ok(()));
    }

    #[test]
    fn a_system_actor_validates() {
        let mut event = valid_event();
        set(
            &mut event,
            "actor",
            serde_json::json!({ "kind": "system", "name": "osf" }),
        );
        assert_eq!(validate(&event), Ok(()));
    }

    #[test]
    fn an_unknown_actor_kind_is_refused() {
        let mut event = valid_event();
        set(
            &mut event,
            "actor",
            serde_json::json!({ "kind": "robot", "name": "osf" }),
        );
        let reasons = validate(&event).expect_err("an unknown actor kind is refused");
        assert!(
            reasons
                .iter()
                .any(|r| r.contains("kind") || r.contains("actor")),
            "{reasons:?}"
        );
    }

    #[test]
    fn a_harness_actor_without_a_model_is_refused() {
        let mut event = valid_event();
        set(
            &mut event,
            "actor",
            serde_json::json!({ "kind": "harness", "name": "demo-harness" }),
        );
        let reasons = validate(&event).expect_err("a harness without a model is refused");
        assert!(
            reasons
                .iter()
                .any(|r| r.contains("model") || r.contains("actor")),
            "{reasons:?}"
        );
    }

    #[test]
    fn a_hand_built_schema_version_2_is_refused() {
        let mut event = valid_event();
        set(&mut event, "schema_version", serde_json::json!(2));
        let reasons = validate(&event).expect_err("version 2 is refused");
        assert!(
            reasons.iter().any(|r| r.contains("schema_version")),
            "{reasons:?}"
        );
    }

    #[test]
    fn an_extra_unknown_field_is_refused() {
        let mut event = valid_event();
        set(&mut event, "extra", serde_json::json!(true));
        let reasons = validate(&event).expect_err("an extra field is refused");
        assert!(reasons.iter().any(|r| r.contains("extra")), "{reasons:?}");
    }

    #[test]
    fn a_bad_hash_string_is_refused() {
        let mut event = valid_event();
        set(&mut event, "prev_hash", serde_json::json!("not-a-hash"));
        let reasons = validate(&event).expect_err("a bad hash is refused");
        assert!(
            reasons.iter().any(|r| r.contains("prev_hash")),
            "{reasons:?}"
        );
    }

    #[test]
    fn an_empty_free_text_field_is_accepted() {
        let mut event = valid_event();
        set(&mut event, "event_type", serde_json::json!("verification"));
        set(
            &mut event,
            "payload",
            serde_json::json!({
                "check": "scan",
                "checkpoint": "pre-commit",
                "result": "passed",
                "duration_ms": 12,
                "findings": 0,
                "grade": "observed",
                "reason": "",
                "summary": ""
            }),
        );
        assert_eq!(validate(&event), Ok(()));
    }

    #[test]
    fn an_empty_finding_message_is_accepted() {
        let mut event = valid_event();
        set(&mut event, "event_type", serde_json::json!("finding"));
        set(
            &mut event,
            "payload",
            serde_json::json!({
                "rule": "example-rule",
                "severity": "warning",
                "action": "fix",
                "message": "",
                "grade": "observed"
            }),
        );
        assert_eq!(validate(&event), Ok(()));
    }

    #[test]
    fn an_empty_identifier_is_still_refused() {
        let mut event = valid_event();
        set(&mut event, "event_type", serde_json::json!("verification"));
        set(
            &mut event,
            "payload",
            serde_json::json!({
                "check": "",
                "checkpoint": "pre-commit",
                "result": "passed",
                "duration_ms": 12,
                "findings": 0,
                "grade": "observed",
                "reason": "",
                "summary": ""
            }),
        );
        let reasons = validate(&event).expect_err("an empty check is refused");
        assert!(reasons.iter().any(|r| r.contains("check")), "{reasons:?}");
    }

    #[test]
    fn work_items_of_each_provider_shape_are_accepted() {
        for work_item in [
            "github:open-software-factory/example#1",
            "jira:PROJ-123",
            "linear:ENG-12",
            concat!("gitlab:group/sub/repo", "#4"),
        ] {
            let mut event = valid_event();
            set(&mut event, "work_item", serde_json::json!(work_item));
            assert_eq!(validate(&event), Ok(()), "{work_item}");
        }
    }

    #[test]
    fn a_work_item_without_a_provider_or_remainder_is_refused() {
        for work_item in [
            "x",
            "open-software-factory/example#1",
            ":abc",
            "github:",
            "github:has space",
            "github:tab\there",
            "github:trailing ",
            " github:leading",
            "GitHub:open-software-factory/example#1",
            "github:abc\n",
        ] {
            let mut event = valid_event();
            set(&mut event, "work_item", serde_json::json!(work_item));
            let reasons = validate(&event).expect_err(work_item);
            assert!(
                reasons.iter().any(|r| r.contains("work_item")),
                "{work_item}: {reasons:?}"
            );
        }
    }
}
