//! Checks on the review workflow file itself: what each job passes to `osf`.
//! The file is read as text, split into its jobs, and each job's text is checked.

use std::path::PathBuf;

fn workflow_text() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/review.yml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Each job in `text` as its id and its own lines, in file order.
fn jobs(text: &str) -> Vec<(String, String)> {
    let mut jobs: Vec<(String, String)> = Vec::new();
    let mut in_jobs = false;
    for line in text.lines() {
        if line == "jobs:" {
            in_jobs = true;
            continue;
        }
        if !in_jobs {
            continue;
        }
        let is_job_key = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('#');
        if is_job_key {
            jobs.push((line.trim().trim_end_matches(':').to_string(), String::new()));
        } else if let Some((_, body)) = jobs.last_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    jobs
}

/// The jobs that start `osf review run` or `osf review reduce`.
fn osf_review_jobs(text: &str) -> Vec<(String, String)> {
    jobs(text)
        .into_iter()
        .filter(|(_, body)| body.contains("osf review run") || body.contains("osf review reduce"))
        .collect()
}

#[test]
fn every_job_that_runs_or_reduces_a_review_passes_the_run_binding() {
    let text = workflow_text();
    let review_jobs = osf_review_jobs(&text);
    assert!(
        review_jobs.len() >= 4,
        "three reviewer jobs and the last job: {:?}",
        review_jobs.iter().map(|(id, _)| id).collect::<Vec<_>>()
    );
    for (id, body) in &review_jobs {
        for flag in [
            "--repository ${{ github.repository }}",
            "--pull-request-number ${{ github.event.pull_request.number }}",
            "--head ${{ github.event.pull_request.head.sha }}",
            "--ci-run-id ${{ github.run_id }}",
        ] {
            assert!(body.contains(flag), "job {id} lacks `{flag}`");
        }
    }
}

fn job_body(text: &str, id: &str) -> String {
    jobs(text)
        .into_iter()
        .find(|(job, _)| job == id)
        .unwrap_or_else(|| panic!("no job named {id}"))
        .1
}

/// The jobs that start one reviewer.
fn reviewer_jobs(text: &str) -> Vec<(String, String)> {
    jobs(text)
        .into_iter()
        .filter(|(_, body)| body.contains("osf review run"))
        .collect()
}

#[test]
fn the_build_job_finds_the_work_item_with_read_only_access() {
    let text = workflow_text();
    let build = job_body(&text, "build");
    assert!(build.contains("issues: read"), "{build}");
    assert!(build.contains("pull-requests: read"), "{build}");
    assert!(
        !build.contains(": write"),
        "the build job's permissions are read-only"
    );
    assert!(build.contains("osf review work-item"), "{build}");
    assert!(build.contains("--out /work-item/work-item.json"), "{build}");
    assert!(
        build.contains("name: work-item\n          path: work-item/work-item.json"),
        "the build job keeps the work item as an artifact"
    );
}

#[test]
fn every_reviewer_job_receives_the_saved_work_item() {
    let text = workflow_text();
    let reviewers = reviewer_jobs(&text);
    assert!(
        reviewers.len() >= 3,
        "{:?}",
        reviewers.iter().map(|(id, _)| id).collect::<Vec<_>>()
    );
    for (id, body) in &reviewers {
        assert!(
            body.contains("name: work-item\n          path: work-item"),
            "job {id} does not download the work item"
        );
        assert!(
            body.contains("-v \"${{ github.workspace }}/work-item:/work-item:ro\""),
            "job {id} does not mount the work item read-only"
        );
        assert!(
            body.contains("--work-item /work-item/work-item.json"),
            "job {id} does not pass the work item to osf"
        );
    }
    let last = job_body(&text, "review");
    assert!(
        !last.contains("work-item"),
        "the last job reads no work item, so it never receives one"
    );
}

#[test]
fn the_review_image_is_named_once_and_pinned_by_digest() {
    let text = workflow_text();
    let named: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("devcontainer@") || line.contains("/devcontainer:"))
        .collect();
    assert_eq!(named.len(), 1, "the image is named in one place: {named:?}");
    let line = named.first().expect("one line");
    let digest = line
        .split("@sha256:")
        .nth(1)
        .unwrap_or_else(|| panic!("the image is pinned by digest: {line}"));
    assert!(
        digest.trim().len() == 64 && digest.trim().chars().all(|c| c.is_ascii_hexdigit()),
        "a sha256 digest has 64 hex digits: {digest}"
    );
    assert!(
        line.trim_start().starts_with("REVIEW_IMAGE:"),
        "the one place is the REVIEW_IMAGE variable: {line}"
    );
    assert!(
        text.contains("edits this value") && text.contains("reviewed like any workflow change"),
        "the comment says how to update the digest through a reviewed change"
    );
}

/// Whether `line` is a command that starts a container, not a comment about one.
fn starts_a_container(line: &str) -> bool {
    line.trim_start()
        .trim_start_matches("run: >-")
        .trim_start()
        .starts_with("docker run")
}

#[test]
fn every_job_that_runs_a_container_runs_the_pinned_image() {
    let text = workflow_text();
    let running: Vec<(String, String)> = jobs(&text)
        .into_iter()
        .filter(|(_, body)| body.lines().any(starts_a_container))
        .collect();
    assert!(
        running.len() >= 5,
        "the build job, three reviewer jobs and the last job: {:?}",
        running.iter().map(|(id, _)| id).collect::<Vec<_>>()
    );
    for (id, body) in &running {
        let runs = body.lines().filter(|line| starts_a_container(line)).count();
        let images = body.matches("${{ env.REVIEW_IMAGE }}").count();
        assert!(
            images >= runs,
            "job {id} starts a container from another image"
        );
    }
}

/// The reviewer an `osf review run` job runs: the word after `--reviewer`.
fn reviewer_of(body: &str) -> String {
    body.lines()
        .find_map(|line| line.trim().strip_prefix("--reviewer "))
        .unwrap_or_else(|| panic!("no --reviewer in {body}"))
        .trim()
        .to_string()
}

/// Every `secrets.NAME` the job reads, other than the automatic token, once each.
fn provider_secrets(body: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for part in body.split("secrets.").skip(1) {
        let name: String = part
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name != "GITHUB_TOKEN" && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// A reviewer job reads one secret, and passes into its container only the
/// variables that its agent's `credential_env` names.
#[test]
fn each_reviewer_job_passes_only_the_credential_its_agent_names() {
    let text = workflow_text();
    let reviewers = reviewer_jobs(&text);
    assert!(reviewers.len() >= 3);
    for (id, body) in &reviewers {
        let name = reviewer_of(body);
        let agent = osf::agents::AGENTS
            .iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("job {id}: no agent named {name}"));
        let mut expected: Vec<&str> = agent
            .review
            .as_ref()
            .unwrap_or_else(|| panic!("job {id}: {name} cannot review"))
            .credential_env
            .to_vec();
        expected.sort_unstable();
        let mut passed: Vec<&str> = body
            .lines()
            .filter_map(|line| line.trim().strip_prefix("-e "))
            .map(str::trim)
            .collect();
        passed.sort_unstable();
        assert_eq!(
            passed, expected,
            "job {id} passes a variable its agent does not name"
        );
        assert_eq!(
            provider_secrets(body).len(),
            1,
            "job {id} reads one provider secret: {:?}",
            provider_secrets(body)
        );
    }
}

fn docs_text(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The `if:` line of a job.
fn condition_of(body: &str) -> String {
    body.lines()
        .find(|line| line.starts_with("    if:"))
        .unwrap_or_else(|| panic!("no job-level if in {body}"))
        .to_string()
}

#[test]
fn the_review_check_is_advisory_in_the_workflow_and_the_guide() {
    let text = workflow_text();
    assert!(
        text.contains("This check is advisory")
            && text.contains("no branch\n# protection or ruleset requires it"),
        "the workflow says the check is advisory and that nothing requires it"
    );
    assert!(
        text.contains("key\n# proxy and the network split"),
        "the workflow says when it becomes required"
    );
    let guide = docs_text("development.md");
    assert!(
        guide.contains("### The review check is advisory"),
        "{guide}"
    );
    assert!(
        !guide.contains("Branch protection that requires the `review` job"),
        "the guide no longer asks for the review job to be required"
    );
}

/// A fork's pull request is reviewed only when the repository variable says
/// so, and nothing in the file sets that variable.
#[test]
fn reviews_are_off_for_forks_unless_the_variable_is_set_and_it_is_never_set_here() {
    let text = workflow_text();
    let all = jobs(&text);
    let same_or_enabled =
        "github.event.pull_request.head.repo.full_name == github.repository || vars.OSF_REVIEW_FORKS == 'true'";
    for id in [
        "build",
        "review-codex",
        "review-claude",
        "review-opencode",
        "review",
    ] {
        let body = job_body(&text, id);
        assert!(
            condition_of(&body).contains(same_or_enabled),
            "job {id} runs for a fork only when OSF_REVIEW_FORKS is true"
        );
    }
    let disabled = job_body(&text, "review-fork-disabled");
    let condition = condition_of(&disabled);
    assert!(
        condition.contains("head.repo.full_name != github.repository")
            && condition.contains("vars.OSF_REVIEW_FORKS != 'true'"),
        "{condition}"
    );
    assert!(all.len() >= 6);
    assert!(
        !text.contains("OSF_REVIEW_FORKS ||") && !text.contains("OSF_REVIEW_FORKS: "),
        "the workflow gives the variable no default"
    );
}
