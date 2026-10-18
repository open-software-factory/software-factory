//! End-to-end cases for the pull request title and description gate: `osf pr
//! check` on an event payload, then `osf scan` and `osf lint writing` on the
//! hand-written files it writes, as `.github/workflows/ci.yml` runs them.
//! Bodies live in `tests/fixtures/pr-description`. The template is this
//! repository's real one, so a change to it is tested here too.

mod common;

use common::TempDir;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const DENYLISTED: &str = "Zorblax";

const PUFFERY: &str = "The second review is a testament to careful engineering.";

/// A fixture holds `{LOCAL_PATH}` where a user path goes and `{PUFFERY}` where
/// a sentence the writing lint rejects goes. No tracked file then holds
/// either one, for the repository scan and the repository writing gate to find.
fn local_path() -> String {
    format!("{}{}", "/home", "/someone/project/notes.md")
}

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pr-description")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reads {}: {e}", path.display()))
        .replace("{LOCAL_PATH}", &local_path())
        .replace("{PUFFERY}", PUFFERY)
}

fn template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/PULL_REQUEST_TEMPLATE.md")
}

fn osf(cwd: &Path, args: &[&str], denylist: bool) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_osf"));
    common::scrub_osf_and_moon_env(&mut cmd);
    cmd.current_dir(cwd).args(args);
    if denylist {
        cmd.env("OSF_DENYLIST", DENYLISTED);
    }
    cmd.output().expect("osf runs")
}

struct Run {
    check: Output,
    scan: Output,
    lint: Output,
    lint_title: Output,
}

fn run(title: &str, body: &str) -> Run {
    run_with(&serde_json::json!({ "pull_request": { "title": title, "body": body } }))
}

fn run_with(event: &serde_json::Value) -> Run {
    let dir = TempDir::new("osf-pr-check");
    let root: &Path = &dir;
    let event_path = root.join("event.json");
    std::fs::write(&event_path, event.to_string()).expect("writes the event");
    let out = root.join("out");
    let s = |p: &Path| p.to_str().expect("utf-8 path").to_string();
    let check = osf(
        root,
        &[
            "pr",
            "check",
            "--event",
            &s(&event_path),
            "--template",
            &s(&template()),
            "--out-dir",
            &s(&out),
        ],
        true,
    );
    let (title, body) = (s(&out.join("title.txt")), s(&out.join("body.md")));
    let scan = osf(root, &["scan", "--no-suppress", &title, &body], true);
    let lint = osf(
        root,
        &[
            "lint",
            "writing",
            "--no-suppress",
            "--gate",
            "--context",
            "document",
            &body,
        ],
        true,
    );
    let lint_title = osf(
        root,
        &[
            "lint",
            "writing",
            "--no-suppress",
            "--gate",
            "--context",
            "commit",
            &title,
        ],
        true,
    );
    Run {
        check,
        scan,
        lint,
        lint_title,
    }
}

fn code(o: &Output) -> i32 {
    o.status.code().expect("exits with a code")
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

const TITLE: &str = "Reject unsigned tokens at the gate";

#[test]
fn a_good_description_passes_every_check() {
    let r = run(TITLE, &fixture("good.md"));
    assert_eq!(code(&r.check), 0, "{}", text(&r.check));
    assert_eq!(code(&r.scan), 0, "{}", text(&r.scan));
    assert_eq!(code(&r.lint), 0, "{}", text(&r.lint));
    assert_eq!(code(&r.lint_title), 0, "{}", text(&r.lint_title));
}

#[test]
fn an_empty_title_fails_the_structural_check() {
    let r = run("", &fixture("good.md"));
    assert_eq!(code(&r.check), 1, "{}", text(&r.check));
    assert!(text(&r.check).contains("the title is empty"));
}

#[test]
fn a_missing_required_heading_fails_and_names_it() {
    let r = run(TITLE, &fixture("missing-heading.md"));
    assert_eq!(code(&r.check), 1, "{}", text(&r.check));
    assert!(text(&r.check).contains("## Testing notes"));
}

#[test]
fn a_null_body_fails_the_structural_check() {
    let r = run_with(&serde_json::json!({ "pull_request": { "title": TITLE, "body": null } }));
    assert_eq!(code(&r.check), 1, "{}", text(&r.check));
}

#[test]
fn a_local_path_in_the_description_fails_the_scan() {
    let r = run(TITLE, &fixture("local-path.md"));
    assert_eq!(code(&r.check), 0, "{}", text(&r.check));
    assert_eq!(code(&r.scan), 1, "{}", text(&r.scan));
}

#[test]
fn a_local_path_in_the_title_fails_the_scan() {
    let title = format!("Read {} first", local_path());
    let r = run(&title, &fixture("good.md"));
    assert_eq!(code(&r.scan), 1, "{}", text(&r.scan));
}

#[test]
fn a_denylisted_name_fails_the_scan_without_printing_it() {
    let r = run(TITLE, &fixture("denylisted.md"));
    assert_eq!(code(&r.scan), 1, "{}", text(&r.scan));
    assert!(!text(&r.scan).contains(DENYLISTED));
}

#[test]
fn a_denylisted_name_in_the_title_fails_the_scan() {
    let r = run("Reject tokens for Zorblax", &fixture("good.md"));
    assert_eq!(code(&r.scan), 1, "{}", text(&r.scan));
}

#[test]
fn puffery_in_the_description_fails_the_writing_lint() {
    let r = run(TITLE, &fixture("puffery.md"));
    assert_eq!(code(&r.check), 0, "{}", text(&r.check));
    assert_eq!(code(&r.lint), 1, "{}", text(&r.lint));
}

#[test]
fn puffery_in_the_title_fails_the_writing_lint() {
    let r = run("A testament to careful engineering", &fixture("good.md"));
    assert_eq!(code(&r.lint_title), 1, "{}", text(&r.lint_title));
}

#[test]
fn text_inside_a_tool_written_block_is_ignored() {
    let r = run(TITLE, &fixture("bad-text-in-tool-block.md"));
    assert_eq!(code(&r.check), 0, "{}", text(&r.check));
    assert_eq!(code(&r.scan), 0, "{}", text(&r.scan));
    assert_eq!(code(&r.lint), 0, "{}", text(&r.lint));
}

#[test]
fn the_same_text_outside_a_block_fails() {
    let mut body = fixture("good.md");
    body.push('\n');
    body.push_str(PUFFERY);
    body.push('\n');
    let r = run(TITLE, &body);
    assert_eq!(code(&r.lint), 1, "{}", text(&r.lint));
}

#[test]
fn an_event_with_no_pull_request_is_a_usage_error_not_a_pass() {
    let r = run_with(&serde_json::json!({ "action": "push" }));
    assert_eq!(code(&r.check), 2, "{}", text(&r.check));
}

#[test]
fn an_unreadable_template_is_a_usage_error_not_a_pass() {
    let dir = TempDir::new("osf-pr-check-notemplate");
    let event = dir.join("event.json");
    std::fs::write(&event, r#"{"pull_request":{"title":"T","body":"B"}}"#)
        .expect("writes the event");
    let out = osf(
        &dir,
        &[
            "pr",
            "check",
            "--event",
            event.to_str().expect("utf-8"),
            "--template",
            "missing-template.md",
            "--out-dir",
            dir.join("o").to_str().expect("utf-8"),
        ],
        false,
    );
    assert_eq!(code(&out), 2, "{}", text(&out));
}

#[test]
fn an_exempt_author_is_reported_as_not_checked_and_writes_nothing() {
    let dir = TempDir::new("osf-pr-check-exempt");
    let event = dir.join("event.json");
    std::fs::write(
        &event,
        serde_json::json!({ "pull_request": { "title": "", "body": null, "user": { "login": "update-bot[bot]" } } })
            .to_string(),
    )
    .expect("writes the event");
    let out = dir.join("o");
    let args = |author: &str| {
        osf(
            &dir,
            &[
                "pr",
                "check",
                "--event",
                event.to_str().expect("utf-8"),
                "--template",
                template().to_str().expect("utf-8"),
                "--out-dir",
                out.to_str().expect("utf-8"),
                "--exempt-author",
                author,
            ],
            false,
        )
    };
    let exempt = args("update-bot[bot]");
    assert_eq!(code(&exempt), 0, "{}", text(&exempt));
    assert!(text(&exempt).contains("NOT CHECKED"));
    assert!(!out.exists());
    let other = args("someone-else[bot]");
    assert_eq!(code(&other), 1, "{}", text(&other));
}
