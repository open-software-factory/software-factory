//! The review answer: the JSON Schema every reviewer answer must match, and
//! the validation the schema alone cannot express.
//!
//! A harness's final message parses as JSON directly first. Failing that,
//! every fenced code block in it is collected — backtick or tilde fences,
//! tagged `json`, tagged with anything else, or untagged — and tried from
//! last to first, keeping the first one that fully validates for the lens
//! asked. This favours a model's real, final answer over an earlier
//! example or a rejected draft it talked itself out of along the way. A
//! JSON object embedded in unfenced prose, with no whole-message JSON and
//! no fence at all, is rejected rather than extracted: picking a
//! brace-delimited object out of free text risks matching a decoy a model
//! prints while reasoning about the answer.

use crate::lenses::Lens;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The review answer schema, versioned with osf.
pub const SCHEMA: &str = include_str!("../schemas/review-answer.schema.json");

/// [`SCHEMA`] with `scores` fixed to one required property per criterion of `lens`, as compact JSON.
///
/// # Panics
/// Panics when the compiled-in [`SCHEMA`] is not valid JSON, a build-time mistake.
#[must_use]
pub fn schema_for(lens: &Lens) -> String {
    let mut schema: serde_json::Value =
        serde_json::from_str(SCHEMA).expect("the review answer schema is valid JSON");

    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for criterion in &lens.criteria {
        required.push(serde_json::Value::String(criterion.id.clone()));
        properties.insert(
            criterion.id.clone(),
            serde_json::json!({ "type": "number", "minimum": 0, "maximum": 1 }),
        );
    }
    let scores = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": required,
        "properties": properties,
    });

    schema
        .get_mut("properties")
        .and_then(serde_json::Value::as_object_mut)
        .expect("the review answer schema has a properties object")
        .insert("scores".to_string(), scores);
    // The identifiers stay in the file for editors. A harness validates the
    // schema it is handed, and some refuse one that names a meta-schema they
    // cannot resolve, so the identifiers never travel.
    if let Some(object) = schema.as_object_mut() {
        object.remove("$schema");
        object.remove("$id");
    }
    schema.to_string()
}

/// One reviewer's answer for one lens: a score for each criterion and the findings it raised.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Answer {
    pub lens: String,
    pub scores: BTreeMap<String, f64>,
    pub findings: Vec<AnswerFinding>,
}

/// One finding a reviewer raised, with the exact quoted text used to confirm it points at something real, not invented.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct AnswerFinding {
    pub path: String,
    pub line: u64,
    pub quote: String,
    pub severity: Severity,
    pub action: Action,
    pub body: String,
}

/// How serious a finding is, drawn from the lens's own severity guide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Blocker,
    Major,
    Minor,
}

/// What the reviewer asks the author to do about a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    MustFix,
    ShouldFix,
    MaybeFix,
    Justify,
    Defer,
    Dismiss,
}

/// The compiled answer schema, built once for the process.
fn validator() -> &'static jsonschema::Validator {
    static CELL: OnceLock<jsonschema::Validator> = OnceLock::new();
    CELL.get_or_init(|| {
        let schema: serde_json::Value =
            serde_json::from_str(SCHEMA).expect("the review answer schema is valid JSON");
        jsonschema::validator_for(&schema).expect("the review answer schema is a valid JSON Schema")
    })
}

/// The pattern that finds a backtick-fenced code block, any tag or none, compiled once for the process.
fn backtick_fence_pattern() -> &'static regex::Regex {
    static CELL: OnceLock<regex::Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        regex::RegexBuilder::new(r"```[^\n]*\n(.*?)```")
            .dot_matches_new_line(true)
            .build()
            .expect("backtick fence pattern compiles")
    })
}

/// The pattern that finds a tilde-fenced code block, any tag or none, compiled once for the process.
fn tilde_fence_pattern() -> &'static regex::Regex {
    static CELL: OnceLock<regex::Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        regex::RegexBuilder::new(r"~~~[^\n]*\n(.*?)~~~")
            .dot_matches_new_line(true)
            .build()
            .expect("tilde fence pattern compiles")
    })
}

/// Every fenced code block in `raw`, backtick or tilde, in the order each one starts.
fn fenced_blocks(raw: &str) -> Vec<String> {
    let mut found: Vec<(usize, String)> = Vec::new();
    for pattern in [backtick_fence_pattern(), tilde_fence_pattern()] {
        for captures in pattern.captures_iter(raw) {
            let Some(whole) = captures.get(0) else {
                continue;
            };
            let Some(content) = captures.get(1) else {
                continue;
            };
            found.push((whole.start(), content.as_str().trim().to_string()));
        }
    }
    found.sort_by_key(|(start, _)| *start);
    found.into_iter().map(|(_, content)| content).collect()
}

/// `pointer` with every segment that the shipped schema does not name
/// replaced by `<field>`. The pointer is walked down the schema: a segment
/// is kept only when it is a declared property, or an index into a position
/// the schema says is an array. A key the reviewer chose, such as one under
/// `scores`, is masked even when it is made of digits.
fn trusted_location(pointer: &str) -> String {
    static CELL: OnceLock<serde_json::Value> = OnceLock::new();
    let schema = CELL.get_or_init(|| {
        serde_json::from_str(SCHEMA).expect("the review answer schema is valid JSON")
    });
    let mut node = Some(schema);
    pointer
        .split('/')
        .map(|segment| {
            if segment.is_empty() {
                return segment.to_string();
            }
            let declared = node
                .and_then(|n| n.get("properties"))
                .and_then(|p| p.get(segment));
            let item = node
                .filter(|n| n.get("type").and_then(serde_json::Value::as_str) == Some("array"))
                .filter(|_| segment.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|n| n.get("items"));
            if let Some(next) = declared.or(item) {
                node = Some(next);
                segment.to_string()
            } else {
                node = node
                    .and_then(|n| n.get("additionalProperties"))
                    .filter(|extra| extra.is_object());
                "<field>".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// The instance path, with reviewer-chosen names hidden, plus a fixed, value-free description of why `error`
/// failed, safe to journal or print: [`jsonschema::ValidationError::masked`]
/// already replaces the failing value itself with a placeholder, but an
/// unexpected-field error also names the field the reviewer chose, which is
/// reviewer text too, so that case is reduced further to a bare category
/// with no names at all.
fn schema_error_reason(error: &jsonschema::ValidationError) -> String {
    use jsonschema::error::ValidationErrorKind as Kind;
    let detail = match &error.kind {
        Kind::AdditionalProperties { .. } | Kind::UnevaluatedProperties { .. } => {
            "an unexpected field".to_string()
        }
        _ => error.masked().to_string(),
    };
    format!(
        "the answer does not match its schema at \"{}\": {detail}",
        trusted_location(&error.instance_path.to_string())
    )
}

/// `value` checked against [`SCHEMA`], then against the two things the schema cannot express: it is for `lens`, and every one of the lens's criteria has a score.
fn validate_candidate(value: serde_json::Value, lens: &Lens) -> Result<Answer, String> {
    validator()
        .validate(&value)
        .map_err(|error| schema_error_reason(&error))?;

    let mut answer: Answer = serde_json::from_value(value)
        .map_err(|_| "the answer matched its schema but not its shape".to_string())?;

    if answer.lens != lens.name {
        return Err(format!(
            "the answer names another lens, not \"{}\"",
            lens.name
        ));
    }

    let missing: Vec<String> = lens
        .criteria
        .iter()
        .map(|criterion| criterion.id.as_str())
        .filter(|id| !answer.scores.contains_key(*id))
        .map(|id| format!("\"{id}\""))
        .collect();
    if !missing.is_empty() {
        let word = if missing.len() == 1 {
            "criterion"
        } else {
            "criteria"
        };
        return Err(format!(
            "the answer has no score for {word} {}",
            missing.join(", ")
        ));
    }

    // A score for a name that is no criterion of the lens is free text from the reviewer, and counts for nothing.
    answer
        .scores
        .retain(|id, _| lens.criteria.iter().any(|criterion| &criterion.id == id));
    Ok(answer)
}

/// `answer`, checked again against [`SCHEMA`] and `lens` the way a reviewer's
/// own run checks it: the same schema, the lens name, every criterion
/// scored. For an answer read back from a saved file, which nothing vouches for.
///
/// # Errors
/// Names the reason when the answer fails any of those checks.
pub fn revalidate(answer: &Answer, lens: &Lens) -> Result<Answer, String> {
    let value = serde_json::to_value(answer)
        .map_err(|_| "the saved answer cannot be read back".to_string())?;
    validate_candidate(value, lens)
}

/// One fenced block's outcome: the reviewer's answer, or why this block is not it.
fn try_block(content: &str, lens: &Lens) -> Result<Answer, String> {
    match serde_json::from_str::<serde_json::Value>(content) {
        Ok(value @ serde_json::Value::Object(_)) => validate_candidate(value, lens),
        Ok(_) => Err("is not a JSON object".to_string()),
        Err(e) => Err(format!(
            "is not valid JSON at line {} column {}",
            e.line(),
            e.column()
        )),
    }
}

/// Parses `raw`, the final message of a reviewer harness, validates it
/// against [`SCHEMA`], and checks the two things the schema cannot: the
/// answer is for `lens`, and every one of the lens's criteria has a score.
///
/// The whole message is tried as JSON first. Failing that, every fenced
/// code block in it — backtick or tilde, any tag or none — is tried from
/// last to first, keeping the first one that fully validates.
///
/// # Errors
/// Names the reason when `raw` carries no JSON and no fenced code block,
/// or when none of the candidates it holds validates: the JSON does not
/// match the schema, the answer is for another lens, or a criterion has
/// no score.
pub fn extract_and_validate(raw: &str, lens: &Lens) -> Result<Answer, String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw.trim()) {
        return validate_candidate(value, lens);
    }

    let blocks = fenced_blocks(raw);
    if blocks.is_empty() {
        return Err(
            "the answer has no JSON object and no fenced code block to fall back to".to_string(),
        );
    }

    let mut reasons_last_to_first: Vec<String> = Vec::with_capacity(blocks.len());
    for content in blocks.iter().rev() {
        match try_block(content, lens) {
            Ok(answer) => return Ok(answer),
            Err(reason) => reasons_last_to_first.push(reason),
        }
    }
    reasons_last_to_first.reverse();

    let numbered: Vec<String> = reasons_last_to_first
        .into_iter()
        .enumerate()
        .map(|(i, reason)| format!("block {}: {reason}", i + 1))
        .collect();
    let count = blocks.len();
    let plural = if count == 1 { "" } else { "s" };
    Err(format!(
        "none of the {count} fenced code block{plural} in the answer validate for lens \"{}\": {}",
        lens.name,
        numbered.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lenses::{Criterion, Runs, SeverityGuide, Trigger};
    use crate::test_support::TempDir;

    fn test_lens() -> Lens {
        Lens {
            name: "correctness".to_string(),
            summary: "does the change do what it claims to do".to_string(),
            criteria: vec![
                Criterion {
                    id: "c1".to_string(),
                    question: "does the change do what it claims".to_string(),
                },
                Criterion {
                    id: "c2".to_string(),
                    question: "does it handle its error paths".to_string(),
                },
            ],
            severity_guide: SeverityGuide {
                blocker: "the change loses data or breaks the build".to_string(),
                major: "a wrong behaviour a user can hit".to_string(),
                minor: "a small correctness nit".to_string(),
            },
            weight: 1.0,
            runs: Runs::Always,
            trigger: Trigger::default(),
            context: Vec::new(),
        }
    }

    /// The real shipped `correctness` lens, loaded the same way `osf review run` would.
    fn shipped_correctness_lens() -> Lens {
        let root = TempDir::new("osf-answer-shipped-correctness");
        std::fs::create_dir_all(root.join(".osf/review-lenses")).expect("dirs");
        let catalogue = crate::lenses::load(&root, None).expect("loads");
        catalogue
            .lenses
            .into_iter()
            .find(|l| l.name == "correctness")
            .expect("correctness lens is shipped")
    }

    /// Every shipped lens, loaded the same way `osf review run` would.
    fn shipped_lenses() -> Vec<Lens> {
        let root = TempDir::new("osf-answer-shipped-all");
        std::fs::create_dir_all(root.join(".osf/review-lenses")).expect("dirs");
        crate::lenses::load(&root, None).expect("loads").lenses
    }

    /// Every keyword the structured-output service accepts.
    const ALLOWED_SCHEMA_KEYWORDS: &[&str] = &[
        "title",
        "description",
        "type",
        "properties",
        "required",
        "additionalProperties",
        "items",
        "enum",
        "minimum",
        "maximum",
        "minLength",
    ];

    /// Walks every sub-schema of `schema` and asserts the structured-output rules.
    fn assert_structured_output_rules(schema: &str) {
        fn walk(value: &serde_json::Value, path: &str) {
            let Some(object) = value.as_object() else {
                return;
            };
            for (key, child) in object {
                assert!(
                    ALLOWED_SCHEMA_KEYWORDS.contains(&key.as_str()),
                    "the schema at \"{path}\" uses the unsupported keyword \"{key}\""
                );
                match key.as_str() {
                    "properties" => {
                        if let Some(properties) = child.as_object() {
                            for (name, subschema) in properties {
                                walk(subschema, &format!("{path}.properties.{name}"));
                            }
                        }
                    }
                    "items" => walk(child, &format!("{path}.items")),
                    "additionalProperties" if child.is_object() => {
                        walk(child, &format!("{path}.additionalProperties"));
                    }
                    _ => {}
                }
            }
            if object.get("type").and_then(serde_json::Value::as_str) != Some("object") {
                return;
            }
            assert_eq!(
                object.get("additionalProperties"),
                Some(&serde_json::Value::Bool(false)),
                "the object schema at \"{path}\" must set additionalProperties to false"
            );
            let properties = object
                .get("properties")
                .and_then(serde_json::Value::as_object)
                .unwrap_or_else(|| panic!("the object schema at \"{path}\" has no properties"));
            let required = object
                .get("required")
                .and_then(serde_json::Value::as_array)
                .unwrap_or_else(|| panic!("the object schema at \"{path}\" has no required"));
            let mut required_names: Vec<&str> = required
                .iter()
                .map(|entry| entry.as_str().expect("a required entry is a string"))
                .collect();
            let mut property_names: Vec<&str> = properties.keys().map(String::as_str).collect();
            required_names.sort_unstable();
            property_names.sort_unstable();
            assert_eq!(
                required_names, property_names,
                "the object schema at \"{path}\" must require exactly the keys of \"properties\""
            );
        }
        walk(
            &serde_json::from_str(schema).expect("the schema is valid JSON"),
            "$",
        );
    }

    #[test]
    fn the_schema_sent_to_a_reviewer_names_no_meta_schema_or_id_but_the_file_keeps_them() {
        let file: serde_json::Value = serde_json::from_str(SCHEMA).expect("the file is JSON");
        assert!(file.get("$schema").is_some() && file.get("$id").is_some());
        let sent: serde_json::Value =
            serde_json::from_str(&schema_for(&test_lens())).expect("the schema is valid JSON");
        assert!(sent.get("$schema").is_none(), "{sent}");
        assert!(sent.get("$id").is_none(), "{sent}");
        let validator = jsonschema::validator_for(&sent).expect("the sent schema compiles");
        let answer = serde_json::json!({
            "lens": "correctness",
            "scores": {"c1": 1, "c2": 0.5},
            "findings": []
        });
        assert!(validator.is_valid(&answer));
        assert!(!validator.is_valid(&serde_json::json!({"lens": 1})));
    }

    #[test]
    fn the_schema_sent_to_a_reviewer_follows_the_structured_output_rules() {
        assert_structured_output_rules(&schema_for(&test_lens()));
        for lens in shipped_lenses() {
            assert_structured_output_rules(&schema_for(&lens));
        }
    }

    #[test]
    fn an_invalid_answer_reason_never_repeats_a_name_the_reviewer_chose() {
        let lens = test_lens();
        let raw = r#"{"lens":"correctness","scores":{"SECRET_VALUE":"invalid"},"findings":[]}"#;
        let reason = extract_and_validate(raw, &lens).expect_err("the answer is invalid");
        assert!(!reason.contains("SECRET_VALUE"), "{reason}");
        assert!(reason.contains("/scores/<field>"), "{reason}");
    }

    #[test]
    fn a_numeric_score_key_is_masked_and_an_array_index_is_kept() {
        assert_eq!(
            trusted_location("/scores/12345678901234567890"),
            "/scores/<field>"
        );
        assert_eq!(trusted_location("/findings/3/path"), "/findings/3/path");
        assert_eq!(
            trusted_location("/findings/x/9"),
            "/findings/<field>/<field>"
        );
    }

    #[test]
    fn the_scores_object_lists_each_criterion_of_the_lens() {
        let schema: serde_json::Value =
            serde_json::from_str(&schema_for(&test_lens())).expect("the schema is valid JSON");
        let scores = schema
            .get("properties")
            .and_then(|properties| properties.get("scores"))
            .expect("the schema has properties.scores");
        let required: Vec<&str> = scores
            .get("required")
            .and_then(serde_json::Value::as_array)
            .expect("scores.required")
            .iter()
            .map(|id| id.as_str().expect("a criterion id"))
            .collect();
        assert_eq!(required, vec!["c1", "c2"]);
        let properties = scores
            .get("properties")
            .and_then(serde_json::Value::as_object)
            .expect("scores.properties");
        assert_eq!(properties.len(), 2, "{properties:?}");
        assert!(properties.contains_key("c1"), "{properties:?}");
        assert!(properties.contains_key("c2"), "{properties:?}");
    }

    #[test]
    fn a_real_codex_answer_validates_against_the_lens_schema_and_the_reducer_checks() {
        let lens = shipped_correctness_lens();
        let raw = include_str!("../tests/fixtures/review/codex-answer.json");
        extract_and_validate(raw, &lens).expect("the real answer passes the reducer's checks");
        let schema: serde_json::Value =
            serde_json::from_str(&schema_for(&lens)).expect("the schema is valid JSON");
        let validator =
            jsonschema::validator_for(&schema).expect("the lens schema is a valid JSON Schema");
        let answer: serde_json::Value =
            serde_json::from_str(raw).expect("the real answer is valid JSON");
        assert!(
            validator.validate(&answer).is_ok(),
            "the real answer must validate against the schema sent to codex"
        );
    }

    #[test]
    fn a_valid_answer_parses() {
        let lens = test_lens();
        let a = extract_and_validate(include_str!("../tests/fixtures/review/valid.json"), &lens)
            .expect("valid");
        assert_eq!(a.lens, lens.name);
        assert_eq!(a.findings.len(), 1);
        let finding = a.findings.first().expect("one finding");
        assert_eq!(finding.severity, Severity::Minor);
        assert_eq!(finding.action, Action::ShouldFix);
    }

    #[test]
    fn prose_with_no_json_is_an_error() {
        assert!(extract_and_validate(
            include_str!("../tests/fixtures/review/prose-only.txt"),
            &test_lens()
        )
        .is_err());
    }

    #[test]
    fn a_json_fence_after_reasoning_is_taken_over_an_earlier_rejected_draft() {
        let a = extract_and_validate(
            include_str!("../tests/fixtures/review/fenced.md"),
            &test_lens(),
        )
        .expect("the second, complete fenced block is used");
        assert_eq!(a.scores.get("c2").copied(), Some(0.9));
    }

    #[test]
    fn an_untagged_fence_is_read() {
        let a = extract_and_validate(
            include_str!("../tests/fixtures/review/fenced-no-tag.md"),
            &test_lens(),
        )
        .expect("an untagged fence still holds a JSON object");
        assert_eq!(a.scores.get("c1").copied(), Some(0.7));
    }

    #[test]
    fn a_tilde_fence_is_read() {
        let a = extract_and_validate(
            include_str!("../tests/fixtures/review/fenced-tilde.md"),
            &test_lens(),
        )
        .expect("a tilde fence is read the same as a backtick fence");
        assert_eq!(a.scores.get("c1").copied(), Some(0.85));
    }

    #[test]
    fn a_trailing_comma_in_the_only_fence_is_rejected_and_named() {
        let e = extract_and_validate(
            include_str!("../tests/fixtures/review/fenced-trailing-comma.md"),
            &test_lens(),
        )
        .expect_err("a trailing comma is not valid JSON");
        assert!(e.contains('1'), "{e}");
        assert!(e.contains("valid JSON"), "{e}");
    }

    #[test]
    fn a_real_answer_followed_by_an_example_block_uses_the_real_answer() {
        let a = extract_and_validate(
            include_str!("../tests/fixtures/review/fenced-example-after.md"),
            &test_lens(),
        )
        .expect("the first block, not the trailing example, validates for this lens");
        assert_eq!(a.scores.get("c1").copied(), Some(0.72));
    }

    #[test]
    fn json_wrapped_in_prose_without_a_fence_is_rejected() {
        let e = extract_and_validate(
            include_str!("../tests/fixtures/review/json-wrapped-in-prose.txt"),
            &test_lens(),
        )
        .expect_err("no fence and not whole-message JSON, so it is rejected, not extracted");
        assert!(e.contains("JSON"), "{e}");
    }

    #[test]
    fn a_score_out_of_range_breaks_the_schema() {
        let e = extract_and_validate(
            include_str!("../tests/fixtures/review/score-out-of-range.json"),
            &test_lens(),
        )
        .expect_err("invalid");
        assert!(e.contains("score") || e.contains("maximum"), "{e}");
    }

    #[test]
    fn a_missing_criterion_score_is_an_error_naming_it() {
        let e = extract_and_validate(
            include_str!("../tests/fixtures/review/missing-criterion.json"),
            &test_lens(),
        )
        .expect_err("invalid");
        assert!(e.contains("criterion"), "{e}");
        assert!(e.contains("c2"), "{e}");
    }

    #[test]
    fn every_missing_criterion_is_named_in_one_error() {
        let lens = shipped_correctness_lens();
        let e = extract_and_validate(
            include_str!("../tests/fixtures/review/missing-two-criteria.json"),
            &lens,
        )
        .expect_err("both of the shipped lens's criteria are missing");
        assert!(e.contains("logic"), "{e}");
        assert!(e.contains("error-handling"), "{e}");
    }

    #[test]
    fn an_answer_for_another_lens_is_an_error() {
        assert!(extract_and_validate(
            include_str!("../tests/fixtures/review/wrong-lens.json"),
            &test_lens()
        )
        .is_err());
    }

    fn saved_answer() -> Answer {
        extract_and_validate(
            include_str!("../tests/fixtures/review/valid.json"),
            &test_lens(),
        )
        .expect("valid")
    }

    #[test]
    fn a_saved_answer_that_validated_validates_again() {
        let again = revalidate(&saved_answer(), &test_lens()).expect("still valid");
        assert_eq!(again, saved_answer());
    }

    #[test]
    fn a_saved_answer_with_a_score_outside_zero_to_one_is_rejected() {
        for bad in [1000.0, -0.5, f64::NAN] {
            let mut answer = saved_answer();
            answer.scores.insert("c1".to_string(), bad);
            let e = revalidate(&answer, &test_lens()).expect_err("rejected");
            assert!(e.contains("scores"), "{bad}: {e}");
        }
    }

    #[test]
    fn a_saved_answer_for_another_lens_or_missing_a_criterion_is_rejected() {
        let mut other = saved_answer();
        other.lens = "security".to_string();
        let e = revalidate(&other, &test_lens()).expect_err("another lens");
        assert!(e.contains("another lens"), "{e}");
        let mut short = saved_answer();
        short.scores.remove("c2");
        let e = revalidate(&short, &test_lens()).expect_err("missing criterion");
        assert!(e.contains("c2"), "{e}");
    }

    #[test]
    fn a_score_for_a_name_that_is_no_criterion_is_dropped() {
        let mut answer = saved_answer();
        answer
            .scores
            .insert("free-text-from-a-reviewer".to_string(), 0.5);
        let kept = revalidate(&answer, &test_lens()).expect("valid");
        assert!(!kept.scores.contains_key("free-text-from-a-reviewer"));
        assert_eq!(kept.scores.len(), 2);
    }
}
