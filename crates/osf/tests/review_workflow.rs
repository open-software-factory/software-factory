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
