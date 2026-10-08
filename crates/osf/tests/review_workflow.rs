//! Checks on the review workflow file itself: what each job passes to `osf`.
//! The file is read as text, split into its jobs, and each job's text is checked.

use std::path::PathBuf;

/// The codex version this workflow names.
const CODEX_VERSION: &str = "0.154.0";

/// Environment a reviewer container needs beyond its own provider credential:
/// where it keeps its temporary files, and whether to keep the reviewer's home
/// for diagnosis. Neither carries a key.
const HOUSEKEEPING_ENV: &[&str] = &["TMPDIR", "OSF_KEEP_REVIEW_HOME"];

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
        running.len() >= 5,
        "the build job, the codex, claude and opencode reviewer jobs and the last job: {:?}",
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
                .filter(|var| {
                    let name = var.split('=').next().unwrap_or(var);
                    !HOUSEKEEPING_ENV.contains(&name)
                })
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
        !text.contains("CODEX_SHA256") && !text.contains("CODEX_BWRAP_SHA256"),
        "the workflow pins no codex release hash"
    );
    assert!(
        !text.contains("rust-v${CODEX_VERSION}"),
        "the workflow downloads no codex release"
    );
    let mut image_digests = 0;
    for line in text.lines() {
        for hex in hex64_runs(line) {
            assert!(
                line.contains("@sha256:"),
                "the only 64-hex value is the review image digest, found {hex} in {line}"
            );
            image_digests += 1;
        }
    }
    assert_eq!(image_digests, 1, "the image digest is the one 64-hex value");
}

#[test]
fn the_build_job_downloads_no_codex_release() {
    let build = job_body(&workflow_text(), "build");
    for forbidden in [
        "release-assets",
        "openai/codex/releases",
        "codex-release",
        "codex-bwrap",
        "bwrap",
    ] {
        assert!(
            !build.contains(forbidden),
            "the build job downloads no codex release: found `{forbidden}` in {build}"
        );
    }
}

#[test]
fn the_codex_job_runs_in_the_review_container() {
    let body = job_body(&workflow_text(), "review-codex");
    assert!(
        body.contains("docker run --rm") && body.contains("${{ env.REVIEW_IMAGE }}"),
        "the codex job starts the review image: {body}"
    );
    assert!(
        body.contains("docker login ghcr.io"),
        "the codex job logs in to the registry: {body}"
    );
    assert_eq!(
        permissions_of(&body),
        vec!["contents: read".to_string(), "packages: read".to_string()],
        "the codex job holds contents: read and packages: read"
    );
    assert_eq!(
        provider_secrets(&body),
        vec!["OPENAI_API_KEY".to_string()],
        "the codex job reads exactly one provider secret"
    );
    let envs: Vec<&str> = body
        .lines()
        .filter_map(|line| line.trim().strip_prefix("-e "))
        .map(str::trim)
        .filter(|var| {
            let name = var.split('=').next().unwrap_or(var);
            !HOUSEKEEPING_ENV.contains(&name)
        })
        .collect();
    assert_eq!(
        envs,
        vec!["CODEX_API_KEY"],
        "the container gets its one credential and the housekeeping variables"
    );
    for mount in ["/pr:/pr:ro", "/base:/base:ro", "/work-item:/work-item:ro"] {
        assert!(body.contains(mount), "the codex job mounts {mount}: {body}");
    }
    assert!(
        !body.contains("docker.sock"),
        "the codex job mounts no container runtime socket: {body}"
    );
}

#[test]
fn the_codex_job_allows_exactly_its_eight_hosts() {
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
        "chatgpt.com:443",
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    expected.sort();
    assert_eq!(hosts, expected, "the codex job names its eight hosts");
    assert!(
        !body.contains("release-assets"),
        "the codex job downloads no release: {body}"
    );
}

#[test]
fn the_codex_helper_and_the_sandbox_check_are_gone() {
    let text = workflow_text();
    for forbidden in [
        "bwrap",
        "codex sandbox",
        "apparmor_restrict_unprivileged_userns",
        "Check that the codex sandbox starts",
        "codex-resources",
        "CODEX_BWRAP",
    ] {
        assert!(
            !text.contains(forbidden),
            "the workflow holds no `{forbidden}`"
        );
    }
}

#[test]
fn the_codex_job_uses_no_privileged_setting() {
    let body = job_body(&workflow_text(), "review-codex");
    for forbidden in [
        "sudo",
        "sysctl",
        "--privileged",
        "--cap-add",
        "--security-opt",
        "--device",
        "docker.sock",
    ] {
        assert!(
            !body.contains(forbidden),
            "the codex job holds no `{forbidden}`: {body}"
        );
    }
}

#[test]
fn the_codex_command_runs_with_the_bypass_flag_in_the_container() {
    let codex = osf::agents::AGENTS
        .iter()
        .find(|a| a.name == "codex")
        .expect("codex is in the agent list");
    let review = codex.review.as_ref().expect("codex reviews");
    let in_container = review
        .in_container
        .as_ref()
        .expect("codex documents an in-container mode");
    assert_eq!(
        in_container.args,
        &["--dangerously-bypass-approvals-and-sandbox"],
        "codex runs with its own sandbox off, because the container is the wall"
    );
    assert!(
        review.read_only.is_none(),
        "codex has no read-only mode of its own"
    );
}

#[test]
fn the_codex_job_checks_the_image_carries_the_pinned_codex() {
    let body = job_body(&workflow_text(), "review-codex");
    assert!(
        body.contains("name: Check that the review image carries the pinned codex"),
        "the codex job names the version check step: {body}"
    );
    assert!(
        body.contains("docker run --rm ${{ env.REVIEW_IMAGE }} codex --version"),
        "the codex job asks the image for its codex version: {body}"
    );
    assert!(
        body.contains("codex-cli ${CODEX_VERSION}"),
        "the codex job compares against the pinned version: {body}"
    );
    assert!(
        body.contains("exit 1"),
        "the codex job fails on a version mismatch: {body}"
    );
}

#[test]
fn the_last_job_mints_the_verifier_apps_token() {
    let text = workflow_text();
    let last = job_body(&text, "review");
    assert!(
        last.contains("alone mints the verifier app's token"),
        "the last job's comment names the verifier app's token: {last}"
    );
    assert!(
        !text.contains("alone mints the code host's token"),
        "no line says the last job mints the code host's token"
    );
}

#[test]
fn the_workflow_header_matches_the_jobs() {
    let text = workflow_text();
    let header: Vec<&str> = text.lines().take_while(|line| *line != "on:").collect();
    let header = header.join("\n");
    assert!(
        !header.contains("only its own provider's network host"),
        "the header describes each reviewer's network list truthfully: {header}"
    );
    assert!(
        !header.contains("alone gets the code host's token"),
        "the header describes the last job's token truthfully: {header}"
    );
    assert!(
        header.contains("alone mints the verifier"),
        "the header says the last job alone mints the verifier app's token: {header}"
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

/// The `docker run` invocation whose command contains `command`.
fn container_before(body: &str, command: &str) -> String {
    let at = body
        .find(command)
        .unwrap_or_else(|| panic!("no `{command}` in {body}"));
    let start = body[..at]
        .rfind("docker run")
        .unwrap_or_else(|| panic!("`{command}` starts no container"));
    body[start..at + command.len()].to_string()
}

/// The runner owns its checkout; the build container writes only into `build-out`.
#[test]
fn the_build_job_builds_as_the_runner_user_into_a_writable_build_out() {
    let build = job_body(&workflow_text(), "build");
    assert!(
        build.contains("mkdir -p build-out") || build.contains("mkdir build-out"),
        "the build job makes the output folder first: {build}"
    );
    let cargo = unquoted(&container_before(&build, "cargo build"));
    for expected in [
        "--user $(id -u):$(id -g)",
        "${{ github.workspace }}/base:/base:ro",
        "${{ github.workspace }}/build-out:/build-out",
        "-e CARGO_HOME=/build-out/cargo-home",
        "-e CARGO_TARGET_DIR=/build-out/target",
    ] {
        assert!(
            cargo.contains(expected),
            "the cargo build container lacks `{expected}`: {cargo}"
        );
    }
    assert!(
        !cargo.contains("build-out:/build-out:ro"),
        "the build output mount is writable: {cargo}"
    );
}

/// The work item is read from a read-only `build-out` and written as the runner user.
#[test]
fn the_build_job_runs_the_work_item_command_from_the_read_only_build_out() {
    let build = job_body(&workflow_text(), "build");
    assert!(
        build.contains("path: build-out/target/release/osf"),
        "the osf-binary artifact comes from build-out: {build}"
    );
    let work_item = unquoted(&container_before(&build, "osf review work-item"));
    for expected in [
        "/build-out/target/release/osf review work-item",
        "${{ github.workspace }}/build-out:/build-out:ro",
        "--user $(id -u):$(id -g)",
    ] {
        assert!(
            work_item.contains(expected),
            "the work item container lacks `{expected}`: {work_item}"
        );
    }
}

/// Every container sees the runner's checkout as read-only.
#[test]
fn no_job_mounts_the_base_checkout_writable() {
    let text = workflow_text();
    let mut mounts = 0;
    for line in text.lines() {
        if let Some(rest) = line.split(":/base").nth(1) {
            mounts += 1;
            assert!(
                rest.starts_with(":ro"),
                "a mount of /base is writable: {}",
                line.trim()
            );
        }
    }
    assert!(mounts >= 6, "found {mounts} mounts of /base");
}

/// A job that mounts a writable `out` must make it writable for the image user.
#[test]
fn every_job_that_mounts_out_writable_chmods_it_for_the_image_user() {
    let text = workflow_text();
    let mut writable = 0;
    for (id, body) in jobs(&text) {
        if !body.contains("/out:/out\"") {
            continue;
        }
        writable += 1;
        assert!(
            body.contains("chmod -R a+rwX out"),
            "job {id} mounts out writable but does not chmod it: {body}"
        );
    }
    assert!(
        writable >= 4,
        "found {writable} jobs that mount out writable"
    );
}

/// The build job runs untrusted code, so it reads no secret but the automatic token.
#[test]
fn the_build_job_reads_only_the_automatic_token() {
    let build = job_body(&workflow_text(), "build");
    assert!(
        build.contains("secrets.GITHUB_TOKEN"),
        "the build job reads the automatic token: {build}"
    );
    assert!(
        provider_secrets(&build).is_empty(),
        "the build job reads only the automatic token: {:?}",
        provider_secrets(&build)
    );
}
