# The product lifecycle

The outer loop: discovery, validation, experimentation, launch, measurement and retirement.

Status: living draft

This is the second document in a three-part set. The Agent-Native Operating Model establishes the two-loop vision and operating model. This document covers the outer product lifecycle. The Automated SPDLC / SDLC covers the inner engineering lifecycle. Read all three together; this document references the others where the loops connect.

## Why the product loop matters more when agents build

In a traditional software organisation, engineering capacity is the binding constraint. Product has more ideas than engineering can build, so how much the team can ship limits the product loop.

In an agent-native business, a well-directed agent swarm can remove that constraint for clearly specified work. What gets built then becomes the dominant variable. A team that runs the engineering loop well but the product loop poorly will build the wrong things efficiently, cheaply and to a high technical standard while delivering little value.

High automation makes the outer loop the primary determinant of whether the business creates value.

As build cost falls, “should we build this?” matters more than “can we build this?” The product loop supplies the evidence and judgment needed to answer the first question.

## Where this document fits

| Document                          | Covers                                                                                                                                                 | Primary Reader                                                        |
|---------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------|
| Agent-Native Operating Model          | The two-loop structure, human roles, competitive theory, architectural governance, the meta-loop. The why and the what.                                    | Anyone setting direction for the business                                 |
| The Product Lifecycle (this document) | The outer loop: discovery, validation, experimentation, launch, measurement, deprecation. How to run the product loop well, with honest automation levels. | Product engineers; anyone owning the outer loop                           |
| Automated SPDLC / SDLC                | The inner loop: phase-by-phase engineering automation, verification tooling, security, the path to exception-only. The implementation detail.              | Product engineers; anyone building or maintaining the automation pipeline |

The automation ceiling on the product loop is lower than on the engineering loop because product decisions are probabilistic. Evidence supports a belief about what users want, but people still make the judgment. Automation reduces the cost of gathering and synthesising evidence so the product loop can run more cycles per unit time.

## Product principles

Product development has principles accumulated through decades of practice and codified by practitioners such as Teresa Torres, Marty Cagan and Eric Ries. They describe how teams make product decisions under uncertainty.

These principles survive the shift to agent-native work because the challenge they address has not changed. The cost structure does change, making some previously aspirational practices achievable.

### Principles that survive unchanged

These principles address the problem of building products for people under uncertainty, which the agent-native shift does not change.

| Principle                                | What It Means                                                                                                                                                                                              | Why It Survives                                                                                                                                                                |
|----------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Outcome over output                          | The measure of success is whether user behaviour changed in the intended direction rather than whether the feature shipped.                                                                                   | An agent swarm can produce output at very high speed. Without this principle, it produces a lot of output and very little outcome. Automation increases this risk.               |
| Evidence over opinion                        | Product decisions should be grounded in observed user behaviour and direct user evidence, not in what the team believes users want.                                                                            | Teams with high engineering throughput are the most exposed to this failure mode. Executing a wrong direction quickly costs more than executing it slowly.     |
| Continuous discovery over big-bang research  | Discovery is a permanent background process, not a phase that concludes before development begins.                                                                                                             | In an agent-native world, the engineering loop executes continuously. The product loop must also run continuously or the engineering loop runs ahead of validated direction.       |
| Problems before solutions                    | A validated problem statement must exist before any solution direction is explored. Jumping to solutions is the most common and expensive product failure mode.                                                | LLMs generate solutions rapidly and fluently. Without this discipline, the product loop collapses into fast, confident and frequently wrong solution generation.                  |
| Small and frequent over large and infrequent | Smaller bets with faster feedback loops accumulate more learning per unit time than large bets with long feedback loops.                                                                                       | Agent execution makes smaller bets cheap. The cost of a small experiment is now genuinely low. This principle becomes easier to follow as execution gets cheaper.                                |
| Kill as a first-class decision               | Stopping work on a direction when evidence says it is wrong is not failure. It is the product loop functioning correctly. The cost of continuing a wrong direction is always higher than the cost of stopping. | With high execution velocity, the cost of continuing a wrong direction compounds faster. The discipline of killing things matters more as velocity rises.                             |
| Separation of problem and solution space     | The team should hold the problem space open long enough to explore multiple solution directions before committing. Premature convergence is a persistent failure mode.                                         | LLM-assisted hypothesis generation makes exploring multiple directions cheap. The principle becomes easier to follow and the excuse for skipping it disappears.                    |
| User proximity                               | The people making product decisions should have direct, regular contact with the users those decisions affect. Mediated, second-hand understanding of users consistently produces worse decisions.             | No automation substitutes for this. Synthesis tools reduce the cost of processing what users say; they do not replace the judgment that comes from direct exposure.                |

### Previously aspirational practices that become achievable

Teams consistently underinvested in these valuable practices because their cost was too high relative to delivery pressure.

In an agent-native world, two things change. First, delivery pressure from engineering capacity is removed as a forcing function. Second, the automation layer reduces the cost of several of these practices directly.

| Practice                               | Always Known to Be Valuable                                                                                                                        | Why It Was Underinvested                                                                                           | Why It Is Now Achievable                                                                                                                        |
|--------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------|
| Experiment design before build             | Defining success criteria before seeing results prevents post-hoc rationalisation and produces honest learning.                                        | Under delivery pressure, teams shipped and measured informally. Writing a proper experiment design felt like overhead. | Delivery pressure from engineering is reduced. Experiment design is a short, structured step that agents can template and LLMs can quality-check.   |
| Regular user research cadence              | Consistent user contact produces better product decisions than periodic research bursts.                                                               | Recruiting, scheduling, and synthesising research is time-consuming. It got cut when sprints were full.                | Synthesis is substantially LLM-assisted. Recruitment tooling is more automated. The cadence is now maintainable by one person.                      |
| Decision documentation                     | Recording what was decided, what evidence supported it, and what alternatives were rejected builds institutional memory and makes reasoning auditable. | Documentation took time and was rarely consulted. It fell behind delivery.                                             | LLM-assisted documentation generation from structured inputs makes this near-zero cost. The research repository makes it searchable.                |
| Multiple solution hypotheses before a spec | Exploring meaningfully different directions before committing produces better decisions than assuming the first solution that comes to mind.           | Generating and evaluating multiple options took time. Teams defaulted to the first viable idea.                        | LLMs generate a broad hypothesis set rapidly. The human evaluates and selects. The exploration cost collapses.                                      |
| Explicit deprecation discipline            | Actively retiring features that no longer earn their maintenance cost keeps the product coherent and the codebase lean.                                | Retiring features was politically difficult and operationally involved. It was easier to leave things running.         | Usage analytics surface retirement candidates automatically. Agent execution makes the code removal cheap. The barrier is now the decision itself, since the effort is small. |

These are established product principles whose cost barrier is now lower.

### The automation ceiling

The product loop has a lower automation ceiling than the engineering loop because the nature of its decisions differs.

Engineering correctness is deterministic: the code either does what the spec says or it does not. Verification is possible in principle for any property that can be specified. The automation ceiling is high because the domain is deterministic.

Product decisions are probabilistic: evidence supports a belief that something will be valuable, but cannot make that decision certain. Judging whether the evidence is sufficient, the problem is real and the direction is right requires understanding the users, market and values of the business. This uncertainty belongs to the work rather than the tooling.

Automation should reduce the cost of evidence, speed up synthesis and surface signals earlier. People remain responsible for the decision.

## 0. The outer loop

This section defines its structure, cadence and relationship to the inner loop.

### 0.1 Why this loop exists

The engineering loop is very good at one thing: building what it is told to build, correctly and verifiably. It has no mechanism for determining whether what it builds is worth building. That is entirely the product loop's responsibility.

The product loop asks whether the work matters, the hypothesis is valid and the intended outcome was achieved. These questions require evidence from user behaviour, market signals and experiment results, followed by human judgment.

### 0.2 Loop structure

| Stage                     | Primary Question                                             | Output                                                 | Cadence                      |
|-------------------------------|------------------------------------------------------------------|------------------------------------------------------------|----------------------------------|
| Continuous discovery          | What problems are real and worth solving?                        | Problem inventory, ranked by evidence weight               | Always running                   |
| Problem definition            | Is this specific problem validated and scoped?                   | Validated problem statement with evidence base             | Per opportunity                  |
| Opportunity assessment        | Is solving this strategically valuable?                          | Go / no-go with documented rationale                       | Per opportunity                  |
| Solution hypotheses           | What are the plausible ways to solve this?                       | Hypothesis set with differentiated risk and value profiles | Per validated problem            |
| Prototype & design validation | Does the proposed solution resonate with users?                  | Evidence of user response; refined direction               | Per hypothesis                   |
| Experiment design             | How will we know in production whether we were right?            | Hypothesis, success metric, instrumentation spec           | Before spec is written           |
| Spec                          | What precisely should be built?                                  | Validated spec entering the engineering loop               | Per decided direction            |
| Beta / early access           | How do real users respond to the built thing before full launch? | Structured feedback; graduation or iteration decision      | Per feature or release           |
| Launch & GTM                  | Are users aware of and able to use the new capability?           | Activated user base; documented launch                     | Per feature or release           |
| Measurement                   | Did we achieve the intended outcome?                             | Outcome verdict: ship / iterate / kill / scale             | Post-launch, per experiment      |
| Deprecation & retirement      | Is this feature still earning its maintenance cost?              | Retirement decision and execution plan                     | Ongoing; triggered by usage data |

### 0.3 Verification model

The engineering loop uses hard pass/fail gates: the code either compiles, passes tests, and clears security scans or it does not. There is a correct answer.

The product loop uses evidence thresholds and decision-quality checks. There is rarely one correct answer, only decisions supported by stronger or weaker evidence. Verification asks whether there is enough evidence to decide with appropriate confidence.

| Checkpoint            | What Is Verified                                                                              | By Whom       | Threshold                                                                   |
|---------------------------|---------------------------------------------------------------------------------------------------|-------------------|---------------------------------------------------------------------------------|
| Problem validation        | Is the problem documented with direct user evidence?                              | Human             | Minimum N user sources + usage data signal                                      |
| Opportunity assessment    | Is the go/no-go documented with strategic rationale and opportunity cost acknowledged?            | Human             | Written rationale covers: value, cost, alternatives, risk                       |
| Hypothesis quality        | Does the hypothesis specify: user, action, outcome, and measurable signal?                        | Automated + Human | LLM scores against hypothesis quality rubric; human reviews                     |
| Experiment design quality | Is the success metric defined, binary, and measurable before launch? Is sample size calculated?   | Automated + Human | Metric defined; sample size documented; measurement infrastructure specced      |
| Spec entry gate           | Has the direction been validated through at least one prototype/user test or equivalent evidence? | Human             | Evidence referenced in spec; no pure-assumption specs enter engineering loop    |
| Outcome measurement       | Was the success metric reached? Was the verdict documented?                                       | Automated + Human | Experiment closed with explicit verdict; no open experiments older than N weeks |

The spec entry gate prevents work from entering the engineering loop before at least one cycle of evidence gathering. Agents build what the spec says; this gate checks that the spec reflects validated intent.

## 1. Continuous discovery

This always-on signal layer surfaces problems before they are scheduled.

### 1.1 What continuous discovery means

Discovery is not a phase that happens before development begins. It is a permanent background process. While the engineering loop executes current validated specs, the product loop is always collecting signals about what the next problems are.

Treating discovery as a phase allows its evidence to become stale before development finishes. Continuous discovery keeps an evidence-weighted problem inventory current.

### 1.2 Signal sources

| Signal Source                | What It Surfaces                                                                                 | Automation Level                                                      | Cadence                                    |
|----------------------------------|------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------|------------------------------------------------|
| User interviews                  | Latent needs, workarounds, mental models, frustrations that quantitative data cannot see             | Low: scheduling assistable; synthesis LLM-assisted; interpretation human | Regular cadence: weekly or fortnightly minimum |
| Support ticket analysis          | Recurring pain points, failure modes, missing features users are working around                      | High: LLM categorisation and theme extraction across ticket corpus       | Weekly automated digest                        |
| Usage analytics                  | Where users go, where they stop, what they ignore, feature adoption rates                            | High: funnel and cohort analysis automated; interpretation human         | Continuous collection; weekly review           |
| In-product surveys (Sprig)       | Contextual feedback triggered by specific behaviours captures intent at the moment of action         | Medium: trigger logic automated; synthesis LLM-assisted                  | Continuous; triggered by events                |
| Session recordings               | Confusion, hesitation, repeated attempts, unexpected navigation paths                                | Medium: anomaly flagging automated; review human                         | Sampled; flagged sessions reviewed weekly      |
| Competitive signals              | What alternatives are building; market direction; pricing changes                                    | Medium: monitoring and summarisation automatable                         | Weekly automated scan; human review            |
| Sales and customer conversations | What prospects ask for; why deals are lost; what customers threaten to leave for                     | Low: CRM notes searchable; synthesis LLM-assisted; capture is manual     | Ongoing; reviewed in discovery sessions        |
| Feature usage trends over time   | Features losing adoption signal retirement candidates; rising adoption signals expansion opportunity | High: automated trend tracking with threshold alerts                     | Monthly trend digest                           |

### 1.3 Research repository

All discovery signals should flow into a structured, searchable research repository that the product loop can use when evaluating problems and opportunities.

Without a repository, discovery knowledge lives in people's heads and in scattered notes. When the person who did the interview leaves, the evidence leaves with them. The repository is the institutional memory of the product loop.

- Interview transcripts and synthesised insights stored and tagged by theme

- Quantitative signals linked to qualitative context where possible

- Evidence linked to the decisions it informed (traceability in the product loop)

- Searchable when evaluating new opportunities: "what do we already know about this problem?"

Dovetail is a purpose-built tool for this. It stores research artefacts, supports LLM-assisted theme extraction and links evidence to insights and decisions.

## 2. Problem definition and opportunity assessment

This stage turns a signal into a decision about whether the problem is worth solving.

### 2.1 Problem definition

A signal from discovery is not a problem statement. "Users are complaining about the export feature" is a signal. "Users who export more than 10 records per week cannot complete the task without switching to a manual workaround because the export does not support their required format" is a problem statement.

The problem statement must be:

- Specific: scoped to a user segment and a context rather than "users generally"

- Evidence-grounded: references the signals that surfaced it

- Solution-free: describes the problem rather than a pre-decided answer

- Outcome-oriented: what is the user trying to achieve that they cannot?

- Sized: rough frequency and severity to inform priority

LLM-assisted review can flag a problem statement that contains a pre-decided solution, lacks a user segment or lacks cited evidence. Human review must still decide whether the problem is real.

### 2.2 Opportunity assessment

Not every validated problem is worth solving. The opportunity assessment is the explicit, documented decision about whether to proceed.

| Dimension             | Question                                                                         | Notes                                                                                                               |
|---------------------------|--------------------------------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------|
| Strategic fit             | Does solving this advance our current product direction and user value proposition?  | Explicit link to current strategy required. "Nice to have" is not sufficient.                                           |
| User impact               | How many users are affected? How severely? How frequently?                           | Frequency × severity × reach all matter. A rare but catastrophic problem may outrank a frequent minor irritant.         |
| Feasibility signal        | Is a solution technically viable within current architecture?                        | A feasibility signal from product engineers is required rather than full scoping.                                       |
| Opportunity cost          | What is not being solved because we are solving this?                                | Always explicit. "We chose this over X because..." must be documentable.                                                |
| Reversibility             | If this turns out to be wrong, how expensive is it to undo?                          | Informs how much validation is required before committing to build.                                                     |
| Competitive context       | Does solving this differentiate or achieve parity?                                   | Both are valid reasons with different urgency and ambition.                                                             |
| Compliance / legal        | Does this touch regulatory requirements, terms of service, or intellectual property? | Must be identified before design begins.                                                          |
| i18n / localisation scope | Is this launching globally or in specific markets? Does copy, format, or RTL matter? | Identified here, not discovered when engineering is complete.                                                           |

## 3. Solution hypotheses and prototype validation

This is the cheapest point to discover that a direction is wrong.

### 3.1 Why multiple hypotheses come before a spec

The transition from problem to spec is where most product processes collapse into a single assumed answer. Someone writes a spec for the first solution that came to mind, calls it "validated," and sends it to engineering.

A set of meaningfully different solution hypotheses should sit between a validated problem and a spec. Their distinct risk and value profiles make the choice explicit before design begins.

LLMs can generate a broad set of solution hypotheses cheaply, including approaches the team may not have considered. People evaluate and select among them.

### 3.2 Prototype validation

A prototype is the cheapest way to test whether a direction is correct before committing engineering resources. The appropriate fidelity depends on the uncertainty being tested:

| Fidelity                 | When to Use                                                    | What It Can Test                                                                       | Tooling                                          |
|------------------------------|--------------------------------------------------------------------|--------------------------------------------------------------------------------------------|------------------------------------------------------|
| Sketch / wireframe           | Very early direction uncertainty: "does this concept make sense?" | Concept comprehension, basic navigation logic, gross structural problems                   | Paper, Excalidraw, Balsamiq                          |
| Low-fidelity prototype       | Direction selected; testing flow and task completion               | Task completion rates, where users get confused, whether the flow matches the mental model | Figma (low-fi), Maze for unmoderated testing         |
| High-fidelity prototype      | Flow validated; testing visual design and copy                     | Copy clarity, visual hierarchy, emotional response, interaction detail                     | Figma (hi-fi), Maze, Lookback for moderated sessions |
| Functional prototype / spike | Technical uncertainty: "can we actually build this?"               | Technical feasibility, performance characteristics, integration complexity                 | Working code; throwaway                              |

### 3.3 Design-system compliance

Build prototypes within the design system from the start to avoid rework. Design-system compliance is checkable:

- Figma component usage: is the prototype using design system components or creating new ones?

- Accessibility in design: colour contrast, touch target size, and focus order are checkable at the prototype stage, before engineering begins

- New patterns: if the prototype introduces a pattern not in the design system, that is a decision to be made explicitly, documented, and the design system updated

Accessibility verification continues into the engineering loop. See [Automated SPDLC / SDLC](automated-spdlc-sdlc-vision.md), Phase 3: Implementation and Phase 6: Staging, for axe-core and WCAG testing detail.

## 4. Experiment design

Define how success will be measured before writing the spec.

### 4.1 Experiment design as a pre-spec requirement

Complete the experiment design before writing the spec. Reversing the sequence turns the experiment into post-hoc justification for a decision already made.

The experiment design answers: if we build what the spec describes, how will we know whether it worked? Without an answer before the build begins, “done” means deployed rather than validated.

### 4.2 Elements of a well-formed experiment

| Element          | Description                                                                                                             | Common Failure Mode                                                                                         |
|----------------------|-----------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------------|
| Hypothesis           | "We believe \[user segment\] will \[action\] because \[reason\], resulting in \[measurable outcome\]." Must be falsifiable. | Vague: "users will find this useful." It names no measure and cannot be proved wrong.                                          |
| Primary metric       | One metric that the experiment will be evaluated against. Binary: success or not.                                           | Multiple metrics with no primary allow cherry-picking after the fact.                                          |
| Guardrail metrics    | Metrics that must not degrade. The feature can succeed on its primary metric while breaking something else.                 | Without guardrails, a feature can improve activation but destroy retention, noticed only months later.          |
| Success threshold    | The specific value the primary metric must reach. Defined before launch.                                         | Threshold set after seeing results. Survivor bias. The experiment tells you what you want to hear.              |
| Minimum sample size  | Required exposure to reach statistical significance at the desired confidence level.                                        | Calling the experiment too early. Noise is mistaken for signal.                                                 |
| Maximum duration     | How long the experiment runs before a verdict is required, regardless of sample size.                                       | Experiments left open indefinitely. Decision fatigue. The backlog fills with unresolved experiments.            |
| Instrumentation spec | Exactly what events, properties, and user attributes must be captured. This becomes a spec requirement for engineering.     | Instrumentation added as afterthought. Experiment launches with missing data. Cannot be measured retroactively. |
| Rollout plan         | What percentage of users see the feature and how that scales. Feature flag configuration.                                   | Full rollout to all users before measurement window closes. Impossible to control for the experiment.           |

### 4.3 Experiment economics in the agent-native world

In a traditional engineering organisation, each experiment variation requires significant engineering time, QA cycles and coordination. The build cost pushes teams towards high-confidence bets that they expect to win, reducing how much they learn.

When agent execution reduces an experiment's build cost from weeks to hours, failed experiments become cheaper. Teams can test directions they are uncertain about and learn from the result.

An agent-native business can therefore allocate more of its backlog to uncertain experiments. A team that runs only high-confidence experiments underuses the capability.

Lower build cost makes previously unaffordable experiments practical. After two to three years at this volume, the team's product knowledge can compound faster than in a traditional organisation.

## 5. Beta and early access

Beta and early access provide controlled real-user exposure before full launch. They serve a different purpose from canary deployment.

### 5.1 Beta as a product mechanism

A canary deployment exposes a small percentage of users to new code to catch technical regressions. It is a safety mechanism. A beta programme exposes a selected cohort of users to a new feature to gather structured product feedback before full launch. It is a learning mechanism. These are different things and should not be conflated.

The canary is managed by the engineering loop. The beta programme is managed by the product loop. They may run simultaneously on the same feature, serving different purposes.

### 5.2 Beta programme structure

| Element                      | Description                                                                                                                                                                                          |
|----------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Cohort selection                 | Criteria for beta participants: power users, representative users, customers who reported the underlying problem, or users in a specific segment. Selection is deliberate.                   |
| Feedback mechanism               | Structured and scheduled. In-product prompts at key moments, scheduled check-in sessions, dedicated feedback channel. The feedback collected should map to the experiment's primary and guardrail metrics. |
| Success criteria                 | What must be true for the feature to graduate from beta to GA? Defined before beta launches. Typically: primary metric threshold reached, critical bugs resolved, support team briefed.                  |
| Duration                         | Minimum and maximum beta period defined upfront. Minimum to accumulate meaningful signal; maximum to avoid indefinite beta that blocks roadmap.                                                          |
| Communication to participants    | Beta participants should know they are in a beta, what is expected of them, and what will happen with their feedback. "We changed X because you told us Y" closes the loop and builds trust.             |
| Graduation or iteration decision | Explicit verdict at the end of the beta period. Graduate to GA, iterate and extend beta, or kill. Documented with rationale.                                                                             |

## 6. Launch and go-to-market

A shipped feature creates no product value if users cannot discover or use it.

### 6.1 Deployment and launch

Deployment is the technical event in which code reaches production with gates passed and monitoring active. Launch is the product event in which users learn that the capability exists, understand it and can use it. Both must happen.

A deployed feature that never launches fails to deliver value. Deployment completes the engineering loop but not the product loop.

### 6.2 Launch-readiness checklist

| Area                  | What Must Be Ready                                                                                      | Automatable?                                                                     |
|---------------------------|-------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------|
| Documentation             | User-facing documentation published and accurate. API documentation updated and live if applicable.         | Partially: doc generation from OpenAPI automatable; user-facing copy human-authored |
| In-product discovery      | How do existing users learn about the new feature? Announcement, tooltip, onboarding flow, changelog entry. | Low: content is human-authored; distribution mechanism is automatable               |
| Support readiness         | Support team briefed on what the feature does, common questions, and known limitations.                     | Low: briefing doc generation assistable; knowledge transfer human                   |
| Sales enablement          | For B2B products: sales team can demo and pitch the new capability. Talking points documented.              | Low                                                                                  |
| Changelog / release notes | User-facing changelog updated. Internal release notes generated from commits and specs.                     | Medium: internal release notes highly automatable; user-facing copy human-edited    |
| Experiment active         | The measurement experiment is running. Instrumentation verified as flowing before launch.                   | High: instrumentation verification automated; experiment configuration human        |
| Deprecation notices       | If this replaces existing functionality, advance notice has been given to users with a migration path.      | Medium: notice distribution automatable; content human-authored                     |
| Legal / compliance        | Terms of service, privacy policy, or DPA changes reviewed and published if required.                        | Low: review is human; distribution automatable                                      |

### 6.3 Launch as a connection to the engineering loop

The launch step requires several things from the engineering loop to be complete before it can proceed: deployment successful, smoke tests passing, feature flags configured correctly, monitoring active, and instrumentation verified flowing. The product loop should not attempt to launch until it has confirmation of these from the engineering loop.

See Automated SPDLC / SDLC, Phase 7: Production Deployment, for the engineering loop's deployment-gate detail.

## 7. Measurement

“Done” means the hypothesis is resolved rather than merely shipped.

### 7.1 Closing the experiment

Every feature designed as an experiment, which in this operating model is most features, must close with an explicit verdict. An open experiment keeps a feature flag active, consumes monitoring overhead and leaves the product decision unresolved.

| Verdict  | Criteria                                                                                            | Next Action                                                                                                                          |
|--------------|---------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------|
| Ship / scale | Primary metric reached or exceeded. Guardrail metrics held. Statistical significance achieved.          | Feature flag removed or set to 100%. Instrumentation transitions to long-term monitoring. Findings documented and fed back to discovery. |
| Iterate      | Signal is directionally positive but primary metric not reached. Clear hypothesis about what to change. | Specific iteration scoped. New experiment designed with updated hypothesis. Original experiment closed.                                  |
| Kill         | Primary metric not reached. No clear hypothesis about why. Or: guardrail metric degraded unacceptably.  | Feature rolled back. Code removed or flagged for cleanup. Learnings documented. Problem re-enters discovery with richer context.         |
| Inconclusive | Insufficient sample size reached within maximum duration. Statistical significance not achievable.      | Decision on whether to extend, redesign the experiment, or make a judgment call. Must not stay open indefinitely.                        |

### 7.2 Measurement tooling

The measurement infrastructure serves two purposes: closing individual experiments and feeding continuous discovery. Usage data that closes one experiment becomes the signal that surfaces the next problem.

| Tool             | Primary Use                                                                                                   | Strength                                                                                                           |
|----------------------|-------------------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------|
| PostHog              | Product analytics + feature flags + session recording + A/B testing in one stack                                  | Open-source, self-hostable; full data ownership; strong API; events, funnels, cohorts, feature flags unified           |
| Amplitude / Mixpanel | Product analytics: funnel analysis, cohort analysis, retention curves, behavioural segmentation                   | Mature; strong querying; Amplitude has stronger behavioural prediction; Mixpanel has more flexible querying            |
| Statsig              | Experimentation platform with statistical rigour: sequential testing, CUPED variance reduction, guardrail metrics | Purpose-built for experimentation; statistical sophistication beyond simple A/B; integrates with most analytics stacks |
| GrowthBook           | Open-source experimentation: feature flags, A/B tests, statistical analysis                                       | Self-hostable; connects to existing data warehouse (BigQuery, Snowflake, etc.); flexible for technical teams           |
| LaunchDarkly         | Feature flag management at scale with experimentation                                                             | Most mature flag platform; strong for complex flag logic and targeting; expensive at scale                             |

For a small, technically capable team, this vision favours PostHog as the primary analytics and flag platform, supplemented by Statsig or GrowthBook when experiments need more statistical rigour. Self-hosting matters for products that handle sensitive user data.

## 8. Deprecation and retirement

A feature's lifecycle continues until it is retired.

### 8.1 Why deprecation is a product concern

Products accumulate. Without an active removal discipline, every feature shipped adds permanent maintenance cost, increases the cognitive surface area of the product for users, and adds to the complexity that agents must navigate when making changes.

Retiring a feature that served its purpose is not a failure. Replacement by something better or usage below its maintenance cost are valid product-loop outcomes.

### 8.2 Retirement triggers

- Usage below threshold for a sustained period (automated alert from analytics)

- A new feature fully replaces the functionality of an older one

- User feedback consistently indicates the feature creates more confusion than value

- Maintenance cost has grown disproportionate to usage (often following a significant refactor that exposes how much complexity the feature adds)

- Compliance or security requirement that cannot be met without rebuilding from scratch

### 8.3 Retirement process

| Stage         | Action                                                                                                                   | Timeline                                  |
|-------------------|------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------|
| Decision          | Explicit go/no-go on retirement. Rationale documented. Usage data cited. Migration path for existing users defined.          | As early as possible                          |
| Advance notice    | Users informed of deprecation timeline. Minimum notice period appropriate to user impact. Migration documentation published. | N months before end-of-life; varies by impact |
| Migration support | Tooling or guidance for users to migrate to the replacement. Automated migration where feasible.                             | Published with advance notice                 |
| End-of-life       | Feature disabled. Code removed or flagged for removal. Spec marked retired.                                                  | On published date                             |
| Post-retirement   | Code fully removed from codebase in next cleanup cycle. ADR updated. Fitness functions updated to reflect removed patterns.  | Next scheduled cleanup sprint                 |

Track post-retirement code removal explicitly. Code left behind can be referenced by agents, remain covered by tests and add noise to architectural reasoning. Retirement is complete only when the code is gone.

## 9. Product tooling landscape

The following tools support the outer loop at different automation levels.

### 9.1 Research and synthesis

| Tool           | Category                     | Automation Level | Key Capability                                                                                                                              |
|--------------------|----------------------------------|----------------------|-------------------------------------------------------------------------------------------------------------------------------------------------|
| Dovetail           | Research repository              | High (synthesis)     | Central store for qualitative research; LLM-assisted theme extraction; links evidence to insights to decisions; searchable institutional memory |
| UserTesting        | Moderated / unmoderated sessions | Medium               | Moderated and unmoderated user testing; participant recruitment; emerging LLM synthesis of session recordings                                   |
| Maze               | Prototype testing                | High (quantitative)  | Unmoderated usability testing on Figma prototypes; task completion rates, time-on-task, misclick rates; API for integration                     |
| Lookback           | Moderated sessions               | Low                  | Moderated interview and usability session recording; team observation; timestamped notes                                                        |
| Sprig              | In-product research              | High (triggering)    | Micro-surveys and concept tests triggered by user behaviour in-product; captures intent at the moment of action                                 |
| Hotjar / FullStory | Session recording + heatmaps     | Medium (flagging)    | Session recording, heatmaps, rage-click detection; FullStory has strong events API and anomaly detection capabilities                           |

### 9.2 Analytics and experimentation

| Tool     | Category          | Automation Level | Key Capability                                                                                                                                                    |
|--------------|-----------------------|----------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| PostHog      | All-in-one            | High                 | Analytics, feature flags, session recording, A/B testing; open-source and self-hostable; strong API; recommended as primary stack for technically capable small teams |
| Amplitude    | Product analytics     | High                 | Funnel analysis, cohort analysis, retention curves, behavioural prediction; strong for understanding where users drop off and why                                     |
| Mixpanel     | Product analytics     | High                 | Flexible event-based analytics; strong custom querying; better for teams that want to define their own metrics rather than use preset reports                         |
| Statsig      | Experimentation       | High                 | Sequential testing, CUPED variance reduction, guardrail metric monitoring, holdout groups; statistical rigour purpose-built for product experimentation               |
| GrowthBook   | Experimentation (OSS) | High                 | Open-source; connects to existing data warehouse; Bayesian and frequentist options; strong for teams with data infrastructure already in place                        |
| LaunchDarkly | Feature flags         | High                 | Most mature flag platform; complex targeting rules, percentage rollouts, user segment management; expensive but reliable at scale                                     |

### 9.3 Feedback and prioritisation

Five products appear below, in four categories. Canny is a board where users post requests and vote on them. ProductBoard is a planning tool. Linear is an issue tracker. Intercom is a customer-messaging platform. Zendesk is a support-ticketing platform.

| Tool           | Category        | Key Capability                                                                                                                                                           |
|--------------------|---------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Canny              | Feedback management | Structured feedback intake, public roadmap, voting; links user feedback to features; good for surfacing frequently requested problems                                        |
| ProductBoard       | Product management  | Links user evidence to features to outcomes; insight repository; prioritisation scoring; the most product-loop-native of the PM tools                                        |
| Linear             | Issue tracking      | Fast, opinionated; excellent for linking specs to engineering tasks; beloved by small technically-oriented teams; not a product loop tool but a strong inner loop complement |
| Intercom / Zendesk | Support + feedback  | Support conversations as discovery signal; LLM synthesis over ticket corpus to extract product themes is high-value and underused                                            |

### 9.4 Design and documentation

| Tool              | Category                          | Key Capability                                                                                                                            |
|-----------------------|---------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------|
| Figma                 | Design + prototyping                  | Prototyping, design system management, developer handoff; API enables automation of design token extraction and component usage auditing      |
| Storybook + Chromatic | Component library + visual regression | Bridge between design and engineering: component documentation, visual regression testing, design system compliance checking                  |
| ReadMe                | Developer documentation               | Developer-facing documentation with interactive API explorer; syncs with OpenAPI spec; keeps documentation current with the API automatically |
| Mintlify              | Documentation platform                | Modern docs platform; GitHub integration for automated publishing on merge; good for products with significant developer-facing documentation |

## 10. The automation ceiling

The gap between automated and human work follows from the type of judgment involved.

### 10.1 Automation map

| Activity                                             | Automation Level | Why                                                                                                                                                     |
|----------------------------------------------------------|----------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Quantitative data analysis (funnels, retention, cohorts) | High                 | Deterministic computation. Automated pipelines, scheduled reports, anomaly detection.                                                                       |
| Research synthesis (themes from transcripts, tickets)    | High                 | LLMs are genuinely good at extracting themes from large text corpora. Synthesis is assisted; sense-making is human.                                         |
| Experiment statistical analysis                          | High                 | Fully deterministic once data exists. Significance, power, confidence intervals, CUPED.                                                                     |
| Hypothesis quality review                                | Medium               | LLM can reliably flag missing elements (a missing user segment, a missing metric, a claim that cannot be proved wrong). Human refines.                                                        |
| Competitive signal monitoring                            | Medium               | Web scraping + LLM summarisation. Noisy but automatable. Signal quality varies.                                                                             |
| Prototype accessibility and design system compliance     | Medium               | Rule-based checks on Figma files. Catches known violation patterns. Does not catch poor design.                                                             |
| Problem statement quality review                         | Medium               | LLM can flag solution-framing, missing evidence, missing user segment. Human judges whether the problem is real.                                            |
| Solution hypothesis generation                           | Medium-High          | LLMs generate plausible directions rapidly. Human evaluates and selects. Breadth at near-zero cost.                                                         |
| Survey distribution and response aggregation             | High                 | Distribution, collection, and quantitative aggregation are fully automated. Interpretation is human.                                                        |
| User need validation (interviews, usability tests)       | Low                  | The human being the user cannot be substituted. Moderated testing requires a human interviewer. Unmoderated testing reduces this but does not eliminate it. |
| Strategic direction and prioritisation                   | Low                  | Requires value judgments about what matters to users and to the business. AI can model tradeoffs and surface options. Decision is human.                    |
| Product taste and aesthetic judgment                     | Low                  | What feels right for this product and these users at this moment. Partially capturable in standards; calibration of standards is human.                     |

### 10.2 Why the gap is structural

Product decisions are probabilistic. They ask whether something is likely to be valuable given current evidence. Answering requires human judgment about what matters as well as what is true.

A more capable model can synthesise evidence faster and generate more plausible options. It cannot want something on behalf of the business, feel what a user feels or accept accountability for the ethical and strategic context behind a decision.

Automation reduces the cost of accumulating and synthesising the evidence that informs human judgment. It does not replace that judgment.

## 11. Connection points to the engineering loop

The outer loop sends work to and receives evidence from the inner loop.

### 11.1 Outbound: product loop to engineering loop

| Connection                  | What Flows                                                                                                             | Quality Gate                                        |
|---------------------------------|----------------------------------------------------------------------------------------------------------------------------|---------------------------------------------------------|
| Spec                            | Validated direction: complete, unambiguous, testable, with acceptance criteria and experiment instrumentation requirements | Spec quality gate; see Automated SPDLC / SDLC, Phase 1: Requirements and specification  |
| Experiment instrumentation spec | Exact events, properties and user attributes that must be captured before engineering begins                              | Verified as present in code before experiment launch    |
| Debt prioritisation signal      | Which technical debt is blocking product directions the business cares about                                               | Engineering loop incorporates into backlog portfolio    |
| Deprecation decision            | Feature retirement decision with timeline that triggers code-removal planning                                              | Engineering loop tracks and executes removal            |

### 11.2 Inbound: engineering loop to product loop

| Connection          | What Flows                                                                | How Product Loop Uses It                                                                   |
|-------------------------|-------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------|
| Production evidence     | Usage data, error rates and performance signals continuously feed discovery  | Weekly digest reviewed in discovery cadence; anomalies trigger immediate review                |
| Feasibility signal      | Technical constraints that shape what solution directions are viable          | Consulted during solution hypothesis generation and opportunity assessment                     |
| Delivery pace signal    | Current throughput capacity, debt level, architectural risk                   | Informs product planning cadence and scope; if pace is constrained, product loop reduces scope |
| Incident findings       | What product assumptions the incident revealed as wrong                       | Fed back into discovery; may reopen a problem statement or invalidate a hypothesis             |
| Deployment confirmation | Feature is live, instrumentation verified flowing, monitoring active          | Required before launch/GTM steps proceed                                                       |

See Agent-Native Operating Model, Section 2.3: Connection points, for the complete two-loop connection model.

See Automated SPDLC / SDLC, Phase 1: Requirements & Specification, for the spec quality gate that governs entry into the engineering loop.
