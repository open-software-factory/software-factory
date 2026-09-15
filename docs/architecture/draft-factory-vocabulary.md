# Draft factory vocabulary

Status: draft hypothesis, not a decision

Date: 2026-09-05

## Why this document exists

The UX benchmark work in August 2026 built a synthetic factory to render an operator console. To do that, it had to name the entities, states, relationships and events an operator sees. That vocabulary is the first concrete one this project has written down.

The benchmark is dropped. This document keeps the vocabulary so the first Factory Engine slice can test it, keep what fits and delete the rest. It is a hypothesis. The Engine slice decides the real vocabulary, as `todo.md` requires.

The vocabulary below reflects the schema and fixture used in the dropped August 2026 benchmark work. That work, including its later changes, is not included in this repository.

## Entity families

Every entity has a stable string ID. Relationships use IDs, never names.

| Family | Fields | Status values |
|---|---|---|
| Repository | id, name, purpose | healthy, attention-required, stale |
| Intent | id, title, summary | none |
| Work item | id, title, category (epic, feature, story, task, bug, chore), repository, dependsOn | ready, active, blocked, verifying, completed |
| Agent session | id, repository, work item, operation | running, waiting, verifying |
| Pull request | id, repository, work item, check IDs, review state (awaiting-review, approved, changes-requested) | open |
| Verification check | id, repository, work item, pull request, phase (pre-deployment, post-deployment), command | passed, failed, running, pending |
| Runner | id, platform (windows-x64, linux-x64, linux-arm64, macos-arm64), location (local, remote), current work item | available, busy, stale |
| Deployment | id, repository, work item, pull request, check IDs, environment (staging, production) | pending, deploying, deployed, rolled-back |
| Attention item | id, severity (critical, high, medium, low), title, related IDs | active or resolved |
| Evidence | id, kind (intent, specification, verification, production-outcome), summary, related IDs | known or not yet known |
| Trace link | id, from (kind, id), to (kind, id), relation (informs, implements, verifies, deploys, observes) | none |
| Design decision | id, title, rationale, intent, related pull requests | none |
| Operator choice | id, design decision, action, impact, confidence, reversibility, missing knowledge | verified |
| Containment action | id, type (rollout-halted, generation-paused, queue-quarantined, snapshot-preserved), related IDs | completed |
| Metric series | id, label, unit, timestamped points | none |
| Event | id, type, at, related IDs, summary | none |

Event types used: `work.stage.changed`, `agent.operation`, `artifact.emitted`, `gate.changed`, `deployment.changed`, `attention.raised`, `runner.stale`, `decision.requested`, `decision.approved`, `recovery.completed`.

## Relationships that the operator console needed

- Intent informs work. Work implements smaller work, then a pull request.
- A pull request is verified by checks. A passing check deploys a deployment.
- A deployment observes production evidence.
- A design decision implements several pull requests at once.
- Attention items, evidence and containment actions point at any of the above through related IDs.

The benchmark needed one unbroken chain from intent to production outcome. That chain was: intent, epic, feature, story, pull request, check, deployment, evidence. The console had to be able to walk it in both directions.

## Catalog versus state

Version 1.1 split each entity into two parts:

- **Catalog.** Facts that do not change during a scenario: IDs, names, purposes, commands, relationships, decision option text.
- **State.** Facts that change: repository status, work status, check status, runner status, deployment status, active attention, completed containment, known evidence, pending decision, and the event history so far.

A snapshot at any moment is the catalog plus the cumulative state. The console only ever receives a snapshot. It cannot read future events or the full catalog with end-of-story statuses.

This split came from a real defect. The first version gave the UI the whole story at once, so the "calm" view already showed failed checks and rolled-back deployments.

## Availability versus domain state

The console receives a product availability value that is separate from the domain state:

`ready`, `loading`, `streaming`, `stale`, `disconnected`, `empty`.

"Healthy" is not an availability value. It is the absence of active unhealthy domain state while availability is `ready`. Attention is derived from domain facts. It is not a mode.

## Actions

Operator actions are domain actions, valid only in the states where their preconditions hold:

- select an attention item;
- select a workstream;
- set a diagnostic overlay;
- open evidence;
- choose a recovery option;
- confirm a recovery option.

An invalid action returns the unchanged snapshot. Controls derive their visible and enabled state from the snapshot, so the UI does not offer invalid actions. Calm state has no incident, choose or confirm controls.

Test and evaluator controls (advance, reset, override availability) live in a separate interface that product UI cannot import.

## The reference incident scenario

This scenario exercises attention, the live floor, investigation, the flight recorder, traceability and a controlled operator decision in one story. It is a candidate acceptance scenario for the smallest working engine and for the operator console.

Fictional setting: six repositories for a portfolio product (investor app, portfolio API, wallet importer, exchange importer, market data sync, platform infrastructure). Two intents: keep portfolio integrity synchronized, and keep portfolio data fresh. One design decision, "coordinate reconciliation ordering", implemented through three pull requests in the portfolio API.

| Step | What is true | State change |
|---|---|---|
| Calm | Routine autonomous work. All repositories healthy, no active attention, no containment, no decision. Three deployments already live. | Initial state. |
| Reconciliation breach | The post-deployment replay gate fails. Twenty-seven fictional portfolios show inconsistent totals. | Portfolio API goes to attention-required. Replay check fails. Critical attention item raised. Breach and replay evidence become known. |
| Contained | Automation halts rollout, pauses generation, quarantines the queue and preserves snapshots. No rollback has happened. | Four containment actions completed. |
| Investigating | A verification session finds the missing cross-service ordering case. | Availability is streaming. |
| Decision required | Two verified options are offered, each with impact, confidence, reversibility and missing knowledge. | Pending decision is required. |
| Rollback applied (branch A) | Deployments are rolled back. Affected portfolios stay stale until safe resync. | Three deployments rolled-back. Decision approved. |
| Forward fix approved (branch B) | The forward fix and controlled replay begin. | Decision approved. Availability is streaming. |
| Recovering | Corrected artifacts move through verification. | Replay check running. |
| Recovered | Recovery is complete. Evidence stays available. | Portfolio API healthy. Replay check passed. Decision cleared. |

Background pressure that stays visible through the incident: a stale remote runner, a wallet contract check still running, a ready-work runway metric, replay failure count and review retry cost.

The two decision options are:

- **Roll back.** Stops inconsistent totals while freshness stays delayed. Confidence 0.98. Reversible by approving a later controlled replay. Unknown: the length of the safe resync window.
- **Forward fix.** Restores totals with changed recovery behaviour. Confidence 0.87. Can be halted before each replay batch completes. Unknown: the overlap rate in the first replay batch.

## What to test against the smallest working engine

- Does the Engine's durable unit map onto work item, agent session, check, deployment and event, or does it need something else, such as attempt or run?
- Is "attention" an entity the Engine owns, or a projection the console derives?
- Is the catalog-versus-state split still natural when state comes from real providers, not a fixture?
- Do trace links need a relation vocabulary this small, or a richer one?
- Do the six availability values survive contact with real disconnects and partial data?

Related: [`open-questions.md`](open-questions.md), [`decisions/0003-deterministic-verification-is-authoritative.md`](decisions/0003-deterministic-verification-is-authoritative.md), [`../research/ux/agent-built-ui-lessons.md`](../research/ux/agent-built-ui-lessons.md).
