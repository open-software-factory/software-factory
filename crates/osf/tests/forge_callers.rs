//! Characterization tests for the three `osf` commands that will move behind
//! a forge interface: `review post`, `pr status apply`, and `pr status
//! refresh`. A fake `gh` shell script sits first on `PATH`; nothing here
//! reaches the network or a real `gh`.

mod common;

use common::{isolated_home, run_osf_with_env, TempRepo};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;

/// One fake `gh` for every test: it logs each call's argv, saves the stdin of
/// each `api` call, and answers from the fixture files the test writes.
const FAKE_GH_SCRIPT: &str = r#"#!/usr/bin/env bash
set -euo pipefail
dir="${FAKE_GH_DIR:?FAKE_GH_DIR is set by the test}"
calls="$dir/calls"
stdin_dir="$dir/stdin"
printf '%s\n' "$*" >> "$calls"

if [ "$1" = "pr" ] && [ "$2" = "view" ]; then
  case "$*" in
    *"--json headRefOid")
      printf '%s\n' '{"headRefOid":"cafe1234"}'
      exit 0
      ;;
    *"--json body -q .body")
      if [ -f "$dir/view-body-fail" ]; then
        cat "$dir/view-body-fail" >&2
        exit 1
      fi
      cat "$dir/body"
      exit 0
      ;;
    *"--json body,baseRefName,headRefName")
      cat "$dir/pr-info.json"
      exit 0
      ;;
    *"--json reviewDecision,reviews,comments")
      printf '%s\n' '{"reviewDecision":"","reviews":[],"comments":[]}'
      exit 0
      ;;
  esac
fi

if [ "$1" = "pr" ] && [ "$2" = "checks" ]; then
  if [ -f "$dir/checks-fail" ]; then
    cat "$dir/checks-fail" >&2
    exit 1
  fi
  if [ -f "$dir/checks.json" ]; then
    cat "$dir/checks.json"
    exit 0
  fi
fi

if [ "$1" = "pr" ] && [ "$2" = "edit" ]; then
  prev=""
  for arg in "$@"; do
    if [ "$prev" = "--body-file" ]; then
      cp "$arg" "$dir/out"
      exit 0
    fi
    prev="$arg"
  done
fi

if [ "$1" = "api" ] && [ "$2" = "-X" ] && [ "$3" = "POST" ]; then
  n=0
  if [ -f "$dir/api-count" ]; then
    n=$(cat "$dir/api-count")
  fi
  n=$((n + 1))
  printf '%s' "$n" > "$dir/api-count"
  cat > "$stdin_dir/api-$n.json"
  if [ "$n" = "1" ] && [ -f "$dir/api-fail" ]; then
    cat "$dir/api-fail" >&2
    exit 1
  fi
  if [ "$n" = "1" ]; then
    printf '%s\n' '{"id":42,"state":"CHANGES_REQUESTED","html_url":"https://example.invalid/r/42"}'
  else
    printf '%s\n' '{"id":43,"state":"COMMENTED","html_url":"https://example.invalid/r/43"}'
  fi
  exit 0
fi

echo "fake gh: unrecognized args" >&2
exit 1
"#;

const FINDINGS_JSON: &str = r#"[
  {"id":"cor-01","path":"src/lib.rs","line":10,"severity":"major","action":"should-fix","body":"Fix the loop."},
  {"id":"hyg-01","path":"AGENTS.md","severity":"minor","action":"maybe-fix","body":"Add a note."}
]"#;
const SUMMARY_MARKDOWN: &str = "Summary line.\n";
const REVIEW_BODY: &str = "Summary line.\n\n**Findings outside the diff**\n- **hyg-01** (minor, maybe-fix) `AGENTS.md`: Add a note.";
const COMMENT_BODY: &str = "**cor-01** · major · should-fix\n\nFix the loop.";

const APPLY_BODY: &str = "Existing text.\n";
const STATUS_BLOCK: &str = concat!(
    "<!-- osf:status:start head=abc1234 -->\n",
    "### Status at `abc1234`\n",
    "\n",
    "| Check | Result | Details |\n",
    "|---|---|---|\n",
    "| **CI** | ✅ passed | all 1 passed |\n",
    "<!-- osf:status:end -->\n",
);
const PASSING_CHECKS: &str = r#"[{"name":"lint","state":"SUCCESS","bucket":"pass"}]"#;
const FAILING_CHECKS: &str = r#"[{"name":"lint","state":"SUCCESS","bucket":"pass"},{"name":"rust","state":"FAILURE","bucket":"fail"}]"#;

/// A fake `gh` on its own `PATH` entry, plus the calls log, saved `api`
/// stdin, and the answer files the script reads.
struct FakeGh {
    dir: PathBuf,
    body: PathBuf,
    pr_info: PathBuf,
    out: PathBuf,
    calls: PathBuf,
    stdin_dir: PathBuf,
}

impl FakeGh {
    fn new(name: &str, initial_body: &str) -> Self {
        let base = std::env::temp_dir(); // osf: temp-dir allowed, unique per test name
        let dir = base.join(format!("osf-forge-callers-{name}"));
        let _ = fs::remove_dir_all(&dir);
        let stdin_dir = dir.join("stdin");
        fs::create_dir_all(&stdin_dir).expect("fake gh stdin dir creates");
        let script = dir.join("gh");
        fs::write(&script, FAKE_GH_SCRIPT).expect("fake gh script writes");
        let mut perms = fs::metadata(&script)
            .expect("fake gh metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("fake gh chmod");
        let fake = FakeGh {
            body: dir.join("body"),
            pr_info: dir.join("pr-info.json"),
            out: dir.join("out"),
            calls: dir.join("calls"),
            stdin_dir,
            dir,
        };
        fake.set_body(initial_body);
        fake
    }

    /// Writes the description the fake `gh pr view` answers with, both to the
    /// plain body file and to the `body,baseRefName,headRefName` JSON file.
    fn set_body(&self, body: &str) {
        fs::write(&self.body, body).expect("body fixture writes");
        let info = serde_json::json!({
            "body": body,
            "baseRefName": "main",
            "headRefName": "feature",
        });
        fs::write(&self.pr_info, info.to_string()).expect("pr info fixture writes");
    }

    fn set_checks(&self, json: &str) {
        fs::write(self.dir.join("checks.json"), json).expect("checks fixture writes");
    }

    fn fail_checks(&self, stderr: &str) {
        fs::write(self.dir.join("checks-fail"), stderr).expect("checks failure fixture writes");
    }

    fn fail_body_view(&self, stderr: &str) {
        fs::write(self.dir.join("view-body-fail"), stderr).expect("body failure fixture writes");
    }

    fn fail_first_api(&self, stderr: &str) {
        fs::write(self.dir.join("api-fail"), stderr).expect("api failure fixture writes");
    }

    fn run_osf(&self, workdir: &Path, home: &Path, args: &[&str]) -> Output {
        let ambient = std::env::var("PATH").unwrap_or_default();
        let path = format!("{}:{ambient}", self.dir.display());
        let fake_dir = self.dir.to_str().expect("fake gh dir is utf-8");
        run_osf_with_env(
            workdir,
            home,
            &[("PATH", path.as_str()), ("FAKE_GH_DIR", fake_dir)],
            args,
        )
    }

    fn calls(&self) -> String {
        fs::read_to_string(&self.calls).unwrap_or_default()
    }

    fn saved_stdin(&self, call: usize) -> String {
        fs::read_to_string(self.stdin_dir.join(format!("api-{call}.json")))
            .expect("gh api saved its stdin")
    }

    fn written_body(&self) -> String {
        fs::read_to_string(&self.out).expect("gh pr edit wrote an out file")
    }
}

impl Drop for FakeGh {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_ok(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        stdout_text(output),
        stderr_text(output)
    );
}

/// Writes the shared findings and summary fixtures and drives `osf review
/// post` against the fake, with any extra flags appended.
fn review_post(fake: &FakeGh, home: &Path, extra: &[&str]) -> Output {
    let findings = fake.dir.join("findings.json");
    let summary = fake.dir.join("summary.md");
    fs::write(&findings, FINDINGS_JSON).expect("findings fixture writes");
    fs::write(&summary, SUMMARY_MARKDOWN).expect("summary fixture writes");
    let mut args = vec![
        "review",
        "post",
        "o/r",
        "7",
        findings.to_str().expect("a utf-8 path"),
        summary.to_str().expect("a utf-8 path"),
    ];
    args.extend_from_slice(extra);
    fake.run_osf(&fake.dir, home, &args)
}

fn expected_comments() -> serde_json::Value {
    serde_json::json!([
        {"path":"src/lib.rs","line":10,"side":"RIGHT","body":COMMENT_BODY}
    ])
}

fn expected_payload(body: &str, event: &str) -> serde_json::Value {
    serde_json::json!({
        "commit_id": "cafe1234",
        "body": body,
        "event": event,
        "comments": expected_comments(),
    })
}

#[test]
fn review_post_posts_one_native_review() {
    let fake = FakeGh::new("review-post-native", "");
    let home = isolated_home("forge-review-native");
    let output = review_post(&fake, &home, &[]);
    assert_ok(&output);
    assert_eq!(
        stdout_text(&output),
        concat!(
            "posted review 42: CHANGES_REQUESTED, https://example.invalid/r/42\n",
            "osf review post: REQUEST_CHANGES with 1 inline comment(s) on o/r#7 at cafe1234\n",
        )
    );
    assert_eq!(stderr_text(&output), "");
    assert_eq!(
        fake.calls(),
        concat!(
            "pr view 7 --repo o/r --json headRefOid\n",
            "api -X POST repos/o/r/pulls/7/reviews --input -\n",
        )
    );
    let saved: serde_json::Value =
        serde_json::from_str(&fake.saved_stdin(1)).expect("saved stdin is JSON");
    assert_eq!(saved, expected_payload(REVIEW_BODY, "REQUEST_CHANGES"));
}

#[test]
fn review_post_falls_back_to_an_advisory_comment_on_a_self_review_refusal() {
    let fake = FakeGh::new("review-post-fallback", "");
    fake.fail_first_api("gh: Review cannot be requested on your own pull request (HTTP 422)\n");
    let home = isolated_home("forge-review-fallback");
    let output = review_post(&fake, &home, &[]);
    assert_ok(&output);
    assert_eq!(
        stdout_text(&output),
        concat!(
            "posted comment 43: COMMENTED, https://example.invalid/r/43\n",
            "osf review post: COMMENT (advisory REQUEST_CHANGES) with 1 inline comment(s) on o/r#7 at cafe1234\n",
        )
    );
    assert_eq!(
        stderr_text(&output),
        "osf review post: GitHub refuses REQUEST_CHANGES from the pull request's own author; posting as COMMENT with the verdict marked advisory. reviewDecision stays empty until the reviewer has its own identity.\n"
    );
    assert_eq!(
        fake.calls(),
        concat!(
            "pr view 7 --repo o/r --json headRefOid\n",
            "api -X POST repos/o/r/pulls/7/reviews --input -\n",
            "api -X POST repos/o/r/pulls/7/reviews --input -\n",
        )
    );
    let advisory_body = format!(
        "**Verdict (advisory): REQUEST_CHANGES** — posted as a comment, because the reviewer and the author share one GitHub identity.\n\n{REVIEW_BODY}"
    );
    let saved: serde_json::Value =
        serde_json::from_str(&fake.saved_stdin(2)).expect("second stdin is JSON");
    assert_eq!(
        saved.get("event").and_then(serde_json::Value::as_str),
        Some("COMMENT")
    );
    assert_eq!(saved, expected_payload(&advisory_body, "COMMENT"));
}

#[test]
fn review_post_reports_a_post_failure_with_exit_1() {
    let fake = FakeGh::new("review-post-failure", "");
    fake.fail_first_api("gh: rate limited (HTTP 403)\n");
    let home = isolated_home("forge-review-failure");
    let output = review_post(&fake, &home, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stdout_text(&output), "");
    assert_eq!(stderr_text(&output), "osf: gh: rate limited (HTTP 403)\n");
    assert_eq!(
        fake.calls(),
        concat!(
            "pr view 7 --repo o/r --json headRefOid\n",
            "api -X POST repos/o/r/pulls/7/reviews --input -\n",
        )
    );
}

#[test]
fn review_post_dry_run_posts_nothing() {
    let fake = FakeGh::new("review-post-dry-run", "");
    let home = isolated_home("forge-review-dry-run");
    let output = review_post(&fake, &home, &["--dry-run"]);
    assert_ok(&output);
    let pretty = serde_json::to_string_pretty(&expected_payload(REVIEW_BODY, "REQUEST_CHANGES"))
        .expect("the payload serializes");
    let expected_stdout = format!(
        "osf review post: dry run — REQUEST_CHANGES with 1 inline comment(s) on o/r#7 at cafe1234\n{pretty}\n"
    );
    assert_eq!(stdout_text(&output), expected_stdout);
    assert_eq!(stderr_text(&output), "");
    assert_eq!(fake.calls(), "pr view 7 --repo o/r --json headRefOid\n");
}

/// Writes the shared apply fixture: the block file and the fake's body.
fn apply_fixture(fake: &FakeGh) -> String {
    let block = fake.dir.join("block.md");
    fs::write(&block, STATUS_BLOCK).expect("block fixture writes");
    block.to_str().expect("a utf-8 path").to_string()
}

fn apply_args<'a>(block: &'a str, extra: &'a [&'a str]) -> Vec<&'a str> {
    let mut args = vec![
        "pr", "status", "apply", "--repo", "o/r", "--pr", "7", "--block", block,
    ];
    args.extend_from_slice(extra);
    args
}

#[test]
fn status_apply_reads_then_writes_the_block() {
    let fake = FakeGh::new("status-apply-write", APPLY_BODY);
    let block = apply_fixture(&fake);
    let home = isolated_home("forge-status-apply-write");
    let output = fake.run_osf(&fake.dir, &home, &apply_args(&block, &[]));
    assert_ok(&output);
    assert_eq!(
        stdout_text(&output),
        "osf pr status apply: applied to o/r#7\n"
    );
    assert_eq!(stderr_text(&output), "");
    assert_eq!(fake.written_body(), format!("{STATUS_BLOCK}\n{APPLY_BODY}"));
    let calls = fake.calls();
    let mut lines = calls.lines();
    assert_eq!(
        lines.next(),
        Some("pr view 7 --repo o/r --json body -q .body")
    );
    let edit = lines.next().expect("an edit call followed the view");
    assert!(
        edit.starts_with("pr edit 7 --repo o/r --body-file "),
        "{edit}"
    );
    assert_eq!(lines.next(), None);
}

#[test]
fn status_apply_a_failed_view_stops_before_the_edit() {
    let fake = FakeGh::new("status-apply-view-fail", APPLY_BODY);
    let block = apply_fixture(&fake);
    fake.fail_body_view("gh: not found\n");
    let home = isolated_home("forge-status-apply-view-fail");
    let output = fake.run_osf(&fake.dir, &home, &apply_args(&block, &[]));
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout_text(&output), "");
    assert_eq!(
        stderr_text(&output),
        "osf: gh pr view failed for o/r#7; nothing was changed\n"
    );
    assert_eq!(fake.calls(), "pr view 7 --repo o/r --json body -q .body\n");
}

#[test]
fn status_apply_dry_run_prints_without_editing() {
    let fake = FakeGh::new("status-apply-dry-run", APPLY_BODY);
    let block = apply_fixture(&fake);
    let home = isolated_home("forge-status-apply-dry-run");
    let output = fake.run_osf(&fake.dir, &home, &apply_args(&block, &["--dry-run"]));
    assert_ok(&output);
    assert_eq!(
        stdout_text(&output),
        format!("{STATUS_BLOCK}\n{APPLY_BODY}")
    );
    assert_eq!(stderr_text(&output), "");
    assert_eq!(fake.calls(), "pr view 7 --repo o/r --json body -q .body\n");
}

/// A throwaway repository with one commit and one uncommitted file, so
/// `changeset risk` has a change to assess against `--base HEAD`.
fn refresh_repo(name: &str) -> (TempRepo, String) {
    let repo = TempRepo::new(name);
    repo.write("notes.txt", "the tracked file\n");
    let head = repo.commit("initial");
    repo.write("change.md", "an uncommitted change\n");
    (repo, head)
}

fn run_refresh(repo: &TempRepo, fake: &FakeGh, home: &Path) -> Output {
    fake.run_osf(
        &repo.dir,
        home,
        &[
            "pr", "status", "refresh", "--repo", "o/r", "--pr", "7", "--base", "HEAD",
        ],
    )
}

#[test]
fn status_refresh_updates_with_a_failing_check() {
    let (repo, head) = refresh_repo("refresh-failing-check");
    let fake = FakeGh::new("refresh-failing-check", "PR body.\n");
    fake.set_checks(FAILING_CHECKS);
    let home = isolated_home("forge-refresh-failing-check");
    let output = run_refresh(&repo, &fake, &home);
    assert_ok(&output);
    assert_eq!(stdout_text(&output), "osf pr status refresh: updated\n");
    let body = fake.written_body();
    let start = format!("<!-- osf:status:start head={head} -->");
    assert!(body.starts_with(start.as_str()), "{body}");
    assert!(
        body.contains("| **CI** | ❌ failed | 1 of 2 passed, failed: rust (FAILURE) |"),
        "{body}"
    );
}

#[test]
fn status_refresh_waits_when_no_checks_are_reported() {
    let (repo, _head) = refresh_repo("refresh-no-checks");
    let fake = FakeGh::new("refresh-no-checks", "PR body.\n");
    fake.fail_checks("no checks reported on the 'feature' branch\n");
    let home = isolated_home("forge-refresh-no-checks");
    let output = run_refresh(&repo, &fake, &home);
    assert_ok(&output);
    assert_eq!(stdout_text(&output), "osf pr status refresh: updated\n");
    let body = fake.written_body();
    assert!(
        body.contains("| **CI** | ⏳ waiting | no checks reported yet |"),
        "{body}"
    );
}

#[test]
fn status_refresh_fails_on_a_real_checks_error() {
    let (repo, _head) = refresh_repo("refresh-checks-error");
    let fake = FakeGh::new("refresh-checks-error", "PR body.\n");
    fake.fail_checks("HTTP 502\n");
    let home = isolated_home("forge-refresh-checks-error");
    let output = run_refresh(&repo, &fake, &home);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout_text(&output), "");
    assert_eq!(
        stderr_text(&output),
        "osf: gh pr checks failed for o/r#7: HTTP 502\n"
    );
    assert!(!fake.calls().lines().any(|line| line.starts_with("pr edit")));
}

#[test]
fn status_refresh_leaves_an_unchanged_block_alone() {
    let (repo, _head) = refresh_repo("refresh-unchanged");
    let fake = FakeGh::new("refresh-unchanged", "PR body.\n");
    fake.set_checks(PASSING_CHECKS);
    let home = isolated_home("forge-refresh-unchanged");
    let first = run_refresh(&repo, &fake, &home);
    assert_ok(&first);
    assert_eq!(stdout_text(&first), "osf pr status refresh: updated\n");
    fake.set_body(&fake.written_body());
    let second = run_refresh(&repo, &fake, &home);
    assert_ok(&second);
    assert_eq!(stdout_text(&second), "osf pr status refresh: unchanged\n");
    assert_eq!(stderr_text(&second), "");
    let edits = fake
        .calls()
        .lines()
        .filter(|line| line.starts_with("pr edit"))
        .count();
    assert_eq!(edits, 1);
}
