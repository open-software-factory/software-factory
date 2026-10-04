//! `osf review work-item`, end to end through a fake `gh`, and the saved work
//! item read by the shipped spec and acceptance lens, so no test reaches the network.

mod common;

use common::{TempDir, TempRepo};
use std::path::Path;

/// A folder holding a fake `gh` that logs its arguments, then either prints
/// the file named by `OSF_FAKE_GH_OUT` or, when `OSF_FAKE_GH_ERR` is set,
/// prints that text to standard error and fails.
fn fake_gh() -> TempDir {
    let bin = TempDir::new("work-item-fake-gh");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let program = bin.join("gh");
        std::fs::write(
            &program,
            "#!/bin/sh\necho \"$@\" >> \"$OSF_FAKE_GH_LOG\"\nif [ -n \"$OSF_FAKE_GH_ERR\" ]; then echo \"$OSF_FAKE_GH_ERR\" 1>&2; exit 1; fi\ncat \"$OSF_FAKE_GH_OUT\"\n",
        )
        .expect("fake gh writes");
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
            .expect("fake gh chmod");
    }
    #[cfg(windows)]
    std::fs::write(
        bin.join("gh.cmd"),
        "@echo off\r\necho %* >> \"%OSF_FAKE_GH_LOG%\"\r\nif defined OSF_FAKE_GH_ERR (\r\n  echo %OSF_FAKE_GH_ERR% 1>&2\r\n  exit /b 1\r\n)\r\ntype \"%OSF_FAKE_GH_OUT%\"\r\n",
    )
    .expect("fake gh writes");
    bin
}

/// What one `osf review work-item` run left behind.
struct Run {
    output: std::process::Output,
    saved: Option<String>,
    log: String,
}

/// The head commit every `osf review work-item` run in this file records.
const HEAD: &str = "0123456789abcdef0123456789abcdef01234567";

/// Runs `osf review work-item` for a pull request whose body is `body`, with
/// the fake `gh` answering `issue_json`, or failing with `error` when it is `Some`.
fn run_work_item(label: &str, body: &str, issue_json: &str, error: Option<&str>) -> Run {
    let bin = fake_gh();
    let dir = TempDir::new(&format!("work-item-{label}"));
    let pr = dir.join("pull-request.json");
    std::fs::write(
        &pr,
        serde_json::json!({"number": 5, "title": "A change", "body": body}).to_string(),
    )
    .expect("pull request file writes");
    let answer = dir.join("answer.json");
    std::fs::write(&answer, issue_json).expect("answer writes");
    let log = dir.join("gh.log");
    let out = dir.join("work-item.json");
    let home = common::isolated_home(&format!("work-item-{label}"));
    let path = std::env::join_paths(std::iter::once(bin.to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .expect("PATH joins")
    .to_string_lossy()
    .into_owned();
    let answer_text = answer.to_string_lossy().into_owned();
    let log_text = log.to_string_lossy().into_owned();
    let mut env = vec![
        ("PATH", path.as_str()),
        ("OSF_FAKE_GH_OUT", answer_text.as_str()),
        ("OSF_FAKE_GH_LOG", log_text.as_str()),
    ];
    if let Some(error) = error {
        env.push(("OSF_FAKE_GH_ERR", error));
    }
    let output = common::run_osf_with_env(
        &dir,
        &home,
        &env,
        &[
            "review",
            "work-item",
            "--pull-request",
            &pr.to_string_lossy(),
            "--repository",
            "open-software-factory/software-factory",
            "--head",
            HEAD,
            "--out",
            &out.to_string_lossy(),
        ],
    );
    Run {
        output,
        saved: std::fs::read_to_string(&out).ok(),
        log: std::fs::read_to_string(&log).unwrap_or_default(),
    }
}

fn stdout_of(run: &Run) -> String {
    String::from_utf8_lossy(&run.output.stdout).into_owned()
}

const ISSUE: &str = r#"{"title": "Build the check", "body": "Why.\n\n## Done when\n- it works\n"}"#;

#[test]
fn the_work_item_is_the_issue_the_pull_request_names_on_its_issue_line() {
    let run = run_work_item("issue-line", "Issue: [open-software-factory/software-factory#137 (Build the check)](https://github.com/open-software-factory/software-factory/issues/137)\n\nText.", ISSUE, None);
    assert!(run.output.status.success(), "{}", stdout_of(&run));
    assert!(
        stdout_of(&run).contains("work item: open-software-factory/software-factory#137"),
        "{}",
        stdout_of(&run)
    );
    assert!(
        run.log
            .contains("api repos/open-software-factory/software-factory/issues/137"),
        "{}",
        run.log
    );
    let saved: serde_json::Value =
        serde_json::from_str(&run.saved.expect("saved")).expect("saved is JSON");
    assert_eq!(
        saved.get("issue").and_then(serde_json::Value::as_str),
        Some("open-software-factory/software-factory#137")
    );
    assert!(saved
        .get("body")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|b| b.contains("## Done when")));
    assert_eq!(
        saved.get("number").and_then(serde_json::Value::as_u64),
        Some(137)
    );
    assert_eq!(
        saved.get("head").and_then(serde_json::Value::as_str),
        Some(HEAD)
    );
}

#[test]
fn an_issue_the_pull_request_closes_is_the_work_item_when_no_issue_line_names_one() {
    let run = run_work_item("closes", "This change closes #44.", ISSUE, None);
    assert!(run.output.status.success(), "{}", stdout_of(&run));
    assert!(
        run.log
            .contains("api repos/open-software-factory/software-factory/issues/44"),
        "{}",
        run.log
    );
}

#[test]
fn a_pull_request_that_links_no_issue_saves_a_reason_that_says_to_link_one() {
    let run = run_work_item("none", "Only prose, no issue named.", ISSUE, None);
    assert!(run.output.status.success(), "{}", stdout_of(&run));
    assert!(run.log.is_empty(), "nothing was fetched: {}", run.log);
    let saved = run.saved.expect("a reason is saved");
    assert!(
        saved.contains("missing") && saved.contains("Link one"),
        "{saved}"
    );
}

#[test]
fn an_issue_that_cannot_be_found_saves_a_reason() {
    let run = run_work_item("gone", "Issue: #9", "", Some("gh: Not Found (HTTP 404)"));
    assert!(run.output.status.success(), "{}", stdout_of(&run));
    let saved = run.saved.expect("a reason is saved");
    assert!(saved.contains("was not found"), "{saved}");
}

#[test]
fn a_failing_code_host_stops_the_command_and_saves_nothing() {
    let run = run_work_item(
        "denied",
        "Issue: #9",
        "",
        Some("gh: Bad credentials (HTTP 401)"),
    );
    assert_eq!(run.output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&run.output.stderr).into_owned();
    assert!(stderr.contains("HTTP 401"), "{stderr}");
    assert!(run.saved.is_none(), "nothing is saved on a failure");
}

/// A repository with one committed change, ready to diff against `origin/main`.
fn changed_repo(name: &str) -> TempRepo {
    let repo = TempRepo::new(name);
    repo.write("src/lib.rs", "fn one() {}\n");
    repo.commit("base");
    repo.track_origin_main();
    repo.write("src/lib.rs", "fn one() {}\nfn two() {}\n");
    repo.commit("add a function");
    repo
}

/// The spec and acceptance lens from the shipped catalogue, and the context it builds from `saved`.
fn spec_context(repo: &TempRepo, saved: &Path) -> Result<String, String> {
    let catalogue = osf::lenses::load(&repo.dir, None).expect("the shipped catalogue loads");
    let lens = catalogue
        .lenses
        .iter()
        .find(|l| l.name == "spec-and-acceptance")
        .expect("the shipped spec and acceptance lens");
    osf::review_context::build(
        lens,
        &osf::review_context::Sources {
            root: &repo.dir,
            config_root: &repo.dir,
            base: "origin/main",
            work_item: Some(saved),
            pull_request: None,
            work_item_binding: None,
        },
    )
}

/// The saved work item is what the shipped spec and acceptance lens needs, so
/// the lens can run for a pull request that links an issue with acceptance criteria.
#[test]
fn the_saved_work_item_lets_the_shipped_spec_lens_build_its_context() {
    let run = run_work_item("lens-found", "Issue: #137", ISSUE, None);
    let repo = changed_repo("work-item-lens-found");
    let saved = repo.dir.join("work-item.json");
    std::fs::write(&saved, run.saved.expect("saved")).expect("copy writes");
    let context = spec_context(&repo, &saved).expect("the lens builds its context");
    assert!(context.contains("## work item"), "{context}");
    assert!(context.contains("it works"), "{context}");
}

/// With no linked issue the shipped lens is could-not-run, and the reason says to link one.
#[test]
fn with_no_linked_issue_the_shipped_spec_lens_is_could_not_run_and_says_to_link_one() {
    let run = run_work_item("lens-missing", "Only prose.", ISSUE, None);
    let repo = changed_repo("work-item-lens-missing");
    let saved = repo.dir.join("work-item.json");
    std::fs::write(&saved, run.saved.expect("saved")).expect("copy writes");
    let err = spec_context(&repo, &saved).expect_err("the lens cannot build its context");
    assert!(
        err.contains("work-item") && err.contains("Link one"),
        "{err}"
    );
}

/// A lens that does not need the work item still builds when there is none.
#[test]
fn a_lens_that_needs_no_work_item_still_builds_when_none_is_linked() {
    let run = run_work_item("lens-other", "Only prose.", ISSUE, None);
    let repo = changed_repo("work-item-lens-other");
    let saved = repo.dir.join("work-item.json");
    std::fs::write(&saved, run.saved.expect("saved")).expect("copy writes");
    let catalogue = osf::lenses::load(&repo.dir, None).expect("the shipped catalogue loads");
    let lens = catalogue
        .lenses
        .iter()
        .find(|l| l.name == "correctness")
        .expect("the shipped correctness lens");
    let context = osf::review_context::build(
        lens,
        &osf::review_context::Sources {
            root: &repo.dir,
            config_root: &repo.dir,
            base: "origin/main",
            work_item: Some(&saved),
            pull_request: None,
            work_item_binding: None,
        },
    )
    .expect("builds");
    assert!(!context.contains("## work item"), "{context}");
}
