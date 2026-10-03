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
