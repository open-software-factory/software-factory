//! Integration tests for assembling a review lens's metadata: one throwaway
//! git repository per case, so `osf::review_context::build` sees exactly
//! what a real change under review would look like.

mod common;

use common::{fake_forge_token, suppress_marker, TempDir, TempRepo};
use osf::lenses::{ContextInput, Criterion, Lens, Runs, SeverityGuide, Trigger};
use osf::review_context::{build, PullRequest, Sources, WorkItemBinding};

/// A lens built only to carry a `context` list; its criteria and severity
/// guide are never read by `build`.
fn lens_with(context: Vec<ContextInput>) -> Lens {
    Lens {
        name: "test-lens".to_string(),
        summary: "a lens built for a review-context test".to_string(),
        criteria: vec![Criterion {
            id: "c1".to_string(),
            question: "q".to_string(),
        }],
        severity_guide: SeverityGuide {
            blocker: "b".to_string(),
            major: "m".to_string(),
            minor: "n".to_string(),
        },
        weight: 1.0,
        runs: Runs::Always,
        trigger: Trigger::default(),
        context,
    }
}

/// A repository with one committed change ready to diff against `origin/main`.
fn changed_repo(name: &str) -> TempRepo {
    let repo = TempRepo::new(name);
    repo.write("src/lib.rs", "fn one() {}\n");
    repo.write("README.md", "base\n");
    repo.commit("base");
    repo.track_origin_main();
    repo.write("src/lib.rs", "fn one() {}\nfn two() {}\n");
    repo.commit("add a function");
    repo
}

fn sources<'a>(repo: &'a TempRepo, work_item: Option<&'a std::path::Path>) -> Sources<'a> {
    Sources {
        root: &repo.dir,
        config_root: &repo.dir,
        base: "origin/main",
        work_item,
        pull_request: None,
        work_item_binding: None,
    }
}

fn pull_request(body: &str) -> PullRequest {
    PullRequest {
        number: 178,
        title: "Add the review check".to_string(),
        body: Some(body.to_string()),
    }
}

#[test]
fn a_lens_with_no_work_item_file_fails_naming_work_item() {
    let repo = changed_repo("no-work-item");
    let lens = lens_with(vec![
        ContextInput::WorkItem,
        ContextInput::AcceptanceCriteria,
    ]);
    let err = build(&lens, &sources(&repo, None)).expect_err("must fail");
    assert!(err.contains("work-item"), "{err}");
}

#[test]
fn a_work_item_with_no_acceptance_heading_fails_naming_acceptance_criteria() {
    let repo = changed_repo("no-acceptance-heading");
    let work_item = repo.dir.join("issue.md");
    std::fs::write(
        &work_item,
        "# Some work item\n\nA description with no acceptance section at all.\n",
    )
    .expect("work item writes");
    let lens = lens_with(vec![
        ContextInput::WorkItem,
        ContextInput::AcceptanceCriteria,
    ]);
    let err = build(&lens, &sources(&repo, Some(&work_item))).expect_err("must fail");
    assert!(err.contains("acceptance-criteria"), "{err}");
}

#[test]
fn a_work_item_is_included_whole() {
    let repo = changed_repo("work-item-whole");
    let work_item = repo.dir.join("issue.md");
    std::fs::write(
        &work_item,
        "# Some work item\n\n## Done when\n- the thing happens\n\n## Notes\nalso kept\n",
    )
    .expect("work item writes");
    let lens = lens_with(vec![ContextInput::AcceptanceCriteria]);
    let ctx = build(&lens, &sources(&repo, Some(&work_item))).expect("builds");
    assert!(ctx.contains("## work item"), "{ctx}");
    assert!(ctx.contains("the thing happens"), "{ctx}");
    assert!(ctx.contains("also kept"), "{ctx}");
}

#[test]
fn the_metadata_names_the_pull_request_both_commits_and_each_changed_file() {
    let repo = changed_repo("metadata");
    let base = repo.git(&["rev-parse", "origin/main"]);
    let head = repo.git(&["rev-parse", "HEAD"]);
    let pr = pull_request("What this pull request does.");
    let mut sources = sources(&repo, None);
    sources.pull_request = Some(&pr);
    let ctx = build(&lens_with(Vec::new()), &sources).expect("builds");
    assert!(ctx.contains("number: 178"), "{ctx}");
    assert!(ctx.contains("title: Add the review check"), "{ctx}");
    assert!(ctx.contains("What this pull request does."), "{ctx}");
    assert!(ctx.contains(&format!("base: {}", base.trim())), "{ctx}");
    assert!(ctx.contains(&format!("head: {}", head.trim())), "{ctx}");
    assert!(ctx.contains("src/lib.rs (+1 -0)"), "{ctx}");
}

#[test]
fn the_metadata_holds_no_diff_and_no_file_content() {
    let repo = changed_repo("no-diff");
    let ctx = build(&lens_with(vec![ContextInput::Diff]), &sources(&repo, None)).expect("builds");
    assert!(ctx.contains("src/lib.rs"), "{ctx}");
    assert!(!ctx.contains("fn two"), "{ctx}");
    assert!(!ctx.contains("@@"), "{ctx}");
}

#[test]
fn a_change_with_no_changed_files_fails() {
    let repo = TempRepo::new("no-changed-files");
    repo.write("README.md", "base\n");
    repo.commit("base");
    repo.track_origin_main();
    let err = build(&lens_with(Vec::new()), &sources(&repo, None)).expect_err("must fail");
    assert!(err.contains("no changed files"), "{err}");
}

#[test]
fn a_diff_that_touches_no_decision_record_says_so() {
    let repo = changed_repo("no-decision-record-touched");
    let ctx = build(&lens_with(Vec::new()), &sources(&repo, None)).expect("builds");
    assert!(ctx.contains("none touched or linked"), "{ctx}");
}

#[test]
fn a_touched_decision_record_is_listed_by_path_only() {
    let repo = TempRepo::new("touched-decision-record");
    repo.write("README.md", "base\n");
    repo.commit("base");
    repo.track_origin_main();
    repo.write(
        "docs/architecture/decisions/0001-example.md",
        "# 0001: an example decision\n\nBody text unique to this record.\n",
    );
    repo.commit("add a decision record");
    let ctx = build(&lens_with(Vec::new()), &sources(&repo, None)).expect("builds");
    assert!(
        ctx.contains("## decision records\ndocs/architecture/decisions/0001-example.md"),
        "{ctx}"
    );
    assert!(!ctx.contains("Body text unique"), "{ctx}");
}

#[test]
fn a_linked_decision_record_that_does_not_exist_is_marked_not_found() {
    let repo = TempRepo::new("missing-decision-record");
    repo.write("src/lib.rs", "// no reference yet\n");
    repo.commit("base");
    repo.track_origin_main();
    repo.write(
        "src/lib.rs",
        "// see docs/architecture/decisions/0099-does-not-exist.md\n",
    );
    repo.commit("reference a missing decision record");
    let ctx = build(&lens_with(Vec::new()), &sources(&repo, None)).expect("builds");
    assert!(
        ctx.contains(
            "docs/architecture/decisions/0099-does-not-exist.md (not found in this checkout)"
        ),
        "{ctx}"
    );
}

#[test]
fn every_rendered_path_is_forward_slash() {
    let repo = TempRepo::new("forward-slash-paths");
    repo.write("src/nested/dir/lib.rs", "fn one() {}\n");
    repo.commit("base");
    repo.track_origin_main();
    repo.write("src/nested/dir/lib.rs", "fn one() {}\nfn changed() {}\n");
    repo.commit("change nested file");
    let ctx = build(&lens_with(Vec::new()), &sources(&repo, None)).expect("builds");
    assert!(ctx.contains("src/nested/dir/lib.rs"), "{ctx}");
    assert!(!ctx.contains('\\'), "{ctx}");
}

#[test]
fn a_fake_secret_in_the_pull_request_body_is_redacted() {
    let repo = changed_repo("redact-fake-secret");
    repo.write(
        "osf.toml",
        "[scan]\ndenylist = [\"sk-fake-secret-[0-9]+\"]\n",
    );
    let secret = "sk-fake-secret-90210";
    let pr = pull_request(&format!("my key is {secret}"));
    let mut sources = sources(&repo, None);
    sources.pull_request = Some(&pr);
    let ctx = build(&lens_with(Vec::new()), &sources).expect("builds");
    assert!(!ctx.contains(secret), "{ctx}");
    assert!(ctx.contains("redacted"), "{ctx}");
}

#[test]
fn a_built_in_secret_shape_in_the_work_item_is_redacted_with_no_configuration() {
    let repo = changed_repo("redact-built-in-secret");
    let secret = fake_forge_token("ghp_");
    let work_item = repo.dir.join("issue.md");
    std::fs::write(&work_item, format!("# Item\n\ntoken: {secret}\n")).expect("work item writes");
    let ctx = build(&lens_with(Vec::new()), &sources(&repo, Some(&work_item))).expect("builds");
    assert!(!ctx.contains(&secret), "{ctx}");
    assert!(ctx.contains("redacted by scan-secret"), "{ctx}");
}

/// A suppression marker silences a finding for a human reading a check's
/// output, never a secret heading into a reviewer's own prompt: redaction
/// must never consult the marker at all.
#[test]
fn a_suppression_marker_never_stops_a_secret_from_being_redacted() {
    let repo = changed_repo("redact-ignores-suppression-marker");
    let secret = fake_forge_token("ghp_");
    let marker = suppress_marker("disable-line", "scan-secret", Some("test fixture"));
    let pr = pull_request(&format!("let leaked = \"{secret}\"; {marker}"));
    let mut sources = sources(&repo, None);
    sources.pull_request = Some(&pr);
    let ctx = build(&lens_with(Vec::new()), &sources).expect("builds");
    assert!(!ctx.contains(&secret), "{ctx}");
    assert!(ctx.contains("redacted by scan-secret"), "{ctx}");
}

/// A pull request's own `osf.toml` cannot loosen redaction by naming the
/// `[scan]` table there: this repository's own attempt to turn off
/// `scan-secret` has no effect, because `redact_secrets` reads `[scan]`
/// from `config_root`, never from `root`.
#[test]
fn a_pull_requests_own_osf_toml_cannot_turn_off_scan_secret() {
    let repo = changed_repo("redact-scan-secret-off-in-pr");
    repo.write("osf.toml", "[scan.levels]\nscan-secret = \"off\"\n");
    let secret = fake_forge_token("ghp_");
    let config_root = TempDir::new("redact-scan-secret-off-in-pr-base");
    let pr = pull_request(&format!("token {secret}"));
    let sources = Sources {
        root: &repo.dir,
        config_root: &config_root,
        base: "origin/main",
        work_item: None,
        pull_request: Some(&pr),
        work_item_binding: None,
    };
    let ctx = build(&lens_with(Vec::new()), &sources).expect("builds");
    assert!(!ctx.contains(&secret), "{ctx}");
    assert!(ctx.contains("redacted by scan-secret"), "{ctx}");
}

/// The same property for a setting that genuinely changes what redaction
/// catches: `project_owner`. A pull request that names its own reference's
/// owner as the project owner would make that reference read as its own,
/// not foreign, and escape redaction, if its own `osf.toml` governed. It
/// does not, because `config_root`'s `project_owner` is what `redact_secrets`
/// actually uses.
#[test]
fn a_pull_requests_own_project_owner_cannot_hide_a_foreign_reference() {
    let repo = changed_repo("redact-project-owner-in-pr");
    repo.write("osf.toml", "[scan]\nproject_owner = \"evil-org\"\n");
    let reference = common::foreign_reference("evil-org", "other-repo", 42);
    let config_root = TempDir::new("redact-project-owner-in-pr-base");
    std::fs::write(
        config_root.join("osf.toml"),
        "[scan]\nproject_owner = \"the-real-owner\"\n",
    )
    .expect("base osf.toml writes");
    let pr = pull_request(&format!("see {reference} for context"));
    let sources = Sources {
        root: &repo.dir,
        config_root: &config_root,
        base: "origin/main",
        work_item: None,
        pull_request: Some(&pr),
        work_item_binding: None,
    };
    let ctx = build(&lens_with(Vec::new()), &sources).expect("builds");
    assert!(!ctx.contains(&reference), "{ctx}");
    assert!(ctx.contains("redacted by scan-foreign-reference"), "{ctx}");
}

/// A saved work item for another commit than the reviewer run is for is
/// refused, and the reason names both commits.
#[test]
fn a_work_item_bound_to_another_commit_is_refused_by_name() {
    let repo = changed_repo("work-item-binding");
    let work_item = repo.dir.join("issue.json");
    let head = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let other = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    std::fs::write(
        &work_item,
        serde_json::json!({
            "issue": "open-software-factory/software-factory#137",
            "number": 137,
            "head": head,
            "title": "Build the check",
            "body": "## Done when\n- it works\n",
        })
        .to_string(),
    )
    .expect("work item writes");
    let pr = pull_request("Issue: #137");
    let mut src = sources(&repo, Some(&work_item));
    src.pull_request = Some(&pr);
    src.work_item_binding = Some(WorkItemBinding {
        repository: "open-software-factory/software-factory",
        head: other,
    });
    let err = build(&lens_with(vec![ContextInput::WorkItem]), &src).expect_err("must fail");
    assert!(err.contains(head) && err.contains(other), "{err}");
}

/// An acceptance heading with no text under it is could-not-run for the
/// shipped spec and acceptance lens, and the reason says the section is empty.
#[test]
fn an_empty_acceptance_section_fails_the_spec_and_acceptance_lens() {
    let repo = changed_repo("empty-acceptance-section");
    let work_item = repo.dir.join("issue.md");
    std::fs::write(
        &work_item,
        "# Some work item\n\n## Done when\n\n## Notes\nmore text\n",
    )
    .expect("work item writes");
    let catalogue = osf::lenses::load(&repo.dir, None).expect("the shipped catalogue loads");
    let lens = catalogue
        .lenses
        .iter()
        .find(|l| l.name == "spec-and-acceptance")
        .expect("the shipped spec and acceptance lens");
    let err = build(lens, &sources(&repo, Some(&work_item))).expect_err("must fail");
    assert!(
        err.contains("acceptance-criteria") && err.contains("empty"),
        "{err}"
    );
}
