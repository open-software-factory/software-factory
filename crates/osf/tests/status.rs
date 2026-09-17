//! Behavioral cases for `osf status`: rendering the block from named
//! sources, and applying it between markers in a pull request description.
//! Every case here is offline: nothing is read from or written to GitHub,
//! except the two cases that exercise the `gh` composition through a fake
//! client.

use osf::status::{self, GhClient, RenderInput, StatusError};
use std::cell::RefCell;

const TIER_JSON: &str =
    r#"{"tier":"normal","reasons":["3 files, 120 lines, no high-blast-radius path"]}"#;

fn review_json(review_decision: &str, reviews: &str, comments: &str) -> String {
    format!(
        r#"{{"reviewDecision":"{review_decision}","reviews":[{reviews}],"comments":[{comments}]}}"#
    )
}

fn advisory_two_rounds_review() -> String {
    review_json(
        "",
        r#"{"state":"COMMENTED","body":"**Verdict (advisory): REQUEST_CHANGES** — posted as a comment.\n\nRound 1 summary."},
           {"state":"COMMENTED","body":"**Verdict (advisory): APPROVE** — posted as a comment.\n\nRound 2 summary."}"#,
        r#"{"body":"Review round 1 (normal): 6 findings, 5 fixed, 1 justified, 0 deferred.\n\nDetail."},
           {"body":"Some other comment."}"#,
    )
}

fn base_input<'a>(gates: &'a str, review_json: &'a str) -> RenderInput<'a> {
    RenderInput {
        tier_json: TIER_JSON,
        gates,
        problem: "The problem.",
        approach: "The approach.",
        review_json,
    }
}

fn render_ok(gates: &str, review_json: &str) -> String {
    status::render(&base_input(gates, review_json)).expect("render succeeds")
}

fn row(block: &str, label: &str) -> String {
    block
        .lines()
        .find(|line| line.starts_with(&format!("| **{label}**")))
        .unwrap_or_else(|| panic!("no {label} row in:\n{block}"))
        .to_string()
}

#[test]
fn ready_is_yes_when_every_gate_passed_and_the_verdict_is_approve() {
    let review = advisory_two_rounds_review();
    let block = render_ok("build: passed, tests (412): passed", &review);
    assert_eq!(row(&block, "Ready"), "| **Ready** | yes |");
    assert_eq!(row(&block, "Verified"), "| **Verified** | all 2 passed |");
    assert!(row(&block, "Risk").contains("normal: 3 files, 120 lines"));
    assert_eq!(
        row(&block, "Review"),
        "| **Review** | APPROVE (advisory): 1 round, 6 findings, 5 fixed, 1 justified, 0 deferred |"
    );
    assert!(block.contains("**Problem**: The problem."));
    assert!(block.contains("**Approach**: The approach."));
    assert_eq!(block.lines().next(), Some("<!-- factory:status:begin -->"));
    assert_eq!(block.lines().last(), Some("<!-- factory:status:end -->"));
}

#[test]
fn a_failed_gate_blocks_and_keeps_its_reason_while_trimming_names() {
    let review = advisory_two_rounds_review();
    let block = render_ok("build (api) : passed, lint: failed: 3 warnings", &review);
    assert_eq!(
        row(&block, "Ready"),
        "| **Ready** | blocked by lint failed |"
    );
    assert_eq!(
        row(&block, "Verified"),
        "| **Verified** | 1 of 2 passed, failed: lint (3 warnings) |"
    );
}

#[test]
fn a_native_changes_requested_decision_blocks_with_no_advisory_mark() {
    let review = review_json("CHANGES_REQUESTED", "", "");
    let block = render_ok("build: passed", &review);
    assert_eq!(
        row(&block, "Ready"),
        "| **Ready** | blocked by review: REQUEST_CHANGES |"
    );
    assert_eq!(
        row(&block, "Review"),
        "| **Review** | REQUEST_CHANGES: no round yet |"
    );
}

#[test]
fn a_native_approved_decision_is_used_as_is() {
    let review = review_json("APPROVED", "", "");
    let block = render_ok("build: passed", &review);
    assert_eq!(row(&block, "Ready"), "| **Ready** | yes |");
    assert_eq!(
        row(&block, "Review"),
        "| **Review** | APPROVE: no round yet |"
    );
}

#[test]
fn review_required_blocks_even_with_an_advisory_approve() {
    let review = review_json(
        "REVIEW_REQUIRED",
        r#"{"state":"COMMENTED","body":"**Verdict (advisory): APPROVE** — posted as a comment.\n\nBranch protection still requires a person's approving review."}"#,
        "",
    );
    let block = render_ok("build: passed", &review);
    assert_eq!(
        row(&block, "Ready"),
        "| **Ready** | blocked by review: human approval required |"
    );
    assert_eq!(
        row(&block, "Review"),
        "| **Review** | APPROVE (advisory): no round yet; human approval required |"
    );
}

#[test]
fn a_tier_json_missing_the_required_fields_is_refused() {
    let review = advisory_two_rounds_review();
    let input = RenderInput {
        tier_json: "{}",
        ..base_input("build: passed", &review)
    };
    let err = status::render(&input).expect_err("empty tier object is refused");
    assert!(format!("{err}").contains("tier"), "{err}");
}

#[test]
fn a_tier_json_with_a_non_string_reason_is_refused() {
    let review = advisory_two_rounds_review();
    let input = RenderInput {
        tier_json: r#"{"tier":"normal","reasons":[{}]}"#,
        ..base_input("build: passed", &review)
    };
    let err = status::render(&input).expect_err("a non-string reason is refused");
    assert!(format!("{err}").contains("reasons"), "{err}");
}

#[test]
fn review_json_that_is_not_json_at_all_is_refused() {
    let input = RenderInput {
        tier_json: TIER_JSON,
        gates: "build: passed",
        problem: "p",
        approach: "a",
        review_json: "",
    };
    assert!(status::render(&input).is_err());
}

#[test]
fn review_json_with_a_non_object_review_is_refused() {
    let input = RenderInput {
        tier_json: TIER_JSON,
        gates: "build: passed",
        problem: "p",
        approach: "a",
        review_json: r#"{"reviews":[1]}"#,
    };
    let err = status::render(&input).expect_err("a non-object review is refused");
    assert!(format!("{err}").contains("shape"), "{err}");
}

#[test]
fn a_comma_inside_a_gate_name_is_refused_naming_the_cause() {
    let review = advisory_two_rounds_review();
    let input = base_input("tests (35, both paths): passed", &review);
    let err = status::render(&input).expect_err("a comma inside a gate name is refused");
    assert!(
        format!("{err}").contains("a comma inside a gate name"),
        "{err}"
    );
}

#[test]
fn a_gate_result_other_than_passed_or_failed_is_refused() {
    let review = advisory_two_rounds_review();
    let input = base_input("build: ok", &review);
    let err = status::render(&input).expect_err("an unknown gate result is refused");
    assert!(
        format!("{err}").contains("must end ': passed' or ': failed"),
        "{err}"
    );
}

#[test]
fn render_gives_byte_identical_output_on_the_same_inputs() {
    let review = advisory_two_rounds_review();
    let input = base_input("build: passed, tests (412): passed", &review);
    let first = status::render(&input).expect("first render succeeds");
    let second = status::render(&input).expect("second render succeeds");
    assert_eq!(first.as_bytes(), second.as_bytes());
}

const BODY_WITH_BLOCK: &str = "<!-- factory:status:begin -->\n| | |\n|---|---|\n| **Ready** | blocked by review: pending |\n| **Risk** | low \u{2014} old reasons |\n<!-- factory:status:end -->\n\n## What changed and why\n\nBody text that must not change.\n\n## Issue\n\nCloses open-software-factory/software-factory#1, the thing.\n";

const BODY_WITHOUT_BLOCK: &str =
    "## What changed and why\n\nA body written before the status block existed.\n";

fn new_block() -> String {
    let review = advisory_two_rounds_review();
    render_ok("build: passed, tests (412): passed", &review)
}

fn strip_block(text: &str) -> String {
    let mut skipping = false;
    text.lines()
        .filter(|line| {
            if *line == "<!-- factory:status:begin -->" {
                skipping = true;
                return false;
            }
            if *line == "<!-- factory:status:end -->" {
                skipping = false;
                return false;
            }
            !skipping
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn apply_replaces_the_existing_block_and_changes_nothing_else() {
    let block = new_block();
    let out = status::apply(BODY_WITH_BLOCK, &block).expect("apply succeeds");
    assert_eq!(strip_block(BODY_WITH_BLOCK), strip_block(&out));
    assert!(out.contains("| **Ready** | yes |"));
    assert!(!out.contains("old reasons"));
    assert!(out.contains("Body text that must not change."));
}

#[test]
fn apply_is_idempotent() {
    let block = new_block();
    let once = status::apply(BODY_WITH_BLOCK, &block).expect("first apply succeeds");
    let twice = status::apply(&once, &block).expect("second apply succeeds");
    assert_eq!(once, twice);
}

#[test]
fn apply_inserts_the_block_at_the_top_when_markers_are_absent() {
    let block = new_block();
    let out = status::apply(BODY_WITHOUT_BLOCK, &block).expect("apply succeeds");
    assert_eq!(out.lines().next(), Some("<!-- factory:status:begin -->"));
    assert_eq!(
        strip_block(&out),
        format!("\n{}", BODY_WITHOUT_BLOCK.trim_end_matches('\n'))
    );
}

#[test]
fn apply_keeps_three_trailing_newlines_exactly() {
    let block = new_block();
    let body = format!("{BODY_WITH_BLOCK}\n\n");
    let out = status::apply(&body, &block).expect("apply succeeds");
    assert!(out.ends_with("\n\n\n"), "{:?}", &out[out.len() - 6..]);
    assert!(!out.ends_with("\n\n\n\n"));
}

#[test]
fn apply_adds_no_final_newline_when_the_body_has_none() {
    let block = new_block();
    let body = BODY_WITHOUT_BLOCK.trim_end_matches('\n');
    let out = status::apply(body, &block).expect("apply succeeds");
    assert!(!out.ends_with('\n'), "{:?}", &out[out.len() - 6..]);
}

#[test]
fn apply_keeps_a_long_run_of_trailing_newlines_and_stays_fast() {
    let block = new_block();
    let long_trail = "\n".repeat(20_000);
    let body = format!("{BODY_WITH_BLOCK}{long_trail}");
    let started = std::time::Instant::now();
    let out = status::apply(&body, &block).expect("apply succeeds");
    assert!(started.elapsed().as_secs() < 5, "apply must finish quickly");
    assert!(out.ends_with(&long_trail));
}

#[test]
fn a_body_with_a_begin_marker_and_no_end_marker_is_refused() {
    let block = new_block();
    let body = "<!-- factory:status:begin -->\ncell\n";
    let err = status::apply(body, &block).expect_err("begin without end is refused");
    assert!(
        format!("{err}").contains("1 begin marker(s) and 0 end marker(s)"),
        "{err}"
    );
}

#[test]
fn a_marker_with_trailing_space_is_not_a_marker() {
    let block = new_block();
    let body = BODY_WITH_BLOCK.replace(
        "<!-- factory:status:begin -->\n",
        "<!-- factory:status:begin --> \n",
    );
    let err = status::apply(&body, &block).expect_err("a marker with trailing space is refused");
    assert!(
        format!("{err}").contains("0 begin marker(s) and 1 end marker(s)"),
        "{err}"
    );
}

#[test]
fn a_block_file_with_an_inline_marker_is_refused() {
    let bad_block = "x <!-- factory:status:begin --> y\n<!-- factory:status:end -->\n";
    let err = status::apply(BODY_WITHOUT_BLOCK, bad_block)
        .expect_err("an inline marker does not count as a marker line");
    assert!(
        format!("{err}").contains("each marker exactly once"),
        "{err}"
    );
}

#[test]
fn a_block_file_with_reversed_markers_is_refused() {
    let bad_block = "<!-- factory:status:end -->\ncell\n<!-- factory:status:begin -->\n";
    let err = status::apply(BODY_WITHOUT_BLOCK, bad_block)
        .expect_err("reversed markers in the block are refused");
    assert!(
        format!("{err}").contains("end marker comes before its begin marker"),
        "{err}"
    );
}

#[test]
fn a_body_with_reversed_markers_is_refused() {
    let block = new_block();
    let body = "<!-- factory:status:end -->\ncell\n<!-- factory:status:begin -->\n";
    let err = status::apply(body, &block).expect_err("reversed markers in the body are refused");
    assert!(
        format!("{err}").contains("end marker comes before its begin marker"),
        "{err}"
    );
}

/// A CRLF-authored description: the inserted block, the replaced span, and
/// the untouched surrounding text all come back with `\r\n` throughout, and
/// the trailing CRLF run is kept exactly.
#[test]
fn apply_on_a_crlf_body_keeps_crlf_throughout_and_preserves_the_trailing_run() {
    let block = new_block();
    let crlf_body = BODY_WITH_BLOCK.replace('\n', "\r\n") + "\r\n";
    let out = status::apply(&crlf_body, &block).expect("apply succeeds on a CRLF body");
    assert!(
        !out.contains("\r\r"),
        "no doubled carriage returns: {out:?}"
    );
    for line in out.lines() {
        assert!(
            !line.ends_with('\r'),
            "a lone \\r would mean a bare \\n crept in: {line:?}"
        );
    }
    assert!(out.ends_with("\r\n\r\n"), "{:?}", &out[out.len() - 8..]);
    assert!(out.contains("Ready** | yes"));
    assert!(out.contains("Body text that must not change."));
    assert!(!out.contains("old reasons"));
}

#[test]
fn apply_on_a_crlf_body_with_no_existing_block_inserts_a_crlf_block() {
    let block = new_block();
    let crlf_body = BODY_WITHOUT_BLOCK.replace('\n', "\r\n");
    let out = status::apply(&crlf_body, &block).expect("apply succeeds");
    let first_line_end = out.find('\n').expect("at least one line");
    assert_eq!(&out[..=first_line_end], "<!-- factory:status:begin -->\r\n");
    assert!(out.contains("A body written before the status block existed.\r\n"));
}

/// A fake [`GhClient`] that records every call, so composition can be
/// checked without a real `gh` process.
struct FakeGh {
    body: String,
    view_fails: bool,
    calls: RefCell<Vec<String>>,
    edited: RefCell<Option<String>>,
}

impl FakeGh {
    fn new(body: &str) -> Self {
        FakeGh {
            body: body.to_string(),
            view_fails: false,
            calls: RefCell::new(Vec::new()),
            edited: RefCell::new(None),
        }
    }

    fn failing_view() -> Self {
        FakeGh {
            body: String::new(),
            view_fails: true,
            calls: RefCell::new(Vec::new()),
            edited: RefCell::new(None),
        }
    }
}

impl GhClient for FakeGh {
    fn view_body(&self, repo: &str, pr: &str) -> Result<String, StatusError> {
        self.calls.borrow_mut().push(format!("view {repo}#{pr}"));
        if self.view_fails {
            return Err(StatusError::new(
                "gh pr view failed for open-software-factory/software-factory#1; nothing was changed",
            ));
        }
        Ok(self.body.clone())
    }

    fn view_review(&self, _repo: &str, _pr: &str) -> Result<String, StatusError> {
        unreachable!("apply never asks for review data")
    }

    fn view_pr_info(&self, _repo: &str, _pr: &str) -> Result<String, StatusError> {
        unreachable!("apply never asks for the pull request's metadata")
    }

    fn view_checks(&self, _repo: &str, _pr: &str) -> Result<String, StatusError> {
        unreachable!("apply never asks for checks")
    }

    fn edit_body(&self, repo: &str, pr: &str, body: &str) -> Result<(), StatusError> {
        self.calls.borrow_mut().push(format!("edit {repo}#{pr}"));
        *self.edited.borrow_mut() = Some(body.to_string());
        Ok(())
    }
}

#[test]
fn a_failed_gh_pr_view_stops_apply_before_gh_pr_edit() {
    let client = FakeGh::failing_view();
    let result = status::apply_via_gh(
        &client,
        "open-software-factory/software-factory",
        "1",
        &new_block(),
    );
    assert!(result.is_err());
    let calls = client.calls.borrow();
    assert!(calls.iter().any(|c| c.starts_with("view")));
    assert!(!calls.iter().any(|c| c.starts_with("edit")));
}

#[test]
fn a_working_gh_pr_view_leads_to_one_gh_pr_edit_with_the_block_on_top() {
    let client = FakeGh::new(BODY_WITHOUT_BLOCK);
    let new_body = status::apply_via_gh(
        &client,
        "open-software-factory/software-factory",
        "1",
        &new_block(),
    )
    .expect("apply succeeds");
    let calls = client.calls.borrow();
    assert_eq!(
        calls.as_slice(),
        [
            "view open-software-factory/software-factory#1",
            "edit open-software-factory/software-factory#1"
        ]
    );
    assert_eq!(
        new_body.lines().next(),
        Some("<!-- factory:status:begin -->")
    );
    assert_eq!(client.edited.borrow().as_deref(), Some(new_body.as_str()));
}

const CHECKS_JSON: &str = r#"[
    {"name":"hygiene","state":"SUCCESS","bucket":"pass"},
    {"name":"rust","state":"FAILURE","bucket":"fail"},
    {"name":"slow-check","state":"PENDING","bucket":"pending"},
    {"name":"status block","state":"IN_PROGRESS","bucket":"pending"}
]"#;

#[test]
fn gates_from_checks_json_maps_buckets_and_skips_the_self_check() {
    let gates =
        status::gates_from_checks_json(CHECKS_JSON, "status block").expect("valid checks JSON");
    assert_eq!(gates, "hygiene: passed, rust: failed: FAILURE");
}

#[test]
fn gates_from_checks_json_of_an_empty_array_reads_as_no_checks_reported_yet() {
    let gates = status::gates_from_checks_json("[]", "status block").expect("valid checks JSON");
    assert_eq!(gates, "");
    let review = review_json("APPROVED", "", "");
    let input = RenderInput {
        tier_json: TIER_JSON,
        gates: &gates,
        problem: "p",
        approach: "a",
        review_json: &review,
    };
    let block = status::render(&input).expect("render succeeds");
    assert_eq!(
        row(&block, "Verified"),
        "| **Verified** | no checks reported yet |"
    );
}

#[test]
fn extract_problem_approach_reads_them_from_an_existing_block() {
    let block = new_block();
    let body = status::apply(BODY_WITHOUT_BLOCK, &block).expect("apply succeeds");
    let (problem, approach) = status::extract_problem_approach(&body)
        .expect("extraction succeeds")
        .expect("a block is there");
    assert_eq!(problem, "The problem.");
    assert_eq!(approach, "The approach.");
}

#[test]
fn extract_problem_approach_is_none_when_the_body_has_no_block_yet() {
    assert!(status::extract_problem_approach(BODY_WITHOUT_BLOCK)
        .expect("extraction succeeds")
        .is_none());
}

#[test]
fn is_unchanged_is_true_when_the_body_already_carries_this_exact_block() {
    let block = new_block();
    let body = status::apply(BODY_WITHOUT_BLOCK, &block).expect("apply succeeds");
    assert!(status::is_unchanged(&body, &block).expect("no marker error"));
}

#[test]
fn is_unchanged_is_false_when_the_bodys_block_differs() {
    let block = new_block();
    assert!(!status::is_unchanged(BODY_WITH_BLOCK, &block).expect("no marker error"));
}

#[test]
fn is_unchanged_is_false_when_the_body_has_no_block_yet() {
    let block = new_block();
    assert!(!status::is_unchanged(BODY_WITHOUT_BLOCK, &block).expect("no marker error"));
}
