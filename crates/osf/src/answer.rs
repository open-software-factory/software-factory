//! The review answer: the JSON Schema every reviewer answer must match, and
//! the validation the schema alone cannot express.
//!
//! A harness's final message parses as JSON directly, or, when a model
//! wraps it in prose, as the last fenced code block tagged `json` in it. A
//! JSON object embedded in unfenced prose, with no whole-message JSON and
//! no fence, is rejected rather than extracted: picking a brace-delimited
//! object out of free text risks matching a decoy a model prints while
//! reasoning about the answer, and the two shapes accepted here are the
//! ones a harness asked for a final JSON answer reliably produces.

use crate::lenses::Lens;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// The review answer schema, versioned with osf.
pub const SCHEMA: &str = include_str!("../schemas/review-answer.schema.json");

/// One reviewer's answer for one lens: a score for each criterion and the findings it raised.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Answer {
    pub lens: String,
    pub scores: BTreeMap<String, f64>,
    pub findings: Vec<AnswerFinding>,
}

/// One finding a reviewer raised: the file, the line and the exact quoted text task 5 checks against the real file.
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

/// The pattern that finds a fenced code block tagged `json`, compiled once for the process.
fn fenced_json_pattern() -> &'static regex::Regex {
    static CELL: OnceLock<regex::Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        regex::RegexBuilder::new(r"```json\s*\n(.*?)```")
            .dot_matches_new_line(true)
            .build()
            .expect("fenced json block pattern compiles")
    })
}

/// `raw` parsed whole as JSON, or, failing that, the last fenced `json` code block in it, parsed as JSON.
fn extract_json(raw: &str) -> Result<serde_json::Value, String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw.trim()) {
        return Ok(value);
    }
    let last_block = fenced_json_pattern()
        .captures_iter(raw)
        .last()
        .and_then(|captures| captures.get(1))
        .map(|m| m.as_str().trim().to_string());
    match last_block {
        Some(block) => serde_json::from_str(&block)
            .map_err(|e| format!("the fenced json block is not valid JSON: {e}")),
        None => {
            Err("the answer has no JSON object and no fenced json block to fall back to".into())
        }
    }
}

/// Parses `raw`, the final message of a reviewer harness, validates it
/// against [`SCHEMA`], and checks the two things the schema cannot: the
/// answer is for `lens`, and every one of the lens's criteria has a score.
///
/// # Errors
/// Names the reason when `raw` carries no JSON, the JSON does not match
/// the schema, the answer is for another lens, or a criterion has no score.
pub fn extract_and_validate(raw: &str, lens: &Lens) -> Result<Answer, String> {
    let value = extract_json(raw)?;

    validator()
        .validate(&value)
        .map_err(|error| format!("{}: {error}", error.instance_path))?;

    let answer: Answer = serde_json::from_value(value)
        .map_err(|e| format!("the answer matched its schema but not its shape: {e}"))?;

    if answer.lens != lens.name {
        return Err(format!(
            "the answer is for lens \"{}\", not \"{}\"",
            answer.lens, lens.name
        ));
    }

    for criterion in &lens.criteria {
        if !answer.scores.contains_key(&criterion.id) {
            return Err(format!(
                "the answer has no score for criterion \"{}\"",
                criterion.id
            ));
        }
    }

    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lenses::{Criterion, Runs, SeverityGuide, Trigger};

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
    fn the_last_fenced_json_block_is_taken_from_prose() {
        let a = extract_and_validate(
            include_str!("../tests/fixtures/review/fenced.md"),
            &test_lens(),
        )
        .expect("the second, complete fenced block is used");
        assert_eq!(a.scores.get("c2").copied(), Some(0.9));
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
    fn an_answer_for_another_lens_is_an_error() {
        assert!(extract_and_validate(
            include_str!("../tests/fixtures/review/wrong-lens.json"),
            &test_lens()
        )
        .is_err());
    }
}
