# 0005: The factory domain model, events included

Status: accepted

Date: 2026-09-15. This record replaces the proposed record of 2026-09-07 that held the engine-language question open; that question is settled in the revised decision 0001.

## Context

Decision 0002 says the core owns the factory's semantics and providers are edges. Nothing yet said what those semantics are. The operator console's design work stalled on exactly this: its state vocabulary is marked provisional, to be re-derived from the domain model when it lands. Every verifier, reporter, policy and console view needs one shared model, or each invents its own.

Three open-source control planes were read before this record was written. Each solved one piece well: one separates "could not read" from "read nothing" and refuses vacuous green; one keeps a hash-chained run journal so identical runs are provably identical; one grades evidence so that only the top grade may carry human attribution and a lower grade cannot fake it. This model takes those pieces.

## Decision

The engine owns one domain model. Provider schemas are translated to it at the edges and never imported into it.

### Entities

| Entity | Holds |
|---|---|
| Work item | The durable unit. Lifecycle states: ready, in progress, verifying, in review, deploying, deployed, signed off, blocked, failed, recovering, and the explicit stops aborted, rolled back, paused. A blocked item names its cause: dependency, human, clarification, ambiguous, capacity. |
| Run | One execution against a work item by one actor. An actor is a harness, a model and a model family, or a person. |
| Verifier run | One deterministic check inside a run: command, tool version, environment, timing, exit status, and whether it ran at all. |
| Review run | One judgment pass: harness, model family, round number, the scope it read. |
| Finding | A located claim from a verifier or a review, with severity, an action bucket, and an evidence grade. |
| Repository, module, deploy target, environment, external dependency | Structure the work touches. |

Edges: executes, depends on, blocks, produces, verifies, deploys to, traced from. No view holds every edge at once; a view chooses which edges it renders.

### Evidence grades

Every finding and every summary carries one grade:

| Grade | Meaning |
|---|---|
| Observed | A verifier measured it. |
| Derived | Computed from observed evidence. |
| Reported | An agent or a person said so, unmeasured. |
| Unverified | No deterministic confirmation yet. |

Only observed evidence may carry a "verified by" attribution. A finding at a lower grade that claims one is rejected as malformed, not accepted with a warning. A policy may refuse to act on reported evidence.

### Events are the model

A state change is an event. The entity tables above are projections of the event stream, not a second store. One envelope, versioned, one event per line:

| Field | Holds |
|---|---|
| schema version | The envelope version, required. |
| event type | One of: run started, verification, review, finding, state change, run complete, attention. |
| run, work item, change | Identifiers, provider-qualified. |
| actor | Harness, model, model family, or the person. |
| timestamp | Wall-clock time, excluded from the run hash below. |
| cost | Money and tokens, when known. |
| payload | The event type's own fields. A verification carries the check name, the check type, the result, the duration, a summary, and the evidence grade. |

A reporter validates every event against the schema and rejects an invalid one loudly. An event is never dropped silently.

### Findings with a location use SARIF

A finding that names a file and a line is written in SARIF 2.1.0, because the language tools the factory wraps already emit it. A finding's identity is a hash of rule, path, line and column, never of its message, so rewording does not create a new finding. Test results are not findings and take their own path; no test framework emits SARIF.

### Runs are replayable

A run's event journal is hash-chained: each event carries the hash of the one before, with wall-clock time excluded. Two runs with identical inputs and identical decisions produce an identical head hash, which is how a replay proves it replayed.

## Consequences

- The event schema is the first contract written, before any verifier, and lives beside the code that emits it.
- The console's provisional state vocabulary is replaced by the lifecycle and grades above.
- A provider adapter's fidelity is stated in its own terms: which entities it can read, which it cannot, and which edges it drops, following the read rule in decision 0003.
- The model says nothing about orchestration, sandboxes or the work-item provider. Those are edges, recorded when their first adapter ships, per decision 0002.
