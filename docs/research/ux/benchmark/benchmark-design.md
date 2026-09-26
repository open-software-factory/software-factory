# UX workflow benchmark design

Status: approved design

Date: 2026-08-17

Benchmark version: `ux-operator-slice-v1`

## Purpose

This benchmark evaluates AI-assisted product-design workflows for the Software Factory. It tests whether a workflow can translate the Factory's existing product intent, visual preferences and interaction principles into a distinctive, useful operator interface that a non-designer can steer.

It does not test which model produces the most attractive unguided screenshot. Every candidate receives the same product context, data, scenario, prototype substrate, interaction budget and verification. In the controlled portable-workflow lane, the design workflow is the only intentional variable.

The benchmark produces two independent findings:

1. Which workflow best supports repeatable, non-designer-led product design.
2. Which product direction is most worthy of promotion into the Factory's design-system foundation.

Those findings may come from different candidates.

## Scope

The first benchmark covers one connected operational slice:

- Command and Attention;
- a persistent live Factory Floor;
- incident investigation across affected work and repositories;
- Flight Recorder and requirements-to-production traceability;
- a controlled operator decision and visible recovery.

The slice is deeper than a dashboard screen but narrower than a miniature implementation of the whole product. It exercises spatial, temporal and evidence-heavy interaction without asking candidates to design every planned Factory surface.

The benchmark does not implement the Factory Engine, settle the production UI architecture, select a graph library, or accept a design system automatically.

## Evaluation and promotion stages

### Stage 1: frozen evaluation

Start with a calibration pair:

1. A fresh Codex session using the frozen Factory UX context and no additional design workflow.
2. A fresh Codex session with byte-identical context plus Trystan-SA's design prompt and procedures.

Both runs use the same model, reasoning level, starter commit, fixture, viewports, interaction budget and deterministic verification. Each run uses an isolated worktree and branch with no access to another candidate's output.

The calibration validates the benchmark rather than declaring a final winner. If the protocol proves discriminating and fair, run the other shortlisted workflows in comparable batches. Claude Code and Claude Design are not part of the first calibration. Kimi K3 experiments remain deferred until access is available.

### Stage 2: promotion

After sufficient frozen-evaluation evidence, select the strongest workflow and strongest product direction independently. The promotion pass:

- applies the direction to a higher-scale Factory Floor with additional overlays and concurrent workstreams;
- expands the Flight Recorder across incident response and ordinary feature delivery;
- exercises longer healthy, degraded and disconnected periods;
- adds at least one PR walkthrough;
- extracts provisional visual rules and reusable patterns;
- verifies that another workflow can preserve the direction if the best workflow and strongest direction came from different candidates.

Nothing from a benchmark run enters production automatically. Promotion is an explicit product decision.

## Frozen product context

Candidates are guided by the Factory's intent rather than left to their training-data defaults. The context manifest pins the Git commit and content hashes of:

- `docs/product/ux/factory-ux-vision.md`;
- `docs/product/ux/design-principles.md`;
- `docs/product/ux/open-questions.md`;
- `docs/product/decisions/0001-factory-ui-is-an-operator-console.md`;
- `docs/research/ux/ux-references.md`;
- the benchmark brief, glossary, visualization-intent guide, fixture and constraint sheet.

The frozen context states the existing preferences plainly:

- dark theme;
- restrained green and blue tint with subtle gradients;
- high but legible information density;
- clear attention hierarchy;
- calm healthy operation and unmistakable exceptions;
- meaningful, non-decorative motion;
- progressive abstraction with a route to raw evidence;
- visual activity derived from real Factory events;
- no generic SaaS dashboard, card grid, chatbot home or decorative AI styling;
- a genuinely beautiful and distinctive interface, not one that is merely functional.

Candidates must produce different interpretations within these constraints. Theme swaps do not count as different directions.

For future hosted tools, generate a sanitized bundle from the same manifest. Do not grant repository access merely for convenience.

## Visualization intent

The benchmark supplies goals and semantics without prescribing chart types, graph rendering technology or a final visual grammar.

| Surface | Required communication | Constraints |
|---|---|---|
| Command and Attention | What needs intervention, overall health, active work, runway and unusual changes | Exception-first; healthy work stays calm; not a KPI-card grid |
| Factory Floor | Live work across repositories, agents, gates, runners and deployments | Persistent spatial backbone, semantic zoom and event-derived motion; not decorative activity |
| Work Graph | Dependencies, parallel work, critical path and blocking | Separate useful lenses rather than one universal graph |
| Traceability | Intent through decisions, work, code, verification, deployment and outcome | Preserve why and evidence; expose missing links |
| Flight Recorder | How work evolved and why its state changed | Semantic events first; raw messages, calls and traces beneath; not a chat transcript |
| PR Walkthrough | What changed, why, risk, verification and evidence | Visual narrative at multiple abstraction levels; not a wall of diffs or prose |
| Factory Runway | How long useful autonomous work can continue and what constrains it | Show assumptions and uncertainty; avoid false precision |
| Factory Intelligence | Anomalies, cohorts, costs, quality and workflow effectiveness | Support exploration and comparison rather than only fixed dashboards |

All visible encoding must correspond to fixture data. Staleness, uncertainty and missing evidence require explicit treatment. Candidates explain why their chosen visual forms fit the operator question. References such as Factorio, Temporal, Honeycomb, Dagster and strategy games offer lessons to learn from and nothing to copy.

## Synthetic product fixture

The domain shape is informed by a real polyglot investment-software estate, but the fixture contains no copied repositories, product names, users, infrastructure, business facts or operational values.

The fictional product is a multi-asset portfolio application with six repositories:

| Repository | Ecosystem | Responsibility |
|---|---|---|
| `investor-app` | Flutter and Dart | Mobile and web client |
| `portfolio-api` | .NET and C# | Portfolio coordination, storage and analysis API |
| `wallet-importer` | TypeScript | Blockchain-wallet balances and transactions |
| `exchange-importer` | Python | Centralized-exchange balances and transactions |
| `market-data-sync` | Java | Market prices and reference data |
| `platform-infra` | Infrastructure code | Cloud resources, deployment and runners |

The dataset contains:

- 45 work items across epics, features, stories, tasks, bugs and chores;
- explicit dependency and parallelism relationships;
- 14 active or recently active agent sessions;
- 9 open PRs in different review and verification states;
- local Windows and remote Linux and macOS execution;
- deterministic test, build, security, data-quality and deployment evidence;
- throughput, runway, retry and cost history;
- enough semantic and raw history for evidence drill-down.

Stable fictional identifiers and timestamps make every rendered value traceable to the fixture. Candidates may not replace or invent data to suit a layout.

## Scenario

### 1. Calm operation

Agents are implementing portfolio-refresh reliability work across the API, importers and app while unrelated market-data and infrastructure work proceeds in parallel. Fourteen sessions are visible without implying that agent count is the goal.

### 2. Deterministic detection

Following a coordinated release, reconciliation detects that 27 fictional portfolios have displayed totals inconsistent with their underlying holdings after overlapping wallet and exchange refreshes.

### 3. Automated containment

The Factory halts the rollout, pauses affected generation, quarantines relevant queue messages and preserves last-known-good snapshots. The interface must show that containment is already complete rather than manufacture work for the operator.

### 4. Correlated investigation

The Factory links the original intent and design decision to work items, three PRs, passing pre-deployment checks, deployment events, traces and failing reconciliation evidence. The likely cause is a cross-service ordering case absent from the original test fixture.

### 5. Genuine operator decision

The Factory presents two verified choices:

- roll back and leave affected portfolios stale until a safe resync;
- approve a forward fix and controlled replay that passed synthetic production-data verification but changes recovery behavior.

The interface explains impact, confidence, reversibility and missing knowledge. The operator decides policy; the Factory performs the mechanics.

### 6. Background pressure

A self-hosted runner is stale, review retries have increased cost, and ready-work runway is shrinking. These conditions remain visible but subordinate to the data-integrity decision.

### 7. Recovery

The chosen action updates work, agent assignments, PR state, verification, deployment and audit history.

The same fixture provides healthy, attention-required, loading and streaming, stale and disconnected, and empty states.

## Factory Floor requirements

The Factory Floor is the persistent spatial backbone of the slice. A decorative graph behind a dashboard would fail the brief.

It represents:

- six repository or work-cell regions;
- durable work and dependency relationships;
- implementation, verification, review and deployment stages;
- agents as resources attached to work, with the work as the organizing entity;
- deterministic gates, runners and deployment targets;
- work moving between stages;
- transient messages, tool calls, artifacts and gate outcomes;
- blocking, starvation, congestion and rework;
- local and remote execution.

The operator can:

- move between whole-factory, selected-workstream and exact-event distances;
- select the incident's affected path across repositories;
- use at least one diagnostic overlay such as Work, Quality, Bottlenecks or Risk;
- see the canary failure change topology or state rather than merely add an alert card;
- open the Flight Recorder without losing spatial context;
- see the approved recovery change work, verification and deployment state.

Required motion includes one work-stage transition, one agent/tool/artifact event and one gate or deployment-state change. Reduced-motion mode must preserve the meaning without relying on animation.

Candidates choose the spatial metaphor and rendering approach. React Flow, conventional node-edge DAGs and conveyor-belt metaphors are neither required nor forbidden. A grid of DevOps cards with a decorative mini-graph fails the Factory Floor gate.

## Prototype substrate

All candidates start from the same minimal browser application:

- React, TypeScript and Vite;
- fixture data loaded locally;
- deterministic scenario controller;
- no backend, Tauri shell, authentication or Factory Engine;
- no supplied component library or house style;
- identical initial dependencies;
- no network dependency at evaluation time.

Candidates may add open-source fonts, icons or libraries if the run records and preserves their versions and licences.

Primary viewport: `1536 × 960`.

Secondary density viewport: `1280 × 800`.

The benchmark is implementation-relevant but disposable. Winning artifacts require an explicit promotion decision before reuse.

## Candidate interaction budget

Each run receives:

- at most three plain-language clarification questions before direction work;
- three materially different initial directions;
- one operator selection with unrestricted natural-language feedback;
- one natural-language revision;
- one further revision based on a pointed or annotated rendered element;
- no unsolicited coaching about typography, grids, component patterns or visual style.

Elapsed time, prompts, observable tool calls and manual interventions are recorded. Calibration has no hard runtime cutoff. Excessive questioning, repair or nudging remains visible as workflow friction.

## Required direction and prototype output

Each initial direction must render enough of the connected slice to judge it:

- calm whole-factory overview;
- critical-attention overview;
- selected-incident Factory Floor state at whole-factory and workstream distances;
- one Flight Recorder or traceability drill-down.

Directions must differ in composition, spatial model, interaction and visual hierarchy. Each includes an ordinary-language explanation of what it optimizes and sacrifices.

After selection, the candidate completes the interactive scenario, required states and both revision rounds.

## Run protocol

### Preflight

1. Create a fresh worktree from the frozen benchmark commit.
2. Record model, harness, reasoning level, package and runtime versions, OS and start time.
3. Confirm that no previous candidate output is visible.
4. Validate the fixture and starter before candidate work begins.

### Candidate work

1. Supply the frozen context and capture clarification questions and answers.
2. Generate directions A, B and C.
3. Record the operator's selection and natural-language feedback.
4. Complete the interactive slice and first revision.
5. Capture the pointed or annotated feedback and second revision.
6. Record added dependencies, assets and licences.

### Verification and capture

Independent checks cover:

- fixture and schema validity;
- Playwright scenario outcomes;
- screenshots of every required state and viewport;
- keyboard paths;
- axe-core accessibility findings;
- overflow and clipping;
- reduced-motion behavior;
- confirmation that visible values derive from the fixture.

Capture the observable prompts, responses and tool calls. Record token or cost data only where the harness exposes it; otherwise mark it unavailable. Do not claim access to private model reasoning.

## Failure handling

- An infrastructure or harness failure gets one clean retry; preserve both attempts.
- Candidate-generated build, interaction or accessibility failures remain part of the result.
- One neutral continuation nudge is allowed after a stall and counts as operator intervention.
- If the benchmark is ambiguous or defective, stop the pair, version the correction and rerun every affected candidate.
- Apply environment repairs to the common starter before rerunning all affected candidates.
- Never clarify or repair the benchmark for only one candidate.

A close result is not automatically a benchmark failure. It may be a genuine tie. Revise the benchmark only when evidence is missing, criteria are ambiguous, the scenario fails to exercise the claimed capability, a run receives an unintended advantage, or required outputs cannot be evaluated consistently.

## Evidence and privacy model

### Publishable research record

Version-controlled and safe to share by construction:

- an explicit “as of August 2026” date;
- exact tool, skill, model and harness versions;
- benchmark method and changes;
- sanitized supplied prompts and context;
- workflow observations, scores and disagreements;
- prototype-only screenshots and interaction captures;
- elapsed time, available usage data and manual effort;
- successes, failures and limitations;
- conclusions with a re-test warning.

### Private raw evidence

Store unfiltered session exports, recordings and diagnostic traces under ignored temporary context. Never commit raw evidence containing machine paths, identities or unrelated workspace information.

Browser capture is restricted to the prototype viewport. Do not capture the full desktop. The publishable record excludes personal identifiers, credentials, local paths, private repository names or data, voice recordings and unrelated screen content.

A separate privacy review is required before producing or publishing a blog post, video, short, LinkedIn post or social thread.

## Scoring

### Product outcome: 74 points

| Criterion | Weight |
|---|---:|
| Operator comprehension and attention hierarchy | 15 |
| Factory Floor system legibility and meaningful live state | 15 |
| Investigation, temporal continuity and traceability | 12 |
| Product-specific judgment; avoids generic DevOps-dashboard patterns | 12 |
| Interaction and required-state completeness | 10 |
| Visual distinction, coherence and craft | 10 |

### Workflow quality: 19 points

| Criterion | Weight |
|---|---:|
| Non-designer steering in ordinary language | 12 |
| Revision fidelity without unwanted visual drift | 5 |
| Time, cost and manual friction | 2 |

### Engineering fitness: 7 points

| Criterion | Weight |
|---|---:|
| Design-system extensibility and artifact portability | 4 |
| Accessibility and deterministic verification | 3 |

### Hard gates

- Non-designer: the workflow cannot require the operator to perform professional design work.
- Factory Floor: the floor must encode real system structure and events rather than decorate a dashboard.
- Reproducibility: inputs, outputs and observable process must be preserved well enough to inspect and rerun.

Operator judgment, blind independent review and deterministic evidence remain separate. Preserve disagreements instead of averaging them away. The weighted total supports comparison but does not make the product decision by itself.

## Design-system acceptance

The promotion pass extracts provisional:

- color and state semantics;
- typography and information-density rules;
- spacing, layering, borders, depth and surfaces;
- iconography;
- motion and reduced-motion language;
- Factory Floor node, edge, event, overlay and semantic-zoom grammar;
- temporal and traceability grammar;
- attention, intervention, evidence and drill-down patterns;
- reusable component families;
- explicit positive and negative examples with rationale.

A rule enters the accepted design-system foundation only if it survives Command and Attention, the broader Factory Floor, and the broader Flight Recorder. The first accepted artifact remains implementation-neutral. React tokens and components follow afterward.

## Repository layout produced by implementation

```text
docs/research/ux/benchmark/
  README.md
  benchmark-design.md
  benchmark-brief.md
  domain-glossary.md
  context-manifest.json
  visualization-intent.md
  constraint-sheet.md
  interaction-script.md
  scorecard.md
  capture-format.md
  journal/
  runs/

experiments/ux/command-attention/
  fixture/
  starter/
  verification/
```

Candidate code lives on isolated experiment branches. Publishable run records on the main branch refer to those commits. Raw evidence remains ignored.

## Research longevity

The benchmark and every public conclusion retain their date and version. Rerun relevant candidates when a major model, harness or workflow changes rather than pretending the August 2026 result remains current indefinitely.

Success does not require an external workflow to beat the baseline or a direction to pass promotion. A well-supported negative result remains useful.
