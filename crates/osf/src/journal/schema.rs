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
}
