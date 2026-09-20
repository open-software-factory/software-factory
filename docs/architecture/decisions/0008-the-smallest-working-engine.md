# 0008: The smallest working engine takes one issue to one pull request

Status: accepted

Date: 2026-09-20

## Context

Six work items and three documents named "the smallest working engine" and none defined it. The domain model in decision 0005 is accepted and unbuilt. The choice of first job decides which parts of that model are tested first, and everything after it is built against that job.

Three shapes were considered. Verifying one change and recording the evidence would have tested the journal and the policy and left the run, the sandbox and the harness untouched. A journal with a console view over it would have tested less. Taking one work item to a pull request without a person tests the whole model at once, on one item, in one sandbox.

## Decision

The smallest working engine takes one ready work item and returns one pull request that is verified, reviewed and ready for a person to merge. No person acts between the start and the pull request unless the engine asks.

Inside the boundary: reading the work item from the tracker, starting the default sandbox on the local platform, running one coding agent on the item, running the repository's deterministic checks, one review round by a harness of a different model family, the pull request with its status block, the event journal with every run, check, review and finding, the policy of decision 0003, attention when the item needs a person, and a recap written back to the work item.

Outside the boundary: deployment, containment and recovery, more than one item at a time, remote platforms, merging without a person, the console's own features beyond reading the journal, cost attribution across runs, the agent-host protocol gateway, and canonical verification through moon, a build task runner the execution design adopts later.

Done means one ready work item in a real repository reaches in review with no person acting, the pull request carries the evidence, the journal validates and its head hash reproduces, a blocked run raises attention and stops, and every check re-runs to the same result.

The design is [`../smallest-working-engine.md`](../smallest-working-engine.md).

## Consequences

- The event schema is written first, because every component writes to the journal.
- The tracker, the sandbox, the harness and the forge each get one provider implementation behind an interface, and the engine's own code names none of them.
- The vocabulary draft's five questions are answered on paper in the design and confirmed against real runs once the engine runs.
- The second slice is parallel items and scheduling. The third is merging without a person, once the risk classification is calibrated.
