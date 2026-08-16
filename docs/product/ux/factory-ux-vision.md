# Software Factory UX vision

## 1. What this product is

The Software Factory UI is the operating console for an autonomous software production system.

Its scope is broader than project management, AI chat, DevOps dashboards, agent status cards or CI/CD presentation.

The factory should increasingly execute routine work without human involvement. Human attention is reserved for intent, taste, exceptions, trade-offs, and calibration of the factory itself.

The UI therefore needs to answer, in roughly this order:

1. Does the factory need me?
2. Is the factory healthy?
3. What is it doing right now?
4. Where is work flowing, slowing, blocked, or failing?
5. Does it have enough useful work to continue operating?
6. How effective and economical is the factory?
7. How can the factory itself be improved?

A useful shorthand is:

> Control room + factory floor + debugger + flight recorder + improvement laboratory.

## 2. Operator model

### Human attention is the scarce resource

The interface should not make healthy autonomous work compete visually with things that genuinely need human judgment.

Routine operation should be calm. Exceptions should be obvious. Raw machinery should always remain inspectable.

### Progressive abstraction

The UI should work at several distances:

- At 10 metres: Is the factory healthy? Does anything need me?
- At 1 metre: Where are work, bottlenecks, queues, costs and constraints?
- At 10 centimetres: What exact event, tool call, diff, trace, gate result or agent interaction caused this?

The system should default to semantic summaries but never trap engineers behind abstraction. There must always be a path to the raw numbers and raw evidence.

### The desktop UI is a control surface over the factory

Core orchestration semantics must not live in React.

The engine should remain independently operable and observable through other surfaces where useful: CLI, APIs, MCP, automation, headless services, and potentially a web-hosted UI later.

The current protocol direction makes the UI an AHP client for synchronized agent-session state and a factory-native API client for work graphs, workflow, policy, verification and lifecycle state. A logically separate Factory AHP Gateway is the AHP server toward factory-owned surfaces; the Factory Engine has no intrinsic AHP role. AHP is a projection and integration boundary. The model of the factory lives elsewhere. See [`../../research/ahp-acp-architecture-direction.md`](../../research/ahp-acp-architecture-direction.md).

## 3. Six primary surfaces

These are conceptual surfaces. They do not necessarily imply six isolated pages or six navigation destinations.

### 3.1 Command / Attention

Operator question: What needs me, and is anything unhealthy?

This is the exception-first operating surface.

Healthy state should be quiet and reassuring without becoming decorative. A typical healthy summary might communicate:

- no intervention required,
- active work count,
- current throughput,
- ready-work runway,
- projected daily cost,
- unusual changes from baseline.

When attention is required, the interface should prioritise by impact and urgency rather than by event arrival time.

Examples:

- production failure,
- failed deployment or rollback,
- security exception,
- PR or architectural review requiring judgment,
- agent blocked on an ambiguity it cannot resolve from existing guidance,
- irreversible migration decision,
- rapidly rising cost or retry loop,
- factory approaching work starvation.

#### Lift the hood

This surface must offer a deliberate path to raw operational detail:

- exact agent counts and states,
- queues,
- token and model usage,
- costs,
- run rates,
- retries,
- gate counts,
- failure rates,
- infrastructure state.

Abstraction should improve cognition while leaving the machine visible.

### 3.2 Factory Floor

Operator question: What is happening right now?

The preferred interaction model is a smoothly zoomable and pannable live spatial topology of interconnected factory components and current work.

A node/edge graph implementation such as React Flow is one candidate, and the design does not depend on it.

The important invariant is:

The topology and visual state must be driven by real factory state and events.

Possible visible entities include:

- work items,
- repositories,
- agents,
- execution stages,
- reviewers,
- deterministic gates,
- build/test environments,
- deployment targets,
- external dependencies.

#### Live movement

Animation should be subtle and meaningful.

Movement may represent real events such as:

- a work item moving stage,
- an agent beginning or completing an operation,
- a message exchanged,
- a tool invocation,
- an artifact emitted,
- a gate changing state,
- a deployment progressing,
- work becoming blocked or unblocked.

Do not animate generic packets or decorative flows to make the factory look alive.

### 3.3 Work Graphs

Operator question: How is this work structurally connected?

Do not build one God Graph containing every relationship. Present different lenses over shared underlying truth.

Initial graph lenses:

#### Execution graph

Work item to subtasks, agents, operations, gates, review and deployment.

#### Dependency graph

What depends on what? Which work can proceed in parallel? What sits on the critical path? What is blocked by a dependency?

#### Code structure / architecture graph

Repos → packages/services → modules/domains → interfaces → dependencies → affected areas.

This view should help expose architectural collisions, coupling, hotspots and drift that may not be visible from individual green PRs.

#### Intent / requirements traceability graph

Business and product intent to evidence, requirement or spec, ADR or design decision, work item, tests, code or PR, deployment and production outcome.

This is the visual expression of traceability back to intent.

### 3.4 Work Item Flight Recorder

Operator question: Exactly what has happened, what is happening, and why?

The Flight Recorder is the semantic timeline for a work item.

It should progressively drill from high-level lifecycle events to raw execution evidence.

A useful drill-down model:

`phase → event → agent/run → interaction → tool call → raw output / trace / diff`

The top-level timeline should show meaningful events, for example:

- implementation started,
- contract tests added,
- type-check failed,
- agent repaired violations,
- reviewer found an architecture problem,
- remediation began,
- deterministic gates passed,
- deployment began,
- rollback triggered.

Raw chat transcripts are supporting evidence beneath semantic events, which carry the primary representation of work.

The Flight Recorder should make it possible to answer:

- Who/what acted?
- When?
- On whose instruction or which work item?
- What changed?
- What evidence caused the next decision?
- What failed and how was it recovered?
- What did this operation cost?
- What artifacts were produced?

### 3.5 Meta-loop / Factory Intelligence

Operator question: How do we make the factory itself better?

This is a first-class surface for investigation and improvement.

The interaction model should support Honeycomb-style exploratory investigation rather than canned dashboarding.

The user should be able to select unusual populations, compare cohorts, pivot by dimensions, inspect distributions and ask what distinguishes good runs from bad ones.

Example investigations:

- Why did review rework rise this week?
- Which prompts produce the most escaped defects?
- Which skills correlate with shorter successful runs?
- Does Model A outperform Model B for Rust but not TypeScript?
- Which deterministic gates create high noise with low actionable yield?
- Which review agents repeatedly find issues that no human or downstream evidence considers useful?
- What classes of failures are not caught until staging or production?
- Where is human time still being spent repeatedly?

Core dimensions include:

- gate false-positive and actionable rates,
- escaped defects,
- retry/rework loops,
- prompt/skill effectiveness,
- model effectiveness by work type/domain,
- tool effectiveness,
- cost per accepted outcome,
- human intervention rate,
- architectural drift,
- agent/tool latency,
- token usage,
- pipeline cost,
- capability changes over time.

The surface should support discovery of unknown problems as well as reporting defined metrics.

### 3.6 Factory Runway

Operator question: Can the factory continue doing useful work without human replenishment?

This is distinct from raw compute or agent capacity.

The core concept is autonomous ready-work runway: how long can the factory continue productive execution at the current rate before it becomes work-starved?

Potential signals:

- ready-work hours,
- expected starvation time,
- number of validated executable items,
- executable frontier width,
- available parallelism,
- critical-path depth,
- work blocked by human decisions,
- work blocked by dependencies,
- specs nearly ready but missing clarification,
- backlog portfolio composition,
- confidence-weighted available work,
- expected drain rate,
- agent/runner capacity versus ready-work supply.

Report runway in operational terms:

> 11.4 hours autonomous runway. At current throughput the implementation pool is likely to become work-starved at 06:42. Seven items are blocked by human decisions; two could become executable with small clarifications.

## 4. Ambient operator assistant

Do not design a chat feature with a permanent chatbot home.

Design an ambient operator agent that can participate throughout the interface.

Possible interaction modes:

- voice,
- command palette,
- keyboard invocation,
- contextual action,
- proactive attention item,
- transient dynamic panel,
- annotation directly on a graph,
- generated investigation view,
- later, optionally, avatar/video.

Conversation is one interaction modality among several.

Example:

The user sees a blocked cluster on the Factory Floor and asks:

> Why are these five jobs stuck?

A good response may be primarily visual rather than textual:

- dim unrelated nodes,
- highlight the common dependency,
- open a comparative event view,
- show the queue increase and relevant time window,
- provide the concise causal explanation.

The assistant should help operate and interrogate the factory from inside it.

## 5. Stable surfaces and dynamic UI

Some product surfaces should remain stable and learnable:

- Command / Attention,
- Factory Floor,
- Work Graph shell,
- Work Item Flight Recorder,
- Meta-loop shell,
- Factory Runway.

Other surfaces may be created dynamically from data, investigations or agent interaction:

- diagnostic views,
- ad-hoc comparisons,
- temporary controls,
- generated visualisations,
- investigation panels,
- novel analysis workflows.

The architecture should therefore leave room for a protocol capable of expressing:

- data,
- events,
- actions,
- streaming updates,
- presentation hints,
- safe interactive components.

Possible technologies such as AG-UI, A2UI, MCP Apps or equivalent patterns are implementation candidates to explore. The project commits to none of them yet.

AHP may carry synchronized, display-ready agent-session state and actions. It does not define a general dynamic-UI protocol. Do not encode generated investigations or the wider factory domain as invented AHP session actions merely to use one wire contract.

## 6. Interaction philosophy

### Prefer direct manipulation over navigation

Where possible, let the operator:

- zoom,
- pan,
- select,
- expand,
- collapse,
- isolate,
- filter,
- compare,
- overlay,
- scrub time,
- drill down,

while preserving spatial and analytical context.

Avoid repeatedly sending users to unrelated pages to answer the next obvious question.

### Same truth, multiple projections

Factory Floor, execution graph, dependency graph, architecture graph, traceability graph, Flight Recorder and meta-loop analysis should be views over shared entities, relationships, events and telemetry.

Do not allow each screen to invent its own competing model of reality.

### Time is first-class

Autonomous systems cannot be understood from current state alone.

The user should often be able to move between:

- now,
- recent history,
- a selected time range,
- before/after a change,
- baseline versus anomaly.

### Overview → explanation → evidence

Every meaningful summary should have a route to:

1. what happened,
2. why the system thinks it happened,
3. the underlying evidence.
