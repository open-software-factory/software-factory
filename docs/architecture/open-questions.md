# Open Questions

These are architectural questions to resolve through implementation and research rather than assumptions.

## Kernel boundary
- What is the smallest genuinely novel responsibility of the factory?
- Is the kernel primarily an orchestrator, capability registry, policy engine, execution engine, or some composition of these?
- What should remain in skills/harnesses instead of the factory?

## Workflow and capabilities
- How are reusable skills/capabilities discovered and described?
- How does a workflow reference capabilities without binding to one agent implementation?
- Can existing systems such as Superpowers be consumed directly, adapted, or treated as a reference implementation?
- What is the right boundary between “workflow” and “orchestration”?

## Execution model
- What is the durable unit: WorkItem, Run, Execution, Task, Step, Attempt, Session?
- How do retries, parallelism, scatter/gather, voting and cancellation work?
- What state must survive process crashes/restarts?
- How are confidence and exceptions represented?

## Protocol boundaries
- What minimum ACP capability profile must a harness satisfy for autonomous factory work?
- How should native ACP implementations, ACP adapters and provider-native integrations advertise their different fidelity and support risk?
- How should AHP session/chat identifiers correlate with factory work items, executions, attempts and workspaces without becoming factory identifiers?
- Which operator-facing concerns fit standard AHP, and which require a separate factory-native query/command/subscription contract?
- Can AHP and factory-native contracts share authentication, transport and connection management without sharing domain schemas?
- What protocol conformance and adapter test suites do we need before interfaces are frozen?
- How do clients and durable factory execution recover differently after a disconnect, harness crash or host crash?
- What internal contract connects the protocol-independent engine to the logically separate Factory AHP Gateway?
- When the factory acts as an AHP client of an external host such as VS Code, which sessions should be imported, mirrored or linked?
- How do external-host ownership, authorization, identity and duplicate-session handling work?

See [`../research/ahp-acp-architecture-direction.md`](../research/ahp-acp-architecture-direction.md) for the approved research direction and evidence behind these questions.

## Sandboxes, execution platforms and compute
- How does an organisation or a team adopt the factory from nothing, and does that differ between the factory as its own repository and the factory vendored into one repository?
- The sandbox is a provider (decision 0002) and a container is the default. What interface is stable across a container, a virtual machine and the sandbox types that follow?
- The execution platform is a separate provider. What interface is stable across a local machine, a server, a cloud and a rented virtual machine?
- How do Windows/macOS/Linux and x64/ARM64 capabilities enter scheduling?
- Where does remote caching belong: factory concern, build-system concern, or provider capability?

## Verification
- How are repo-defined gates discovered?
- How do we deduplicate expensive test/gate executions across agents and workflow steps?
- How do deterministic gates and LLM-based review signals compose?
- How is gate calibration measured over time?
- How does the factory detect that a full set of green deterministic gates covered less than the requirement? In August 2026 four agent-built consoles passed every gate while showing test controls and design doctrine as product copy. See [`../research/ux/agent-built-ui-lessons.md`](../research/ux/agent-built-ui-lessons.md).
- Which semantic assertions about state belong beside mechanical DOM and lint checks, and who writes them?
- Whatever a test requires in the product, the agent builds into the product. How do test-only controls stay out of product surfaces?

## Domain vocabulary
- The UX benchmark left a draft entity, state, action and event vocabulary in [`draft-factory-vocabulary.md`](draft-factory-vocabulary.md). Which parts survive the smallest working engine, the smallest version of the engine that does one useful job from end to end?
- Is the split between an immutable catalog and cumulative state natural when state comes from real providers?
- Is attention an Engine entity or a console projection?

## Observability and meta-loop
- Where does the evidence ledger live: forge-native first, or an external store such as DuckDB or OpenTelemetry, and when is the crossover worth it?
- What events/traces are first-class from day one?
- OpenTelemetry as the canonical telemetry substrate?
- How are gate false positives, escapes, agent performance and cost measured?
- Which meta-loop recommendations can be generated automatically?

## External systems
- Backlog: which tracker does an adopter bring? Candidates are GitHub issues and projects, Linear and Jira, both hosted issue trackers, a Markdown file, or something of their own.
- SCM/forge: GitHub first but provider-neutral?
- Agent surfaces: Codex, Pi, OpenCode, Claude Code and others.
- Coding-agent harness control: ACP first where supported, with provider-native adapters where necessary.
- Host/operator session synchronization: AHP as a first-class candidate, without making it the factory domain model.
- Agent access to tools and context: MCP and native harness mechanisms.
- Agent-to-agent interoperability, if required: A2A or a separate factory-mediated contract; not AHP/ACP by default.
- Dynamic/operator UI: AG-UI, A2UI, MCP Apps or a factory-native typed presentation protocol; not conflated with AHP session synchronization.
- On-demand analytics surface: do CopilotKit, AG-UI or the Vercel AI SDK fit the natural-language query layer over the evidence ledger?
