//! End-to-end proof that `unplaceable-reference` runs against the real, compiled `osf` binary at the stop hook, the commit-message check, and the prompt hook.

mod common;

use common::{isolated_home, run_osf, run_osf_with_stdin, TempRepo};

/// The six rule ids this branch deleted, replaced by one id.
const DELETED_RULE_IDS: &[&str] = &[
    "bare-reference",
    "reference-without-label",
    "reference-without-link",
    "chat-local-reference",
    "undefined-name",
    "undefined-name-at-start",
];

fn stop_event(session: &str, reply: &str) -> String {
    serde_json::json!({
        "session_id": session,
        "last_assistant_message": reply,
    })
    .to_string()
}

fn run_stop(case: &str, reply: &str) -> std::process::Output {
    let repo = TempRepo::new(&format!("stop-{case}"));
    let home = isolated_home(&format!("stop-{case}"));
    let event = stop_event(&format!("e2e-unplaceable-stop-{case}"), reply);
    run_osf_with_stdin(&repo.dir, &home, &["hook", "stop"], &event)
}

fn is_refused(output: &std::process::Output) -> bool {
    output.status.code() == Some(2)
}

#[test]
fn a_bare_fix_number_refuses_the_stop_naming_unplaceable_reference() {
    let out = run_stop("fix-5", "Deploying fix 5 cleared the stuck queue.");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(is_refused(&out), "expected a refusal: {out:?}");
    assert!(stderr.contains("unplaceable-reference"), "{stderr}");
}

#[test]
fn a_bare_issue_number_refuses_the_stop_naming_unplaceable_reference() {
    let out = run_stop("issue-31", "We tracked it down to issue 31 today.");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(is_refused(&out), "expected a refusal: {out:?}");
    assert!(stderr.contains("unplaceable-reference"), "{stderr}");
}

#[test]
fn a_reply_that_quotes_fix_5_as_an_example_is_not_refused() {
    let out = run_stop("quoted-fix-5", "The label \"fix 5\" means nothing here.");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!is_refused(&out), "should not refuse: {stderr}");
}

#[test]
fn a_repository_qualified_number_with_a_description_and_a_label_is_not_refused() {
    let out = run_stop(
        "repo-qualified",
        "The change landed in open-software-factory/software-factory#131 \
         (one rule for references), labelled done.",
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!is_refused(&out), "should not refuse: {stderr}");
}

/// A relative time is never a candidate in transcript context, so it never refuses a reply.
#[test]
fn a_relative_time_in_a_reply_is_not_refused() {
    let out = run_stop(
        "relative-time",
        "We shipped that yesterday, right after the deploy went out.",
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!is_refused(&out), "should not refuse: {stderr}");
}

fn commit_message_check(case: &str, message: &str) -> std::process::Output {
    let repo = TempRepo::new(&format!("commit-msg-{case}"));
    let home = isolated_home(&format!("commit-msg-{case}"));
    let path = repo.dir.join("commit-msg.txt");
    std::fs::write(&path, message).expect("commit message file writes");
    run_osf(
        &repo.dir,
        &home,
        &[
            "lint",
            "writing",
            "--context",
            "commit",
            "--format",
            "human",
            path.to_str().expect("temp path is valid UTF-8"),
        ],
    )
}

#[test]
fn a_commit_message_with_mechanism_b_fails_the_commit_message_check() {
    let out = commit_message_check(
        "mechanism-b",
        "fix: patch the race in the retry queue\n\n\
         The outage traced back to mechanism (b), a race between two \
         workers writing the same row.\n",
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !out.status.success(),
        "expected a failing exit code: {out:?}"
    );
    assert!(stdout.contains("unplaceable-reference"), "{stdout}");
}

#[test]
fn a_clean_commit_message_passes_the_commit_message_check() {
    let out = commit_message_check(
        "clean",
        "fix: stop double-processing of retried transfer events\n\n\
         Adds a lock around the retry queue so two workers can no longer \
         claim the same transfer at once.\n",
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "expected a clean pass: stdout={stdout} stderr={stderr}"
    );
}

/// Proves the check runs the rule in commit context: transcript context ignores this relative time, commit context does not.
#[test]
fn the_same_relative_time_that_a_reply_may_use_fails_the_commit_message_check() {
    let out = commit_message_check(
        "relative-time",
        "fix: land the deploy fix\n\n\
         We shipped that yesterday, right after the deploy went out.\n",
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !out.status.success(),
        "expected a failing exit code: {out:?}"
    );
    assert!(stdout.contains("unplaceable-reference"), "{stdout}");
}

/// The reminder printed on every prompt must never point at a retired rule id.
#[test]
fn the_prompt_hook_never_names_a_deleted_rule_id() {
    let repo = TempRepo::new("prompt-no-deleted-ids");
    let home = isolated_home("prompt-no-deleted-ids");
    let event = serde_json::json!({ "session_id": "e2e-unplaceable-prompt" }).to_string();
    let out = run_osf_with_stdin(&repo.dir, &home, &["hook", "prompt"], &event);
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.is_empty(), "the reminder must print something");
    for id in DELETED_RULE_IDS {
        assert!(
            !stdout.contains(id),
            "prompt text still names {id}: {stdout}"
        );
    }
}
