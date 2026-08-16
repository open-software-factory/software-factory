# Open questions and non-decisions

This document exists to prevent exploratory ideas from becoming accidental architecture decisions.

## 1. Graph implementation

Current mental model: smoothly zoomable/pannable interactive graph with live state.

Current candidate: React Flow.

Not yet decided:

- whether React Flow remains suitable at very large graph sizes,
- whether different graph lenses use the same renderer,
- whether the Factory Floor and Work Graph share a graph engine,
- graph layout strategy,
- semantic zoom behaviour,
- clustering/grouping model,
- how historical/time-travel state appears on the graph.

Design for the interaction model first. Select the graph technology after realistic prototypes.

## 2. Desktop versus web

Current direction:

- Tauri-based desktop UI first,
- React/TypeScript presentation layer,
- engine independently runnable outside the UI.

Possible later direction:

- package/reuse significant parts of the UI as a web application,
- remotely operate a cloud-hosted factory.

Do not add first-iteration complexity to guarantee browser deployment. Also avoid coupling that would prevent it later.

## 3. Ambient operator assistant

The assistant should be designed as an ambient operator interface rather than a chatbot screen.

Potential modalities:

- voice,
- command palette,
- keyboard invocation,
- context actions,
- proactive intervention cards,
- transient panels,
- direct graph annotations and visual emphasis,
- generated diagnostic views,
- optional future video avatar.

Open questions:

- voice architecture,
- local versus remote speech/model options,
- proactive-interruption policy,
- privacy and recording behaviour,
- whether avatar/video adds value or becomes gimmicky,
- how assistant actions are authorised/audited,
- how deeply the assistant can manipulate the UI versus propose actions.

## 4. Dynamic UI protocol

Some UI should be stable and predefined. Other UI may need to be generated/composed dynamically during an investigation or assistant interaction.

Possible concepts to investigate:

- AG-UI,
- A2UI,
- MCP Apps,
- typed UI schemas,
- server-driven UI,
- streamed component/data protocols.

Desired properties:

- data-driven,
- streamable,
- strongly typed where practical,
- safe and permissioned,
- actions explicitly declared,
- reusable across desktop/web/agent surfaces where possible,
- does not require arbitrary LLM-generated application code.

No protocol/framework is selected yet.

AHP is now a first-class candidate for synchronizing shared agent-session state between a factory host/gateway and operator clients. It does not define arbitrary dynamic layouts or the factory's work/lifecycle domain, so it does not select or replace a dynamic-UI protocol. See [`../../research/ahp-acp-architecture-direction.md`](../../research/ahp-acp-architecture-direction.md).

## 5. AI / agent frameworks

Potential ecosystem options mentioned for exploration include:

- Vercel AI SDK,
- Mastra,
- LangGraph,
- DeepAgents,
- PI coding agent,
- CopilotKit,
- Rust-native libraries/frameworks not yet evaluated.

These are implementation candidates. The design requires none of them.

The desired product behaviour should be specified independently of which framework currently provides it best.

## 6. Visual language

Current preference:

- dark,
- mild green/blue tint,
- subtle gradients,
- strong hierarchy,
- calm and technical,
- interactive/live without spectacle.

Still open:

- exact base palette,
- whether contextual tinting improves cognition,
- typography,
- icon system,
- radius/shadow/border language,
- graph node visual grammar,
- motion timing/easing language,
- light-theme requirement,
- degree of translucency/glass (default bias: restrained),
- how much visual inspiration to take from industrial control rooms versus modern developer tools.

Explore multiple coherent directions before locking the system.

## 7. Factory-floor topology

Open questions:

- what exactly is a persistent node versus transient event,
- whether repositories, work cells, agents and gates occupy one graph or nested layers,
- whether agents should be visible as first-class nodes or appear inside work-item execution contexts,
- how much information is visible at each zoom level,
- how to visualise congestion/starvation without overwhelming the topology,
- how overlays interact with live animation.

Current bias: show work before agents. Agents are execution resources; work and intent are durable.

## 8. Factory Runway model

The concept is agreed; exact calculation is not.

Potential inputs:

- ready executable work,
- estimated execution duration,
- historical throughput,
- confidence in estimates,
- dependency constraints,
- available parallelism,
- agent/model/tool capacity,
- runner capacity,
- work blocked by humans,
- work awaiting spec refinement.

Avoid presenting a false-precision single number until the underlying model earns it.

## 9. Meta-loop analytical model

The goal is exploratory factory intelligence that can answer questions beyond a fixed set of dashboards.

Open questions:

- event/telemetry schema,
- dimensional model,
- cohort definition model,
- experiment metadata,
- prompt/skill/model version traceability,
- relationship between OpenTelemetry-style traces and higher-level work events,
- local analytical engine versus external observability backend,
- how recommendations are generated and validated.

The UI should be able to answer new questions without requiring a new hard-coded dashboard every time.

## 10. Anti-premature-commitment rule

When implementing early prototypes:

- prefer reversible choices,
- preserve domain/event boundaries,
- collect real usage evidence,
- test with realistically dense data,
- do not generalise an abstraction merely because the first screen needs it,
- do not promote an implementation library into a product concept.
