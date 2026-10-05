//! Checks on the review workflow file itself: what each job passes to `osf`.
//! The file is read as text, split into its jobs, and each job's text is checked.

use std::path::PathBuf;

/// The codex release this workflow names, and the sha256 of it and its helper.
const CODEX_VERSION: &str = "0.154.0";
const CODEX_SHA256: &str = "d7e18b2597ae8f242f5f31ee9e90deef48dbc9edd634d9868fb6435d08c07f02";
const CODEX_BWRAP_SHA256: &str = "1e6a0f2802c4199f81e1d3d9a962d64dc274693d8391f02d1f4ab457e57c4c38";

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

/// A job body with quotes removed, so a quoted command path still reads as a command.
fn unquoted(body: &str) -> String {
    body.replace('"', "")
}

/// The jobs that start `osf review run` or `osf review reduce`.
fn osf_review_jobs(text: &str) -> Vec<(String, String)> {
    jobs(text)
        .into_iter()
        .filter(|(_, body)| {
            let body = unquoted(body);
            body.contains("osf review run") || body.contains("osf review reduce")
        })
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
        .filter(|(_, body)| unquoted(body).contains("osf review run"))
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
    assert!(
        build.contains("--head ${{ github.event.pull_request.head.sha }}"),
        "the build job records the head commit: {build}"
    );
    assert!(build.contains("--out /work-item/work-item.json"), "{build}");
    assert!(
        build.contains("name: work-item\n          path: work-item/work-item.json"),
        "the build job keeps the work item as an artifact"
    );
}

/// The trigger types named under `on: pull_request_target:`.
fn trigger_types(text: &str) -> Vec<String> {
    let mut in_on = false;
    for line in text.lines() {
        if line == "on:" {
            in_on = true;
            continue;
        }
        if !in_on {
            continue;
        }
        if line.starts_with("permissions:") {
            break;
        }
        if let Some(rest) = line.trim().strip_prefix("types:") {
            let list = rest.trim().trim_start_matches('[').trim_end_matches(']');
            return list
                .split(',')
                .map(|name| name.trim().to_string())
                .collect();
        }
    }
    Vec::new()
}

#[test]
fn the_workflow_runs_again_when_the_pull_request_text_is_edited() {
    let types = trigger_types(&workflow_text());
    for name in [
        "opened",
        "synchronize",
        "reopened",
        "ready_for_review",
        "edited",
    ] {
        assert!(
            types.iter().any(|found| found == name),
            "missing {name} in {types:?}"
        );
    }
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
        if body.contains("docker run") {
            assert!(
                body.contains("-v \"${{ github.workspace }}/work-item:/work-item:ro\""),
                "job {id} does not mount the work item read-only"
            );
            assert!(
                body.contains("--work-item /work-item/work-item.json"),
                "job {id} does not pass the work item to osf"
            );
        } else {
            assert!(
                body.contains("--work-item \"$GITHUB_WORKSPACE/work-item/work-item.json\""),
                "job {id} does not pass the work item to osf"
            );
        }
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
        running.len() >= 4,
        "the build job, the claude and opencode reviewer jobs and the last job: {:?}",
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

/// A reviewer job passes only the credential its agent's `credential_env` names.
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
        let mut passed: Vec<&str> = if body.contains("docker run") {
            body.lines()
                .filter_map(|line| line.trim().strip_prefix("-e "))
                .map(str::trim)
                .collect()
        } else {
            env_secret_names(body)
        };
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

/// The environment variable names a job gives a `secrets.` value.
fn env_secret_names(body: &str) -> Vec<&str> {
    let mut names: Vec<&str> = Vec::new();
    for line in body.lines() {
        if let Some((name, value)) = line.trim().split_once(": ") {
            if value.contains("secrets.") && !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
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

/// Every 64-character hexadecimal run in `line`.
fn hex64_runs(line: &str) -> Vec<String> {
    line.split(|c: char| !c.is_ascii_hexdigit())
        .filter(|token| token.len() == 64)
        .map(str::to_string)
        .collect()
}

/// The entries under a job's own `permissions:` key.
fn permissions_of(body: &str) -> Vec<String> {
    let mut entries = Vec::new();
    let mut in_block = false;
    for line in body.lines() {
        if line == "    permissions:" {
            in_block = true;
            continue;
        }
        if !in_block {
            continue;
        }
        if line.starts_with("      ") && !line.starts_with("       ") && line.contains(':') {
            entries.push(line.trim().to_string());
        } else if !line.trim().is_empty() {
            break;
        }
    }
    entries
}

/// The hosts under the first `allowed-endpoints:` folded list in a job.
fn endpoints_of(body: &str) -> Vec<String> {
    let mut hosts = Vec::new();
    let mut in_list = false;
    for line in body.lines() {
        if line.trim_start().starts_with("allowed-endpoints:") {
            in_list = true;
            continue;
        }
        if !in_list {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent >= 12 {
            hosts.push(line.trim().to_string());
        } else {
            break;
        }
    }
    hosts
}

#[test]
fn the_codex_release_is_pinned_in_one_place() {
    let text = workflow_text();
    assert!(
        text.contains(&format!("CODEX_VERSION: \"{CODEX_VERSION}\"")),
        "the top-level env pins the codex version"
    );
    assert!(
        text.contains(&format!("CODEX_SHA256: \"{CODEX_SHA256}\"")),
        "the top-level env pins the codex sha256"
    );
    assert!(
        text.contains(&format!("CODEX_BWRAP_SHA256: \"{CODEX_BWRAP_SHA256}\"")),
        "the top-level env pins the bubblewrap helper sha256"
    );
    assert!(
        text.contains("rust-v${CODEX_VERSION}"),
        "the download URL uses the pinned version"
    );
    let mut release_hashes = 0;
    let mut helper_hashes = 0;
    let mut image_digests = 0;
    for line in text.lines() {
        for hex in hex64_runs(line) {
            if hex.as_str() == CODEX_SHA256 {
                release_hashes += 1;
            } else if hex.as_str() == CODEX_BWRAP_SHA256 {
                helper_hashes += 1;
            } else if line.contains("@sha256:") {
                image_digests += 1;
            } else {
                panic!("unexpected 64-hex value {hex} in {line}");
            }
        }
    }
    assert_eq!(release_hashes, 1, "the codex release is pinned once");
    assert_eq!(helper_hashes, 1, "the bubblewrap helper is pinned once");
    assert_eq!(
        image_digests, 1,
        "the image digest is the one other 64-hex value"
    );
}

#[test]
fn the_build_job_downloads_and_keeps_the_codex_release() {
    let build = job_body(&workflow_text(), "build");
    assert!(
        build.contains("release-assets.githubusercontent.com:443"),
        "the release download redirects to that host: {build}"
    );
    assert!(build.contains("rust-v${CODEX_VERSION}"), "{build}");
    assert!(
        build.contains("bwrap-x86_64-unknown-linux-musl.tar.gz"),
        "the build job downloads the sandbox helper: {build}"
    );
    assert!(build.contains("sha256sum --check"), "{build}");
    assert!(
        build.contains(
            "echo \"${CODEX_BWRAP_SHA256}  codex-release/bwrap.tar.gz\" | sha256sum --check --strict -"
        ),
        "the build job checks the helper sha256: {build}"
    );
    assert!(
        build.contains("codex-release/bwrap.tar.gz"),
        "the build job checks the helper: {build}"
    );
    assert!(
        build.contains("name: codex-release\n          path: codex-release/codex.tar.gz"),
        "the build job keeps the codex release as an artifact"
    );
    assert!(
        build.contains("name: codex-bwrap\n          path: codex-release/bwrap.tar.gz"),
        "the build job keeps the sandbox helper as an artifact"
    );
}

#[test]
fn the_codex_job_checks_the_release_hash_before_it_extracts() {
    let body = job_body(&workflow_text(), "review-codex");
    let checks: Vec<usize> = body
        .match_indices("sha256sum --check")
        .map(|(at, _)| at)
        .collect();
    assert_eq!(
        checks.len(),
        2,
        "the codex job checks both release files: {body}"
    );
    let extracted = body
        .find("tar ")
        .expect("the codex job extracts the release");
    for check in checks {
        assert!(
            check < extracted,
            "the codex job checks every hash before it extracts: {body}"
        );
    }
    assert!(
        body.contains("codex-resources/bwrap"),
        "the codex job installs the sandbox helper next to codex: {body}"
    );
    assert!(
        body.contains(
            "echo \"${CODEX_BWRAP_SHA256}  codex-bwrap/bwrap.tar.gz\" | sha256sum --check --strict -"
        ),
        "the codex job checks the helper sha256: {body}"
    );
    assert!(
        body.contains("name: codex-bwrap\n          path: codex-bwrap"),
        "the codex job downloads the sandbox helper artifact: {body}"
    );
}

#[test]
fn the_codex_job_runs_on_the_host() {
    let body = job_body(&workflow_text(), "review-codex");
    assert!(
        !body.contains("docker"),
        "the codex job runs no container: {body}"
    );
    assert!(
        !body.contains("packages:"),
        "the codex job needs no package permission: {body}"
    );
    assert_eq!(
        permissions_of(&body),
        vec!["contents: read".to_string()],
        "the codex job holds only contents: read"
    );
    assert_eq!(
        provider_secrets(&body),
        vec!["OPENAI_API_KEY".to_string()],
        "the codex job reads exactly one provider secret"
    );
}

#[test]
fn the_codex_job_allows_only_its_seven_hosts() {
    let body = job_body(&workflow_text(), "review-codex");
    let mut hosts = endpoints_of(&body);
    hosts.sort();
    let mut expected: Vec<String> = [
        "github.com:443",
        "api.github.com:443",
        "codeload.github.com:443",
        "objects.githubusercontent.com:443",
        "ghcr.io:443",
        "pkg-containers.githubusercontent.com:443",
        "api.openai.com:443",
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    expected.sort();
    assert_eq!(hosts, expected, "the codex job names its seven hosts");
    assert!(
        !body.contains("release-assets"),
        "the codex job downloads no release: {body}"
    );
}

#[test]
fn the_codex_sandbox_check_is_documented_and_reports_the_reason() {
    let text = workflow_text();
    let body = job_body(&text, "review-codex");
    let name = "name: Check that the codex sandbox starts on this runner";
    let check = body.find(name).expect("the codex job checks the sandbox");
    let reviewer = body
        .find("name: Run this reviewer")
        .expect("the codex job runs the reviewer");
    assert!(
        check < reviewer,
        "the sandbox check comes before the reviewer"
    );
    let rest = &body[check..];
    let end = rest
        .find("\n      - name: ")
        .map_or(body.len(), |off| check + off);
    let step = &body[check..end];
    assert!(step.contains("codex sandbox -- true"), "{step}");
    assert!(
        step.contains("kernel.apparmor_restrict_unprivileged_userns"),
        "{step}"
    );
    assert!(step.contains("exit 1"), "{step}");
    assert!(
        !step.contains("env:"),
        "the sandbox check holds no env: {step}"
    );
    assert!(
        !step.contains("secrets."),
        "the sandbox check holds no secret: {step}"
    );
    assert!(
        text.contains("It does not change any kernel or AppArmor setting"),
        "the workflow documents the sandbox check"
    );
    assert!(
        text.contains("Codex starts its read-only sandbox with user namespaces"),
        "the workflow names the sandbox assumption"
    );
}

#[test]
fn the_workflow_uses_no_privileged_settings() {
    let text = workflow_text();
    for forbidden in [
        "sudo",
        "sysctl -w",
        "apparmor_parser",
        "--privileged",
        "seccomp=unconfined",
        "apparmor=unconfined",
    ] {
        assert!(
            !text.contains(forbidden),
            "the workflow holds no `{forbidden}`"
        );
    }
}
