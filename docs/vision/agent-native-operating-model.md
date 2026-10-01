# The agent-native business

An operating model for software product companies where agents build and humans direct.

Status: living draft

This document describes the operating model of a software product business built around AI-agent execution. It is a strategic companion to the Automated SPDLC / SDLC document, which covers the automation and verification mechanics of the engineering loop.

The central claim is that when agents do the building, clarity of intent, quality of judgment and product-loop discipline become scarcer than engineering capacity. A 3–5 person team operating this model can match or exceed the throughput, quality and value output of an organisation ten times its size by wasting less and learning more per unit time.

## 1. The fundamental shift

The binding constraint moves from engineering capacity to clarity of intent.

### 1.1 What has changed

For most of software's history, engineering capacity has constrained product businesses. The product had more ideas than engineering could build. Management mediated that constraint through prioritisation, sequencing and resourcing. Much of product management emerged from this reality.

Well-directed agent swarms can remove that constraint for clearly specified work. If an agent swarm can implement a feature in hours instead of weeks with automated quality verification at every step, “can we build this?” becomes “have we specified this clearly enough?”

| The Old Constraint                       | The New Constraint                                  |
|----------------------------------------------|---------------------------------------------------------|
| Engineering capacity                         | Clarity of intent                                       |
| Headcount limits throughput                  | Spec quality limits throughput                          |
| Features queued behind engineering bandwidth | Experiments limited only by validated hypothesis volume |
| More people = more output                    | Better specifications + better judgment = more output   |
| Technical debt limits future velocity        | Architectural drift limits agent coherence              |
| Code review as quality gate                  | Architectural fitness as quality gate                   |
| Done = deployed                              | Done = hypothesis validated against production evidence |

### 1.2 New failure modes

The shift introduces distinct failure modes that the operating model must address.

| Failure Mode     | Description                                                                                                                                                                       | Antidote                                                                                                         |
|----------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------|
| Spec inflation       | When building is cheap, there is temptation to write specs for unvalidated ideas, explore too many directions and mistake "cheap to build" for "worth building."                        | Product loop discipline. The outer loop governs what enters the backlog.                                             |
| Architectural drift  | Each agent output passes all gates individually but the system collectively drifts from intended architecture. Invisible at the PR level; only visible at the system level over time. | Architectural fitness functions. System-level checks that run across the full codebase as well as individual changes. |
| Exception fatigue    | If escalation volume is too high, human reviewers approve without genuine judgment. The exception-only model breaks down silently.                                                    | Gate calibration as critical infrastructure. Meta-loop runs on a schedule, whether or not something has broken.                |
| Confident wrongness  | Agents produce well-tested, correctly-gated output that is structurally wrong because the spec was unclear or the architecture context was missing.                                   | Spec quality gates. Human architectural review on significant changes. The spec is the primary interface.            |
| Incoherent product   | Parallel agent execution produces features that individually pass all tests but do not form a coherent product whole.                                                                 | Portfolio-level product judgment. Human taste operates at the system and feature levels.                             |
| Capability blindness | Humans lose touch with what is technically possible or what the agents are actually doing. Direction becomes disconnected from execution reality.                                     | Product engineers maintain architectural understanding. They do not write code but they think architecturally.       |

## 2. The two-loop model

The two loops run concurrently at different cadences and connect at defined points.

### 2.1 Structure

The operating model is built around two concurrent loops running at different cadences, neither subordinate to the other, with multiple defined handoff and feedback points between them.

|                  | The Product Loop                                            | The Engineering Loop                                 |
|------------------|-----------------------------------------------------------------|----------------------------------------------------------|
| Primary question | What is worth building, for whom, and why?                      | Does this work as specified, safely and correctly?       |
| Governed by      | Evidence accumulation and validated learning                    | Correctness verification and quality gates               |
| Primary artefact | A decision with supporting evidence                             | Working, verified, deployable software                   |
| Executed by      | Humans (product engineers) + research/analytics tools           | Agent swarm + automated verification pipeline            |
| Cadence          | Slower; governed by evidence accumulation and market rhythm     | Faster; multiple concurrent specs in parallel execution  |
| "Done" means     | Intended outcome achieved, measured against original hypothesis | Spec implemented, all gates passed, deployed, monitored  |
| Failure mode     | Building the right thing poorly described                       | Building the wrong thing correctly                       |

### 2.2 Why the loops are concurrent

The most common failure in product organisations is running these loops sequentially: product finishes, then engineering starts. This means:

- Product direction is stale by the time engineering delivers because the market has moved.

- Engineering discoveries (what was easy, hard, or surprising) do not feed back into product direction in time to matter

- Technical debt accumulates invisibly until it suddenly constrains product options

- The loops cannot calibrate each other because they are not in contact.

Running them concurrently keeps the product loop discovering, validating and refining the next directions while the engineering loop executes the current validated backlog. Neither loop waits for the other.

### 2.3 Connection points

The loops touch at defined points in both directions. These are not organisational hand-offs (one team finishing, another starting). They are data flows between two ongoing processes.

| Direction | Connection Point       | What Flows                                                                                   | Trigger                                                    |
|---------------|----------------------------|--------------------------------------------------------------------------------------------------|----------------------------------------------------------------|
| → Engineering | Validated spec             | Intent + constraints + acceptance criteria, precise enough for agent execution                   | When product loop declares sufficient evidence for a direction |
| → Engineering | Experiment instrumentation | What to measure, how to measure it and success thresholds before the feature is built            | When experiment is designed in the product loop                |
| → Engineering | Debt prioritisation signal | Which technical debt is blocking product directions the business cares about                     | Quarterly portfolio review                                     |
| ← Product     | Production evidence        | Usage, behaviour, performance and errors feeding discovery and hypothesis validation             | Continuous; weekly digest                                      |
| ← Product     | Feasibility signal         | Technical constraints that shape what solution directions are viable                             | During product loop hypothesis generation                      |
| ← Product     | Delivery pace signal       | Current throughput capacity, debt level and architectural risk informing product planning cadence | Monthly                                                       |
| ← Product     | Incident findings          | What product assumptions were wrong, what the system revealed about user behaviour               | Per incident, post-resolution                                  |

Delivery pace and debt level should flow back into product scope. When debt slows engineering, the product loop should reduce scope and allocate cycles to debt reduction instead of adding capacity. This is a product decision.

## 3. The product loop

The product loop is evidence-driven, human-led and governs what enters the engineering loop.

### 3.1 Loop structure

The product loop is not a phase that concludes. It runs continuously, in parallel with everything else. Its output is a stream of validated decisions that become the fuel for the engineering loop.

| Stage              | Question Being Answered                                              | Output                                                                | Automation Level                                        |
|------------------------|--------------------------------------------------------------------------|---------------------------------------------------------------------------|-------------------------------------------------------------|
| Continuous discovery   | What problems are users experiencing? What opportunities exist?          | Problem inventory, ranked by frequency and severity                       | High: synthesis automated; sense-making human               |
| Problem definition     | Is this problem real, frequent, and worth solving for our users?         | Validated problem statement with evidence                                 | Medium: evidence aggregated automatically; definition human |
| Opportunity assessment | Is solving this strategically valuable? What is the opportunity cost?    | Go/no-go decision with documented rationale                               | Medium: competitive signals automated; judgment human       |
| Solution hypotheses    | What are the different ways we could solve this?                         | Hypothesis set with differentiated risk/value profiles                    | Low: generation assisted; selection human                   |
| Prototype & test       | What does the right solution feel like? Do users respond to it?          | Evidence of user response to proposed direction                           | Medium: tooling automated; interpretation human             |
| Experiment design      | How will we know, in production, if we were right?                       | Hypothesis, metric, success threshold, instrumentation requirements       | Medium: structure validated automatically; design human     |
| Spec                   | What precisely should be built?                                          | Validated spec: complete, unambiguous, testable, with acceptance criteria | High: quality review automated; authorship human            |
| Measurement            | Did we achieve the intended outcome?                                     | Outcome verdict: ship / iterate / kill / scale                            | High: analysis automated; verdict human                     |

### 3.2 The deliberate automation gap

The automation ceiling on the product loop is lower than on the engineering loop because the work differs structurally.

Product decisions are probabilistic: evidence supports a belief about what users want. Engineering verification can often be deterministic: the code either does what the specification requires or it does not. Automation fits deterministic verification better than probabilistic sense-making.

Automation in the product loop reduces the cost of evidence gathering and synthesis by making research faster, experiment analysis immediate and signal aggregation continuous. This allows more product-loop cycles per unit time. Humans still make the judgments between evidence and insight.

See Automated SPDLC / SDLC, Phase 0: Discovery and ideation, and Phase 1: Requirements & Specification, for tooling and verification detail.

## 4. The engineering loop

The engineering loop is agent-executed and verification-governed, with exception-only human involvement at maturity.

### 4.1 Agent swarm model

At maturity, a swarm of agents executes a structured backlog with automated verification at every step. Human involvement is limited to genuine exceptions.

| Layer                | Who / What            | Responsibility                                                                                                                   |
|--------------------------|---------------------------|--------------------------------------------------------------------------------------------------------------------------------------|
| Backlog                  | Product engineers (human) | Structured, prioritised, validated specs ready for agent execution. The primary interface between loops.                             |
| Execution                | Agent swarm               | Parallel implementation of backlog items. No human code authorship.                                                                  |
| Verification             | Automated pipeline        | All quality, security, performance, and compliance gates. Hard pass/fail. Deterministic tooling.                                     |
| Architectural governance | Product engineers (human) | Fitness functions, drift detection, structural review on significant changes. Human judgment on architecture, applied above the level of one pull request. |
| Exception handling       | Product engineers (human) | Low-confidence escalations, genuine ambiguity, novel territory outside agent competence. Reactive by design.                    |
| Meta-loop                | Product engineers (human) | Periodic calibration of gates, tools, thresholds, and agent prompts. The system reviewing itself.                                    |

### 4.2 The backlog as a portfolio

A traditional backlog is a sequential queue. An agent-swarm backlog is a portfolio of concurrent work categorised by confidence and intent.

| Category             | Description                                                                                     | Evidence Required Before Entry                              | Typical Volume |
|--------------------------|-----------------------------------------------------------------------------------------------------|-----------------------------------------------------------------|--------------------|
| Validated bets           | Features with strong evidence and clear hypothesis. High-confidence direction.                      | User research + prototype validation + experiment design        | 30–40%             |
| Experiments              | Low-cost implementations designed to test uncertain hypotheses. May not ship.                       | Problem validated; solution direction uncertain; metric defined | 30–40%             |
| Architectural investment | Debt reduction, infrastructure improvement, fitness function improvements. Enables future delivery. | Debt trend data; delivery pace signal; architectural review     | 15–25%             |
| Operational necessity    | Security patches, compliance requirements, dependency updates. Non-negotiable.                      | Vulnerability report, compliance deadline, or dependency EOL    | 5–10%              |

The portfolio mix is a product decision. A business with an 80% share on validated bets and a 5% share on experiments harvests known value while discovering little new value. A business with a 50% share on experiments learns quickly but may deliver too little. The right mix depends on business stage, market position and current evidence quality.

### 4.3 The path to exception-only

Exception-only human involvement is earned domain by domain as automation proves its reliability:

| Stage                  | Human Involvement                                                          | Gate Condition                                                                |
|----------------------------|--------------------------------------------------------------------------------|-----------------------------------------------------------------------------------|
| High oversight             | Human reviews every significant output                                         | Automation newly introduced; no reliability track record                          |
| Spot-check                 | Human samples a percentage of outputs on a cadence                             | False positive rate below threshold; no escaped defects                           |
| Exception-only             | Human responds to escalations; system runs autonomously                        | Sustained low false-positive rate; tracked reliability metrics; rollback tested   |
| Autonomous with monitoring | Human reviews the aggregate metrics and meta-loop output. Individual decisions go unreviewed | Mature, stable, monitored automation with demonstrated self-correction capability |

See [Automated SPDLC / SDLC](automated-spdlc-sdlc-vision.md) for engineering-loop verification detail and gate specifications, in Phase 3 (the implementation stage) through Phase 8 (the operations stage).

## 5. Human roles

### 5.1 Three contributions

In this model, human contribution concentrates in three areas. Other work moves towards automation.

| Contribution   | Description                                                                                                         | Why Irreducibly Human                                                                                                                                                                                |
|--------------------|-------------------------------------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Intent             | What problem are we solving, for whom, and why does it matter? Business direction, value judgments, ethical choices.    | Requires understanding of users, market, ethics, and value that cannot be derived from data alone. Agents can model options and surface tradeoffs; they cannot want something on behalf of the business. |
| Taste              | What does good look like? The aesthetic, experiential, and qualitative judgment about whether output is worth shipping. | Standards and examples can capture part of it. Human judgment must still calibrate what elegant means for this product, user and context.                                                                |
| Exception judgment | When the system surfaces genuine ambiguity, low confidence, novel territory, or ethical edge cases, a human decides.    | Reactive by design. The human stays on call and works no shift. But the decisions they make are the highest-stakes decisions in the system.                                                                 |

### 5.2 The product engineer

The “product engineer” is the primary human role in this operating model, with a different scope from the conventional job title.

A product engineer in this model holds:

- The full problem space simultaneously: user need, technical constraints, business context and competitive landscape

- Sufficient architectural understanding to recognise when agent output is structurally wrong, even when all tests pass

- Spec authorship discipline: writing precisely enough for an agent swarm to execute without constant clarification

- Product judgment: evaluating output against intent as well as the spec, which may be correct while the output still misses something

- Experiment literacy: designing experiments that produce signal rather than noise and reading results honestly

- System-level taste: operating at the portfolio level so the product forms a coherent whole instead of a collection of correct features

This person directs rather than writes code, but understands agent output at an architectural level. The relationship is closer to a director of photography with a camera crew than a manager with a development team: deep craft knowledge expressed through direction rather than execution.

The product engineer role expands the scope of both traditional product management and software engineering. It is shaped by this operating model rather than formed by combining two existing job descriptions. Recruiting and developing for this role is one of the main people challenges of the agent-native business.

## 6. Architectural governance

The human technical concern is fitness rather than authorship.

### 6.1 Architecture's purpose

Architecture should make the system adaptable, dependable and fit for its users' needs. Those needs come from the outer product and value-creation loop.

This directly affects how architectural decisions are evaluated. A decision is correct when it serves the product's current and anticipated needs better than the alternatives. Elegance, theoretical purity and technical sophistication matter only when they improve fitness.

In the agent-native model, this means architectural governance is a product concern that happens to require technical knowledge. The product engineer holds it, and no separate architecture function exists.

### 6.2 Fitness functions

An architectural fitness function is an automated check that runs against the full codebase and signals whether the system remains aligned with its intended architecture.

Fitness functions are the primary mechanism for preventing architectural drift at scale. They are to architecture what unit tests are to correctness: a specification of what must remain true, expressed as a runnable check.

| Function Type          | What It Checks                                                                                   | Tooling                             | Frequency                |
|----------------------------|------------------------------------------------------------------------------------------------------|-----------------------------------------|------------------------------|
| Dependency direction       | Layer boundaries not violated; no upward imports or cross-domain coupling                            | dependency-cruiser, import-linter       | Every PR + nightly full scan |
| Domain isolation           | Bounded contexts not bleeding into each other at the data or API level                               | Custom rules + static analysis          | Nightly                      |
| Complexity trend           | Is average complexity increasing over time? Is the codebase becoming harder to change?               | SonarQube trend, CodeClimate            | Weekly trend report          |
| API surface drift          | Is the public API surface growing unintentionally? Are contracts being quietly broken?               | OpenAPI diff, breaking change detection | Every PR touching API        |
| Test architecture          | Is the test suite structured to support the production architecture? No cross-domain test coupling.  | Custom rules                            | Nightly                      |
| Performance baseline       | Is system-level performance (not per-unit benchmark) holding against the SLOs defined in specs?      | k6, Grafana SLO tracking                | Daily comparison             |
| Observability completeness | Are all defined traces, metrics, and logs from specs actually present and flowing?                   | Custom verification, Grafana            | Daily                        |
| Tech choice drift          | Is the codebase using the agreed technology choices? Are new patterns being introduced without ADRs? | Custom linting rules, ADR audit         | Weekly                       |

### 6.3 Architectural review and governance

In a traditional team, senior engineers maintain architecture by catching structural problems in code review. That approach does not scale to an agent swarm producing parallel output.

The model shifts from review (human examining individual changes) to governance (automated fitness functions detecting drift, humans responding to exceptions and setting direction).

| Traditional Review                                 | Agent-Native Governance                                                  |
|--------------------------------------------------------|------------------------------------------------------------------------------|
| Human reviews each PR for architectural concerns       | Fitness functions run continuously; humans respond to drift signals          |
| Architectural knowledge in individual reviewers’ heads | Architectural intent expressed as runnable fitness functions                 |
| PR review as bottleneck and quality gate               | PR automation as first gate; fitness functions as structural gate            |
| Architecture enforced by convention and culture        | Architecture enforced by automated rules with explicit documented exceptions |
| Significant changes escalate to principal engineers    | Fitness function failures escalate to product engineers                      |

See [Automated SPDLC / SDLC](automated-spdlc-sdlc-vision.md), Phase 2: Design & Architecture and Phase 11: The Meta-Loop, for fitness-function implementation detail.

## 7. The meta-loop

The meta-loop lets the system review and improve its own operation.

### 7.1 Why the meta-loop is infrastructure

Product discovery, engineering execution and architectural governance all degrade without maintenance. Gates drift out of calibration. LLM capabilities improve faster than prompts are updated. Tools are superseded. Thresholds that were right six months ago produce noise now.

The meta-loop maintains the automation layer so the other loops continue to work correctly.

### 7.2 Initial automation of the meta-loop

The meta-loop can be partially automated. Automation measures and surfaces signals; people decide what to act on and how.

| Signal                | Automated Measurement                                                                               | Human Decision                                                 |
|---------------------------|---------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------|
| Gate false-positive rate  | Track per-gate: findings raised vs findings acted on. Flag gates producing \> threshold noise.          | Adjust threshold, update rules, or retire gate                     |
| Escape analysis           | Defects reaching production that all gates passed. Root cause classification.                           | Identify gate gap; add or update rule                              |
| LLM capability delta      | On major model releases: benchmark current prompts/skills against new capability. Surface improvements. | Update prompts, retire manual steps now automatable                |
| Architectural drift trend | Fitness function failure rate over time. Is drift accelerating?                                         | Add fitness function, update ADR, or address root cause in backlog |
| Pipeline cost             | CI/CD cost, LLM API cost, tooling cost per deployment. Trend and per-gate breakdown.                    | Optimise or eliminate high-cost low-signal gates                   |
| Exception volume trend    | Escalation rate per domain. Rising rate = gate miscalibration or new failure mode.                      | Investigate root cause; recalibrate or add automation              |
| Tooling currency          | Periodic scan of tooling landscape against current stack. Flag superseded tools.                        | Evaluate alternatives; schedule migration if justified             |
| Human time audit          | Where are humans spending time? Is any of it newly automatable?                                         | Identify and prioritise automation opportunities                   |

A good meta-loop calibration session can improve thousands of future automated runs. Few human activities in this operating model have a longer tail of compounding benefit.

See Automated SPDLC / SDLC, Phase 11: The Meta-Loop, for implementation detail.

## 8. Competitive advantage

The proposed advantage is structural as well as economic.

### 8.1 Traditional theory

The traditional theory holds that competitive advantage in software comes from scale: more engineers, features and data, plus network effects and distribution moats. A small team cannot out-scale a large organisation.

Agent execution changes some of those assumptions.

### 8.2 Emerging theory

When agents remove engineering capacity as the binding constraint, the scale advantage of engineering headcount weakens. A large organisation can no longer rely on more engineers to build more software faster.

Learning velocity becomes the source of advantage: discovering what users need, building it well, verifying that it works and adapting quickly.

| Scale Advantage               | Why It Attenuates                                                                              | What Replaces It                                    |
|-----------------------------------|----------------------------------------------------------------------------------------------------|---------------------------------------------------------|
| More engineers = more output      | Agent swarm scales output without headcount. Small team approaches same throughput.                | Spec quality and clarity of direction                   |
| More people = more specialisation | Specialisation creates silos, handoff costs, coordination overhead. Small team holds full context. | Full-context product engineers with broad capability    |
| Large team = more experiments     | Cost per experiment drops to near-zero. Small team runs more experiments per quarter.              | Experiment volume and quality of hypothesis design      |
| Large codebase = more features    | Large codebases accumulate debt that constrains future velocity. Greenfield starts clean.          | Architectural discipline sustained by fitness functions |

### 8.3 Compounding effect

The argument rests on compounding knowledge as well as efficiency.

An organisation that can run 10x more experiments per quarter, including genuinely uncertain ones that a larger org would not attempt due to cost, accumulates product knowledge at a rate the larger org structurally cannot match. After two or three years of operating this model, the knowledge gap is wider than the headcount gap. The small team knows their users and their product better.

Better tooling cannot remove all of a large organisation's structural drag. Coordination costs scale superlinearly with headcount. Management layers add latency to decisions. Incentive structures optimise for individual contribution visibility rather than system outcomes. These costs follow from the structure itself.

A small team with the operating model described in this document has:

- Near-zero coordination overhead

- Full system context held by every person

- Decision latency measured in minutes

- Direct incentive alignment because all roles benefit from the system working well

- No inherited technical debt because the automation is built in rather than retrofitted

The model predicts that a 3–5 person business can match the throughput, quality and value output of a 50-person organisation when engineering capacity ceases to be the binding constraint and the larger organisation retains its structural drag.

## 9. Preconditions and risks

### 9.1 Preconditions

This operating model depends on several conditions being true in practice:

| Precondition                       | What It Requires                                                                                                             | Failure Mode If Absent                                                                                            |
|----------------------------------------|----------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------|
| Automation is actually built well      | Sustained investment in calibrating gates, updating rules, maintaining the pipeline. The meta-loop runs on a schedule.           | False positives accumulate, humans start ignoring gates, quality degrades silently                                    |
| Team is genuinely high-capability      | Product engineers operate across product thinking, engineering judgment, architectural awareness and user empathy.               | Automation amplifies mediocrity as readily as capability. The model accelerates in the direction of the team's quality. |
| Product loop does real work   | Continuous discovery actually runs. Experiments are genuinely uncertain. Measurement against hypotheses is honest.               | Engineering loop builds the wrong things faster. Agent swarm executes confidently in the wrong direction.             |
| Technical discipline is non-negotiable | No accumulated debt. Fitness functions maintained. Architectural decisions documented as ADRs.                                   | Small team loses its advantage faster than a large team because there is no slack to absorb debt.                     |
| Spec quality is treated as primary     | Upstream product work is resourced and respected. Spec quality review is a hard gate.                          | Agent swarm builds confident, well-tested, wrong output. The engineering loop works perfectly against the wrong spec. |

### 9.2 Non-negotiable technical discipline

Small teams often underestimate this precondition. The model requires them to prevent technical debt or pay it down aggressively and continuously as a first-class backlog concern.

A five-person team that lets debt accumulate loses its structural advantage faster than a 50-person team. There is no slack in the system. A large team can route around slow areas, assign experts to fragile modules or live with legacy systems. The same debt can halt a small team's agent swarm because there is no institutional memory to compensate.

The architectural investment category in the portfolio (15–25% of the backlog) is not optional. It is load-bearing.

## 10. Automation trajectory

### 10.1 The frontier moves quickly

Every quarter, the frontier of what is automatable advances. LLM capabilities improve. New deterministic tooling emerges. Patterns that once required human judgment can, before long, be expressed as rules. The meta-loop exists specifically to capture these advances as they occur.

This operating model is a snapshot of a moving target. Treat the current allocation of human and automated work as provisional, and periodically re-evaluate it.

### 10.2 The stable human core

Three things are likely to remain human for the foreseeable future because of the judgments and accountability they require:

- Intent: the decision of what is worth building rests on value judgments about what matters to humans, which requires being human

- Ethical judgment: decisions with significant ethical implications require accountability that cannot be delegated to an agent

- Taste calibration: the standards against which the system runs can be captured, but the ongoing judgment of whether those standards are still right is human

Everything else is on the trajectory toward automation.

### 10.3 Practical horizon

| Timeframe | What Becomes Automatable                                                                                              | What Remains Human                                                                    |
|---------------|---------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------------------------------------------------|
| Now           | Full engineering loop execution and verification (with exception handling). Synthesis of quantitative product data.       | Spec authorship, direction, architectural governance, qualitative research interpretation |
| 1–2 years     | Spec drafting from validated direction (human reviews and refines). Prototype generation. First-pass experiment analysis. | Direction, hypothesis validation with real users, taste calibration, ethical judgment     |
| 3–5 years     | Significant portions of product loop synthesis. Architectural fitness function generation. Threat modeling automation.    | Strategic direction, genuine novel problem definition, taste at the business level        |
| Beyond        | Unknown. The frontier moves faster than predictions.                                                                      | Intent and ethics, at minimum                                                             |

The goal is to reduce waste, tedium and low-judgment work so people can focus on decisions that require human judgment.

## 11. Relationship to the SPDLC / SDLC document

This document and its companion cover different parts of the same operating model.

### This document covers

- The operating model: the why and the what

- The two-loop structure and the connection points between them

- The human roles and what they actually require

- Architectural governance as a product concern

- The competitive theory: why this model produces structural advantage

- The preconditions and risks

### The companion document covers

- Phase-by-phase automation and verification detail for the engineering loop

- Specific tooling for each phase: SAST, SCA, IaC scanning, DAST, performance testing, etc.

- The product loop phases: discovery, prototyping, validation, experimentation, GTM, deprecation

- Security automation integrated at every phase

- The path from high oversight to exception-only for each domain

- The meta-loop implementation: gate calibration, escape analysis, tool currency review

- Architectural fitness function implementation detail

See Automated SPDLC / SDLC for implementation, tooling, verification and the path to exception-only across all phases.

### Relationship

This document is the operating model. The companion document is the implementation plan. They should be read together: this document explains why the investment in the companion document is worth making, and what business outcome it is in service of.

The companion document is the living implementation guide and changes as tools improve, thresholds are recalibrated and new automation becomes possible. This operating model should change more slowly than the implementation detail.
