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

/// `value` checked against [`SCHEMA`], then against the two things the schema cannot express: it is for `lens`, and every one of the lens's criteria has a score.
fn validate_candidate(value: serde_json::Value, lens: &Lens) -> Result<Answer, String> {
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

    Ok(answer)
}

/// One fenced block's outcome: the reviewer's answer, or why this block is not it.
fn try_block(content: &str, lens: &Lens) -> Result<Answer, String> {
    match serde_json::from_str::<serde_json::Value>(content) {
        Ok(value @ serde_json::Value::Object(_)) => validate_candidate(value, lens),
        Ok(_) => Err("is not a JSON object".to_string()),
        Err(e) => Err(format!("is not valid JSON: {e}")),
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
}
