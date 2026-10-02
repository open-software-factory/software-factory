//! Behavioral cases for `osf pr status`: rendering the block from named
//! sources, and applying it between markers in a pull request description.
//! Every case here is offline: nothing is read from or written to GitHub,
//! except the two cases that exercise the `gh` composition through a fake
//! client.

use osf::marker;
use osf::pr_status::{self, GhClient, RenderInput, StatusError};
use std::cell::RefCell;

const TIER_JSON: &str = r#"{"tier":"normal","reasons":["3 files, 120 lines, no high-blast-radius path","touches no deploy file"]}"#;

const HEAD: &str = "0123456789abcdef0123456789abcdef01234567";
const START: &str = "<!-- osf:status:start head=0123456789abcdef0123456789abcdef01234567 -->";
const END: &str = "<!-- osf:status:end -->";
const OLD_START: &str = "<!-- factory:status:begin -->";
const OLD_END: &str = "<!-- factory:status:end -->";

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
        review_json,
        head: HEAD,
        tests: None,
    }
}

fn render_ok(gates: &str, review_json: &str) -> String {
    pr_status::render(&base_input(gates, review_json)).expect("render succeeds")
}

fn row(block: &str, label: &str) -> String {
    block
        .lines()
        .find(|line| line.starts_with(&format!("| {label} |")))
        .unwrap_or_else(|| panic!("no {label} row in:\n{block}"))
        .to_string()
}

#[test]
fn the_block_opens_with_the_new_marker_and_a_heading_naming_the_head() {
    let block = render_ok("rust: passed", &advisory_two_rounds_review());
    let mut lines = block.lines();
    assert_eq!(lines.next(), Some(START));
    assert_eq!(lines.next(), Some("### Status at 0123456"));
    assert_eq!(block.lines().last(), Some(END));
}

#[test]
fn the_head_sha_is_written_into_the_start_marker() {
    let review = advisory_two_rounds_review();
    let input = RenderInput {
        head: "fedcba9876543210",
        ..base_input("rust: passed", &review)
    };
    let block = pr_status::render(&input).expect("render succeeds");
    assert_eq!(
        block.lines().next(),
        Some("<!-- osf:status:start head=fedcba9876543210 -->")
    );
    assert!(block.contains("### Status at fedcba9"), "{block}");
}

#[test]
fn the_table_has_the_agreed_header_and_rows_in_order() {
    let block = render_ok("rust: passed", &advisory_two_rounds_review());
    assert!(block.contains("| Check | Result | Details |\n|---|---|---|\n"));
    let labels: Vec<&str> = block
        .lines()
        .filter(|l| l.starts_with("| ") && !l.starts_with("| Check"))
        .filter_map(|l| l.split('|').nth(1))
        .map(str::trim)
        .collect();
    assert_eq!(
        labels,
        [
            "Risk",
            "Tests",
            "CI",
            "Commit messages",
            "Contributor agreement",
            "Automated review",
            "Human review"
        ]
    );
}

#[test]
fn a_passing_change_shows_passed_ci_and_the_advisory_approval() {
    let block = render_ok(
        "build: passed, tests (412): passed",
        &advisory_two_rounds_review(),
    );
    assert_eq!(row(&block, "CI"), "| CI | ✅ passed | all 2 passed |");
    assert_eq!(
        row(&block, "Automated review"),
        "| Automated review | ✅ APPROVE | 1 round, 6 findings, 5 fixed, 1 justified, 0 deferred |"
    );
    assert_eq!(
        row(&block, "Human review"),
        "| Human review | ⏳ waiting | no approving review yet |"
    );
    assert_eq!(
        row(&block, "Risk"),
        "| Risk | normal | 3 files, 120 lines, no high-blast-radius path |"
    );
}

#[test]
fn a_failed_gate_fails_ci_and_keeps_its_reason_while_trimming_names() {
    let block = render_ok(
        "build (api) : passed, lint: failed: 3 warnings",
        &advisory_two_rounds_review(),
    );
    assert_eq!(
        row(&block, "CI"),
        "| CI | ❌ failed | 1 of 2 passed, failed: lint (3 warnings) |"
    );
}

#[test]
fn no_gates_yet_reads_as_waiting_never_as_passed() {
    let block = render_ok("", &advisory_two_rounds_review());
    assert_eq!(
        row(&block, "CI"),
        "| CI | ⏳ waiting | no checks reported yet |"
    );
}

#[test]
fn a_check_that_has_no_implementation_is_not_run_and_never_passed() {
    let block = render_ok("rust: passed", &advisory_two_rounds_review());
    assert!(row(&block, "Commit messages").contains("⏸ not run"));
    assert!(row(&block, "Contributor agreement").contains("⏸ not run"));
    assert!(!row(&block, "Commit messages").contains('✅'));
    assert!(!row(&block, "Contributor agreement").contains('✅'));
}

#[test]
fn a_native_changes_requested_decision_fails_human_review() {
    let review = review_json("CHANGES_REQUESTED", "", "");
    let block = render_ok("build: passed", &review);
    assert_eq!(
        row(&block, "Human review"),
        "| Human review | ❌ changes requested | changes requested on GitHub |"
    );
    assert_eq!(
        row(&block, "Automated review"),
        "| Automated review | ⏳ waiting | no round yet |"
    );
}

#[test]
fn a_native_approved_decision_passes_human_review() {
    let review = review_json("APPROVED", "", "");
    let block = render_ok("build: passed", &review);
    assert_eq!(
        row(&block, "Human review"),
        "| Human review | ✅ approved | approved on GitHub |"
    );
}

#[test]
fn an_advisory_request_for_changes_fails_automated_review() {
    let review = review_json(
        "REVIEW_REQUIRED",
        r#"{"state":"COMMENTED","body":"**Verdict (advisory): REQUEST_CHANGES** — posted as a comment."}"#,
        "",
    );
    let block = render_ok("build: passed", &review);
    assert_eq!(
        row(&block, "Automated review"),
        "| Automated review | ❌ REQUEST_CHANGES | no round yet |"
    );
    assert_eq!(
        row(&block, "Human review"),
        "| Human review | ⏳ waiting | no approving review yet |"
    );
}

#[test]
fn the_risk_reasons_go_in_a_collapsed_section_named_for_the_level() {
    let block = render_ok("build: passed", &advisory_two_rounds_review());
    assert!(
        block.contains(
            "<details>\n<summary>Why the risk is normal</summary>\n\n- 3 files, 120 lines, no high-blast-radius path\n- touches no deploy file\n\n</details>\n"
        ),
        "{block}"
    );
}

#[test]
fn a_bar_in_a_cell_is_escaped_so_the_table_holds() {
    let block = render_ok("a|b: failed: x|y", &advisory_two_rounds_review());
    assert!(row(&block, "CI").contains("a\\|b (x\\|y)"), "{block}");
}

#[test]
fn a_tier_json_missing_the_required_fields_is_refused() {
    let review = advisory_two_rounds_review();
    let input = RenderInput {
        tier_json: "{}",
        ..base_input("build: passed", &review)
    };
    let err = pr_status::render(&input).expect_err("empty tier object is refused");
    assert!(format!("{err}").contains("tier"), "{err}");
}

#[test]
fn a_tier_json_with_a_non_string_reason_is_refused() {
    let review = advisory_two_rounds_review();
    let input = RenderInput {
        tier_json: r#"{"tier":"normal","reasons":[{}]}"#,
        ..base_input("build: passed", &review)
    };
    let err = pr_status::render(&input).expect_err("a non-string reason is refused");
    assert!(format!("{err}").contains("reasons"), "{err}");
}

#[test]
fn an_empty_head_is_refused() {
    let review = advisory_two_rounds_review();
    let input = RenderInput {
        head: "",
        ..base_input("build: passed", &review)
    };
    let err = pr_status::render(&input).expect_err("an empty head is refused");
    assert!(format!("{err}").contains("--head"), "{err}");
}

#[test]
fn review_json_that_is_not_json_at_all_is_refused() {
    let input = RenderInput {
        review_json: "",
        ..base_input("build: passed", "")
    };
    assert!(pr_status::render(&input).is_err());
}

#[test]
fn review_json_with_a_non_object_review_is_refused() {
    let input = RenderInput {
        review_json: r#"{"reviews":[1]}"#,
        ..base_input("build: passed", "")
    };
    let err = pr_status::render(&input).expect_err("a non-object review is refused");
    assert!(format!("{err}").contains("shape"), "{err}");
}

#[test]
fn a_comma_inside_a_gate_name_is_refused_naming_the_cause() {
    let review = advisory_two_rounds_review();
    let input = base_input("tests (35, both paths): passed", &review);
    let err = pr_status::render(&input).expect_err("a comma inside a gate name is refused");
    assert!(
        format!("{err}").contains("a comma inside a gate name"),
        "{err}"
    );
}

#[test]
fn a_gate_result_other_than_passed_or_failed_is_refused() {
    let review = advisory_two_rounds_review();
    let input = base_input("build: ok", &review);
    let err = pr_status::render(&input).expect_err("an unknown gate result is refused");
    assert!(
        format!("{err}").contains("must end ': passed' or ': failed"),
        "{err}"
    );
}

#[test]
fn render_gives_byte_identical_output_on_the_same_inputs() {
    let review = advisory_two_rounds_review();
    let input = base_input("build: passed, tests (412): passed", &review);
    let first = pr_status::render(&input).expect("first render succeeds");
    let second = pr_status::render(&input).expect("second render succeeds");
    assert_eq!(first.as_bytes(), second.as_bytes());
}

#[test]
fn with_no_tests_summary_the_tests_row_is_not_run_and_has_no_detail_section() {
    let block = render_ok("build: passed", &advisory_two_rounds_review());
    assert_eq!(
        row(&block, "Tests"),
        "| Tests | ⏸ not run | no base to compare against |"
    );
    assert!(!block.contains("Test changes"));
}

#[test]
fn a_given_tests_summary_fills_the_row_and_a_collapsed_section() {
    let review = advisory_two_rounds_review();
    let input = RenderInput {
        tests: Some("**Tests**: 1 added, 0 changed, 0 removed\n- `osf`: 1 added, 0 changed, 0 removed\n  - added: it works"),
        ..base_input("build: passed", &review)
    };
    let block = pr_status::render(&input).expect("render succeeds");
    assert_eq!(
        row(&block, "Tests"),
        "| Tests | 1 added, 0 changed, 0 removed | [Testing notes](#testing-notes) |"
    );
    assert!(
        block.contains(
            "<details>\n<summary>Test changes</summary>\n\n- `osf`: 1 added, 0 changed, 0 removed\n  - added: it works\n\n</details>\n"
        ),
        "{block}"
    );
}

const BODY_WITH_NEW_BLOCK: &str = "Issue: open-software-factory/software-factory#1 (the thing)\n\n<!-- osf:status:start head=aaaaaaaa -->\n### Status at aaaaaaa\n\nold table\n<!-- osf:status:end -->\n\n## Testing notes\n\nBody text that must not change.\n";

const BODY_WITH_OLD_BLOCK: &str = "<!-- factory:status:begin -->\n| | |\n|---|---|\n| **Ready** | blocked by review: pending |\n| **Risk** | low \u{2014} old reasons |\n<!-- factory:status:end -->\n\n## What changed and why\n\nBody text that must not change.\n\n## Issue\n\nCloses open-software-factory/software-factory#1, the thing.\n";

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
            if marker::is_start_line(line, "status") {
                skipping = true;
                return false;
            }
            if marker::is_end_line(line, "status") {
                skipping = false;
                return false;
            }
            !skipping
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_new_marker_round_trips() {
    let block = new_block();
    let once = pr_status::apply(BODY_WITHOUT_BLOCK, &block).expect("first apply succeeds");
    assert_eq!(once.lines().next(), Some(START));
    let found = pr_status::current_block(&once)
        .expect("markers resolve")
        .expect("a block is there");
    assert_eq!(found, block);
    assert!(pr_status::is_unchanged(&once, &block).expect("no marker error"));
}

#[test]
fn a_block_for_a_new_head_replaces_the_one_for_the_old_head() {
    let review = advisory_two_rounds_review();
    let first = new_block();
    let moved = pr_status::render(&RenderInput {
        head: "ffffffffffffffff",
        ..base_input("build: passed", &review)
    })
    .expect("render succeeds");
    let once = pr_status::apply(BODY_WITHOUT_BLOCK, &first).expect("first apply");
    assert!(!pr_status::is_unchanged(&once, &moved).expect("no marker error"));
    let twice = pr_status::apply(&once, &moved).expect("second apply");
    assert_eq!(twice.matches("osf:status:start").count(), 1, "{twice}");
    assert!(twice.contains("head=ffffffffffffffff"), "{twice}");
    assert!(!twice.contains(HEAD), "{twice}");
}

#[test]
fn an_old_style_block_is_replaced_in_place_not_duplicated() {
    let block = new_block();
    let out = pr_status::apply(BODY_WITH_OLD_BLOCK, &block).expect("apply succeeds");
    assert!(!out.contains("factory:status"), "{out}");
    assert_eq!(out.matches("osf:status:start").count(), 1, "{out}");
    assert_eq!(out.matches("osf:status:end").count(), 1, "{out}");
    assert!(!out.contains("old reasons"), "{out}");
    assert_eq!(out.lines().next(), Some(START));
    assert_eq!(strip_block(BODY_WITH_OLD_BLOCK), strip_block(&out));
}

#[test]
fn an_old_style_block_counts_as_changed_so_a_refresh_migrates_it() {
    let block = new_block();
    assert!(!pr_status::is_unchanged(BODY_WITH_OLD_BLOCK, &block).expect("no marker error"));
}

#[test]
fn apply_replaces_the_existing_block_and_changes_nothing_else() {
    let block = new_block();
    let out = pr_status::apply(BODY_WITH_NEW_BLOCK, &block).expect("apply succeeds");
    assert_eq!(strip_block(BODY_WITH_NEW_BLOCK), strip_block(&out));
    assert!(out.contains("| CI | ✅ passed | all 2 passed |"));
    assert!(!out.contains("old table"));
    assert!(out.contains("Body text that must not change."));
}

#[test]
fn apply_is_idempotent() {
    let block = new_block();
    let once = pr_status::apply(BODY_WITH_NEW_BLOCK, &block).expect("first apply succeeds");
    let twice = pr_status::apply(&once, &block).expect("second apply succeeds");
    assert_eq!(once, twice);
}

#[test]
fn apply_inserts_the_block_at_the_top_when_markers_are_absent() {
    let block = new_block();
    let out = pr_status::apply(BODY_WITHOUT_BLOCK, &block).expect("apply succeeds");
    assert_eq!(out.lines().next(), Some(START));
    assert_eq!(
        strip_block(&out),
        format!("\n{}", BODY_WITHOUT_BLOCK.trim_end_matches('\n'))
    );
}

#[test]
fn apply_keeps_three_trailing_newlines_exactly() {
    let block = new_block();
    let body = format!("{BODY_WITH_NEW_BLOCK}\n\n");
    let out = pr_status::apply(&body, &block).expect("apply succeeds");
    assert!(out.ends_with("\n\n\n"), "{:?}", &out[out.len() - 6..]);
    assert!(!out.ends_with("\n\n\n\n"));
}

#[test]
fn apply_adds_no_final_newline_when_the_body_has_none() {
    let block = new_block();
    let body = BODY_WITHOUT_BLOCK.trim_end_matches('\n');
    let out = pr_status::apply(body, &block).expect("apply succeeds");
    assert!(!out.ends_with('\n'), "{:?}", &out[out.len() - 6..]);
}

#[test]
fn apply_keeps_a_long_run_of_trailing_newlines_and_stays_fast() {
    let block = new_block();
    let long_trail = "\n".repeat(20_000);
    let body = format!("{BODY_WITH_NEW_BLOCK}{long_trail}");
    let started = std::time::Instant::now();
    let out = pr_status::apply(&body, &block).expect("apply succeeds");
    assert!(started.elapsed().as_secs() < 5, "apply must finish quickly");
    assert!(out.ends_with(&long_trail));
}

#[test]
fn a_body_with_a_start_marker_and_no_end_marker_is_refused() {
    let block = new_block();
    let body = format!("{START}\ncell\n");
    let err = pr_status::apply(&body, &block).expect_err("begin without end is refused");
    assert!(
        format!("{err}").contains("1 begin marker(s) and 0 end marker(s)"),
        "{err}"
    );
}

#[test]
fn a_body_with_an_old_block_and_a_new_block_is_refused_not_merged() {
    let block = new_block();
    let body = format!("{OLD_START}\nx\n{OLD_END}\n\n{START}\ny\n{END}\n");
    let err = pr_status::apply(&body, &block).expect_err("two blocks are refused");
    assert!(
        format!("{err}").contains("2 begin marker(s) and 2 end marker(s)"),
        "{err}"
    );
}

#[test]
fn a_marker_with_trailing_space_is_not_a_marker() {
    let block = new_block();
    let body = BODY_WITH_NEW_BLOCK.replace(
        "<!-- osf:status:start head=aaaaaaaa -->\n",
        "<!-- osf:status:start head=aaaaaaaa --> \n",
    );
    let err = pr_status::apply(&body, &block).expect_err("a marker with trailing space is refused");
    assert!(
        format!("{err}").contains("0 begin marker(s) and 1 end marker(s)"),
        "{err}"
    );
}

#[test]
fn a_block_file_with_an_inline_marker_is_refused() {
    let bad_block = format!("x {START} y\n{END}\n");
    let err = pr_status::apply(BODY_WITHOUT_BLOCK, &bad_block)
        .expect_err("an inline marker does not count as a marker line");
    assert!(
        format!("{err}").contains("each marker exactly once"),
        "{err}"
    );
}

#[test]
fn a_block_file_with_reversed_markers_is_refused() {
    let bad_block = format!("{END}\ncell\n{START}\n");
    let err = pr_status::apply(BODY_WITHOUT_BLOCK, &bad_block)
        .expect_err("reversed markers in the block are refused");
    assert!(
        format!("{err}").contains("end marker comes before its begin marker"),
        "{err}"
    );
}

#[test]
fn a_body_with_reversed_markers_is_refused() {
    let block = new_block();
    let body = format!("{END}\ncell\n{START}\n");
    let err =
        pr_status::apply(&body, &block).expect_err("reversed markers in the body are refused");
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
    let crlf_body = BODY_WITH_OLD_BLOCK.replace('\n', "\r\n") + "\r\n";
    let out = pr_status::apply(&crlf_body, &block).expect("apply succeeds on a CRLF body");
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
    assert!(out.contains("| CI | ✅ passed"));
    assert!(out.contains("Body text that must not change."));
    assert!(!out.contains("old reasons"));
}

#[test]
fn apply_on_a_crlf_body_with_no_existing_block_inserts_a_crlf_block() {
    let block = new_block();
    let crlf_body = BODY_WITHOUT_BLOCK.replace('\n', "\r\n");
    let out = pr_status::apply(&crlf_body, &block).expect("apply succeeds");
    let first_line_end = out.find('\n').expect("at least one line");
    assert_eq!(&out[..=first_line_end], format!("{START}\r\n"));
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
    let result = pr_status::apply_via_gh(
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
    let new_body = pr_status::apply_via_gh(
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
    assert_eq!(new_body.lines().next(), Some(START));
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
        pr_status::gates_from_checks_json(CHECKS_JSON, "status block").expect("valid checks JSON");
    assert_eq!(gates, "hygiene: passed, rust: failed: FAILURE");
}

#[test]
fn gates_from_checks_json_of_an_empty_array_reads_as_no_checks_reported_yet() {
    let gates = pr_status::gates_from_checks_json("[]", "status block").expect("valid checks JSON");
    assert_eq!(gates, "");
    let review = review_json("APPROVED", "", "");
    let block = render_ok(&gates, &review);
    assert_eq!(
        row(&block, "CI"),
        "| CI | ⏳ waiting | no checks reported yet |"
    );
}

#[test]
fn is_unchanged_is_true_when_the_body_already_carries_this_exact_block() {
    let block = new_block();
    let body = pr_status::apply(BODY_WITHOUT_BLOCK, &block).expect("apply succeeds");
    assert!(pr_status::is_unchanged(&body, &block).expect("no marker error"));
}

#[test]
fn is_unchanged_is_false_when_the_bodys_block_differs() {
    let block = new_block();
    assert!(!pr_status::is_unchanged(BODY_WITH_NEW_BLOCK, &block).expect("no marker error"));
}

#[test]
fn is_unchanged_is_false_when_the_body_has_no_block_yet() {
    let block = new_block();
    assert!(!pr_status::is_unchanged(BODY_WITHOUT_BLOCK, &block).expect("no marker error"));
}
