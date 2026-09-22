# 0013: The aggregation check

Status: accepted

Date: 2026-09-23

## Context

A pull request in an adopting repository has the adopter's own jobs, the factory's tagged tasks and a review. Something has to read them all and decide. The design is [the verification seam](../verification-seam.md). This record holds the decisions about that one required check.

## Options considered

**Which checks are required.** The factory's aggregation alone, the adopter's jobs alone, or both. Both was taken. Checks run in parallel and the aggregation runs last.

**What the aggregation reads from an adopter's job.** The job's conclusion, the result files it uploads, or both. Both was taken. The conclusion decides pass or fail. The files add counts and findings, and the test-count shrink check reads them. Result files are found by content rather than by a fixed path, and a job with a known conclusion and no readable file still counts as passed or failed.

**Where the aggregation runs.** As a workflow in the adopter's repository, or posted from outside through the factory's app identity. The workflow was taken. A job on the first code host can wait only on jobs in its own workflow, and an adopter's checks span several. A workflow that re-runs each time another finishes converges without polling and needs no new infrastructure. The app identity takes over when the factory runs as a service.

**What the aggregation shows.**

| Option | What it meant | Why it was set aside |
|---|---|---|
| One check run whose summary is the slot table, rendered from one journal event | Each check writes its verification event. The aggregation writes one checkpoint-complete event carrying the slot table as data, and the check run's summary renders that event. | Taken. One source, two views. |
| One check run per slot plus the final one | The code host's own checks list becomes the table, and a required-checks rule can name a slot. | One run per slot on every commit multiplies the noise, and the slot table still needs an event to be durable. |
| A pull-request comment updated in place | The comment is the primary surface and the check is only pass or fail. | It sits in the conversation and is one more thing to keep in step with the checks tab. |

**Reviews.** A separate mechanism beside the checks, or a check with a slot of its own. A check was taken. It carries the evidence grade reported, and a policy never merges on reported evidence alone. On the first code host the setting that requires every review thread to be resolved enforces it.

## Decision

The aggregation is the one required factory check on a pull request. It runs as a generated workflow in the adopter's repository after every workflow on the commit finishes. It lists the check runs on the commit and waits while one is still running. When every check is done it reads each conclusion, downloads the artifacts, hands each file to the reader that recognises its content, takes the check recogniser's slot states, applies levels and suppressions, writes the checkpoint-complete event, posts the one required check, and flushes the journal.

The readers detect a result file by content. The first formats are JUnit XML, TRX, xUnit XML, Cobertura, LCOV, JaCoCo, SARIF and CTRF. A glob in `osf.toml` narrows the scan.

The pull-request status block gets one line per slot only when the slot's state differs from the base branch.

A job that did not run because its path filter excluded the change is reported as not affected. A job that was cancelled or whose check run is absent for any other reason makes its slot unknown, and the aggregation fails. That is the read rule of [decision 0003](0003-deterministic-verification-is-authoritative.md) applied to a slot.

## Consequences

- The ruleset on main requires the aggregation, the adopter's own jobs, and resolved review threads.
- One concurrency group per commit lets one aggregation run at a time, and a later one supersedes.
- The aggregation cannot advance a state until the journal reaches its sink, per [decision 0009](0009-journal-store-and-sinks.md), so a flush failure in CI fails the aggregation.
- A result-file format enters as one reader with a corpus of real files, stored without extensions.
