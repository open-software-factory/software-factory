# AHP and ACP architecture direction

Status: Approved research direction, not a protocol lock-in or detailed architecture

Reviewed: 15 August 2026

Decision: Keep the Factory Engine domain independent while treating ACP and AHP as first-class protocol implementations at distinct edges. The Factory UI is an AHP client; a separate Factory AHP Gateway is the AHP server toward factory-owned surfaces.

## Executive assessment

The recommended initial topology is:

```text
Factory UI: desktop | web/mobile | IDE | CLI/TUI
        ├── AHP client ───────────────┐
        └── Factory API client ───────┤
                                      ▼
                            Factory Surface Gateway
                            ├── AHP server
                            └── Factory API server
                                      │
                                      ▼
                              Factory Engine
                     orchestration | workflow | dependencies
                     policy | verification | persistence
                     evidence | traceability | product lifecycle
                                      │
                                      ▼
                          Harness integration edge
                     ACP clients | provider-native adapters
                                      │
                                      ▼
                         Individual coding-agent harnesses
                Codex | Claude | Gemini | Kimi | OpenCode | others
```

This topology relies on seven boundaries:

1. The Factory UI is an AHP client. It consumes shared agent-session state from a host and sends session-level actions back to it.
2. The Factory AHP Gateway is the AHP server or host toward factory-owned operator surfaces. It owns the authoritative AHP projection and synchronization behavior, while the Factory Engine owns the factory domain.
3. The Factory Engine has no intrinsic AHP client or server role. It stays protocol-independent and communicates with edge components through internal application contracts.
4. ACP is the preferred boundary to an individual coding-agent harness because it already has meaningful native and adapter-based adoption.
5. AHP covers agent sessions, chats, terminals, changesets and related presentation state. The factory still needs a client API for work-item graphs, software-product lifecycles, orchestration, quality gates and traceability.
6. The Factory Engine remains authoritative for factory semantics. Factory-native query, command and subscription contracts sit alongside AHP at the Surface Gateway without representing the factory domain as invented AHP actions.
7. Protocol adapters translate at the edges. The architecture should preserve the option to replace, supplement or remove either protocol cheaply.

This is an architectural direction to validate during architecture work. AHP in particular remains too young to make a settled dependency.

## AHP roles in the factory

The word `host` describes an AHP protocol role, not the whole Software Factory product.

In VS Code's architecture, the Agents/editor UI is an AHP client and the separate Agent Host process is the AHP server. VS Code ships both roles as parts of one product. We should preserve the same logical distinction even if early factory deployments package several components together.

### Initial role assignment

| Component | AHP role | Other role |
|---|---|---|
| Factory desktop/web/mobile/IDE UI | Client | Also consumes factory-native read models and commands |
| Factory CLI/TUI | Client where it renders or controls agent sessions | Also a factory-native operational client |
| Factory Surface Gateway | Server/host | Also serves factory-native queries, subscriptions and commands |
| Factory Engine | None intrinsically | Owns orchestration, policy, persistence and lifecycle semantics |
| Harness integration edge | Normally none | ACP client or provider-native adapter toward coding harnesses |
| Coding-agent harness | Normally ACP agent/server | Owns its agent loop and session behavior |

This is a logical separation. Whether the Surface Gateway initially runs in the same binary, as a supervised sidecar, or as a separately deployed service remains an implementation choice. AHP schemas and state machinery must still stay outside core Factory Engine packages.

### Future external-host connection

The factory may later need to observe or operate sessions hosted elsewhere, such as sessions already managed by VS Code's Agent Host. In that case, a separate connector takes the AHP client role:

```text
External VS Code Agent Host
         AHP server
              │
              ▼
Factory External-Host Connector
         AHP client
              │
      translates/correlates
              │
              ▼
        Factory Engine
              │
              ▼
   Factory Surface Gateway
         AHP server
              │
              ▼
         Factory UI
         AHP client
```

The factory system can eventually participate in both AHP directions without making the engine itself protocol-dependent:

- server toward its own operator surfaces, through the Factory AHP Gateway;
- client toward external AHP hosts, through an external-host connector.

The connector should import and correlate external session observations rather than blindly proxying protocol streams. Factory policies, identities, work-item relationships and audit history must remain explicit.

### Topology options

#### Option 1: UI connects directly to an external AHP host

```text
Factory UI ◄──AHP──► VS Code Agent Host
```

This is simple for viewing an isolated external session, but it bypasses factory correlation, policy, persistence and the unified operator model. It may be useful as a diagnostic or deep-link capability, not as the primary factory topology.

#### Option 2: factory-owned gateway (recommended initial topology)

```text
Factory UI ◄──AHP──► Factory AHP Gateway ──internal contracts──► Factory Engine
                                                        └────► harness integrations
```

This gives factory-owned surfaces one authoritative session projection while keeping orchestration and lifecycle semantics in the engine. It is the smallest topology that exercises AHP's value without coupling the core to it.

#### Option 3: federated external hosts (future extension)

```text
External AHP hosts ◄──AHP──► Factory connectors ──► Factory Engine
                                                     │
Factory UI ◄──AHP──► Factory AHP Gateway ◄───────────┘
```

This allows the factory to aggregate sessions from VS Code and other hosts. It adds identity mapping, ownership, duplicate-session handling, capability differences and cross-host recovery, so it should be validated after the factory-owned gateway works.

## Fit with the existing factory direction

The canonical documents make the factory broader than an agent-session host:

- the product lifecycle begins with evidence, problems, hypotheses, experiments and intent;
- the engineering lifecycle connects specifications to design, implementation, deterministic verification, PRs, deployment and production evidence;
- the operating model coordinates parallel work, exception handling, architectural fitness and a meta-loop that improves the factory itself;
- traceability connects all three.

The current UX direction expresses that domain through several projections over shared truth: the Factory Floor, work graphs, Flight Recorder, attention, runway and factory intelligence. AHP describes only part of the live operational state needed by those projections.

The repository currently contains vision, research and bootstrap planning rather than an implemented engine or client. The decided core language remains Go, with integrations expected to be language-neutral and preferably out of process. Protocol seams should be validated before production packages make them expensive to change.

## Evidence labels used here

- `Established fact` means the claim is stated by a protocol specification, an upstream project or first-party product documentation.
- `Ecosystem evidence` means an implementation or adoption signal exists today, with its implementation form identified.
- `Factory conclusion` means our architectural interpretation of those facts and signals.

This distinction is important because an upstream registry entry proves interoperability, but not necessarily native support by the underlying agent vendor.

## What ACP is

The [Agent Client Protocol](https://agentclientprotocol.com/get-started/introduction) standardizes communication between an agent client, originally an editor or IDE, and a coding-agent process.

### Established responsibilities

ACP defines a point-to-point client/agent relationship covering concerns such as:

- initialization and capability negotiation;
- authentication;
- creating, loading and controlling agent sessions;
- sending prompts and streaming updates;
- content blocks, plans and diffs;
- tool-call progress and permission requests;
- terminal and filesystem operations exposed by the client;
- cancellation;
- MCP server configuration.

Its normal local deployment starts the agent as a subprocess and communicates using JSON-RPC over standard input/output. A single connection can carry multiple agent sessions, but it still represents one client talking to one agent endpoint. The [ACP architecture documentation](https://agentclientprotocol.com/get-started/architecture) explicitly describes this bidirectional relationship.

ACP is suitable in principle for local and remote use. However, its own [introduction](https://agentclientprotocol.com/get-started/introduction) says that full remote-agent support remains work in progress. Stdio is therefore the proven common denominator today; remote transports must be evaluated implementation by implementation.

The current stable wire protocol is v1, while v2 is draft. The protocol negotiates versions and optional capabilities independently of SDK package versions, as described by the [ACP repository](https://github.com/agentclientprotocol/agent-client-protocol).

### What ACP does not own

ACP does not define:

- how a coding agent reasons or implements its loop;
- a portfolio of work items or their dependencies;
- allocation of work across multiple agents;
- agent-to-agent coordination;
- factory workflow or retry semantics;
- workspace and runner scheduling;
- deterministic quality gates;
- durable software-delivery history;
- product-lifecycle or traceability semantics.

An ACP session represents an agent interaction. A factory work item, attempt, workflow or execution remains a separate concept.

## What AHP is

Microsoft's [Agent Host Protocol](https://microsoft.github.io/agent-host-protocol/guide/what-is-ahp.html) defines how a standalone agent-session host communicates with multiple clients.

### Established responsibilities

AHP is a host-authoritative state-synchronization protocol. Its core concepts include:

- URI-addressed subscribable channels;
- immutable state snapshots;
- ordered action envelopes;
- pure reducers shared by clients and hosts;
- optimistic client updates with write-ahead reconciliation;
- multi-client synchronization;
- reconnection using replayed actions or replacement snapshots;
- session and chat catalogues;
- terminals, changesets, annotations and resource access;
- capability and version negotiation;
- optional OpenTelemetry logs, traces and metrics carried as OTLP/JSON.

The [AHP and ACP layering guide](https://microsoft.github.io/agent-host-protocol/guide/ahp-and-acp.html) describes AHP as the coordination layer above an agent host and ACP as a possible 1:1 communication layer below it. The host translates agent-specific events into agent-agnostic client-facing state.

AHP is transport-agnostic. Microsoft's current reference usage employs local IPC and JSON-RPC over WebSocket for remote hosts. AHP's synchronization model supports clients disconnecting and later recovering missed state. It also supports an agent host continuing while no operator client is connected.

### Limits

The [AHP doctrine](https://microsoft.github.io/agent-host-protocol/guide/doctrine.html) explicitly excludes:

- agent implementation and reasoning;
- a required agent loop or model router;
- a universal backend tool schema;
- a required storage backend;
- a required UI framework or layout;
- agent-to-agent coordination;
- replacement of ACP or other downstream agent protocols.

AHP replay is also not equivalent to durable workflow recovery. Its [connection lifecycle](https://microsoft.github.io/agent-host-protocol/specification/lifecycle.html) says that, after unexpected server termination, in-progress turns should be considered failed even if the environment restarts the host. The factory therefore still needs its own persistence, process supervision, recovery and retry policies.

AHP's standard state is a client-facing presentation model for agent experiences. It should not become the source schema for the Software Factory domain.

### Maturity

AHP is currently a working draft under active development. The [specification overview](https://microsoft.github.io/agent-host-protocol/specification/overview.html) warns that breaking changes to wire types, actions and state shapes are expected.

The protocol has concrete engineering behind it:

- a VS Code reference host and built-in client;
- published client libraries for TypeScript, Rust, Go, Kotlin and Swift;
- protocol version negotiation and capabilities;
- a CLI/Node client named AHPX;
- formal channel and state specifications.

The available engineering evidence is concentrated rather than broadly independent. As of this review, the [implementation list](https://microsoft.github.io/agent-host-protocol/guide/implementations.html) identifies VS Code as the reference server. SDK availability must not be counted as multiple independent host implementations.

### Implications for the current Go direction

Microsoft publishes an official AHP Go client library, but its implementation list does not identify a reusable Go host/server implementation. A Go-based Factory AHP Gateway may therefore need to implement the server side from the specification/generated wire types, contribute reusable host support upstream, or supervise a temporary sidecar.

ACP's protocol organization currently lists official SDKs for Kotlin, Java, Python, Rust and TypeScript, not Go. The community-maintained [`coder/acp-go-sdk`](https://github.com/coder/acp-go-sdk) provides typed Go client and agent support and is a credible implementation candidate, but it should not be represented as an official ACP SDK.

These library gaps do not justify changing the core-language decision. They reinforce the existing posture: keep protocol types isolated, prefer process boundaries, validate libraries with conformance fixtures, and avoid allowing SDK convenience to determine the factory domain.

## How AHP and ACP compose

The protocols solve different relationships:

| Relationship | Suitable protocol | Authority |
|---|---|---|
| One client-side host adapter controlling one coding-agent endpoint | ACP | Agent owns its session behavior; client drives it through negotiated capabilities |
| Several clients observing and controlling shared agent sessions | AHP | Host sequences actions and owns synchronized presentation state |
| Work items, dependencies, policies, gates and delivery lifecycle | Factory domain contracts | Factory Engine |
| Agent access to tools and context | MCP and native harness mechanisms | Tool/provider boundary |
| Agent-to-agent communication, if ever required | Separate concern; potentially A2A or a factory-mediated contract | Factory orchestration policy |
| Generated or server-driven interactive UI | Separate dynamic-UI concern | Surface gateway and client safety model |

The AHP host may use ACP, an adapter around a proprietary SDK, or another agent-specific interface. AHP does not require ACP below it. For the factory, ACP should be the preferred path rather than the only path.

## Current adoption evidence

### Classification

- `Native`: the upstream coding-agent project implements an ACP mode in its own distribution.
- `First-party ecosystem adapter`: an adapter is maintained by the ACP organization or a closely involved client project, while the underlying agent does not itself speak ACP.
- `Third-party wrapper`: an independent project translates between ACP and a harness's CLI, SDK or server API.
- `Client adoption`: an operator surface speaks ACP; this says nothing about whether a particular agent is native.

These categories should be recorded in provider metadata because they affect fidelity, support expectations and upgrade risk.

### Coding-agent support

| Harness/ecosystem | Current form | Evidence | Factory implication |
|---|---|---|---|
| Gemini CLI | Native | Gemini CLI ships [`gemini --acp`](https://github.com/google-gemini/gemini-cli/blob/main/docs/cli/acp-mode.md) and contains its ACP implementation in the upstream repository. | Strong early interoperability target. |
| Kimi CLI | Native | Kimi ships [`kimi acp`](https://github.com/MoonshotAI/kimi-cli) as an out-of-the-box ACP agent server. | Strong target, particularly given interest in Kimi's harness. |
| OpenCode | Native | OpenCode documents [`opencode acp`](https://dev.opencode.ai/docs/acp/) as a built-in subprocess mode. | Strong open-source target. |
| GitHub Copilot CLI | Native, public preview | GitHub documents [`copilot --acp`](https://docs.github.com/en/copilot/reference/copilot-cli-reference/acp-server), including stdio and TCP modes. | Valuable subscription-reuse target, but preview status requires capability testing. |
| Codex CLI | Adapter | [`codex-acp`](https://github.com/agentclientprotocol/codex-acp) starts Codex App Server and maps between Codex operations/events and ACP. Codex CLI does not implement ACP natively. | Reuse the adapter first; preserve Codex metadata and keep a direct-provider escape hatch. |
| Claude Agent/Claude Code | Adapter | [`claude-agent-acp`](https://github.com/agentclientprotocol/claude-agent-acp) exposes the Claude Agent SDK through ACP. Anthropic's harness itself is not thereby a native ACP implementation. | Treat adapter behavior and subscription/auth compatibility as independently testable. |
| Pi | Adapter | The ACP ecosystem lists Pi through a separate `pi-acp` adapter on its [agent integrations page](https://agentclientprotocol.com/get-started/agents). | Useful open harness, but adapter maintenance is part of provider risk. |

The [ACP registry](https://agentclientprotocol.com/get-started/registry) contains many more agents. Registry presence proves that an installable endpoint passes registry requirements; it does not by itself establish native upstream support, feature completeness or operational quality.

### Client adoption

- Zed created ACP with Gemini CLI as its initial reference integration and supports external ACP agents natively. Its own account of the collaboration is in [Bring Your Own Agent to Zed](https://zed.dev/blog/bring-your-own-agent-to-zed).
- JetBrains IDEs include a built-in ACP client and can launch configured ACP agents over stdio, as shown in [JetBrains' ACP integration documentation](https://blog.jetbrains.com/ai/2026/02/koog-x-acp-connect-an-agent-to-your-ide-and-more/).
- The ACP ecosystem now includes editor, CLI/TUI, desktop, web, mobile and messaging clients. This breadth is a useful adoption signal, but the individual projects vary widely in maturity.

### AHP adoption

- VS Code is both the reference AHP server and a built-in client.
- VS Code 1.130 describes the Agent Host and its Copilot, Claude and Codex harness adapters as a progressive rollout. See the [July 2026 release notes](https://code.visualstudio.com/updates/v1_130).
- VS Code documents remote hosts, browser/mobile access, session continuity across disconnected clients and WebSocket transport in its [Agent Host architecture](https://code.visualstudio.com/docs/agents/concepts/agent-host).
- Microsoft publishes AHP client libraries in five languages, including Go, through the [AHP repository](https://github.com/microsoft/agent-host-protocol).

Adoption is asymmetric: ACP has a multi-vendor agent and client ecosystem, while AHP has a substantial reference implementation and broad SDK coverage but remains predominantly Microsoft/VS Code-led.

## Why this matters to the planned factory experience

### ReactFlow-based live Factory Floor

AHP can supply high-fidelity live state for agent sessions: turns, tool calls, permission requests, terminals, changesets and connection status. ACP can supply the underlying harness events where agents support it.

Its nodes and edges also represent work items, dependencies, repositories, workspaces, gates, deployments and lifecycle state. Those must be projected from Factory Engine entities and events.

Factory conclusion: AHP/ACP events are inputs into the Factory Floor projection. The graph must not render directly from protocol objects or equate an AHP session with a unit of work.

### Temporal-style work-item inspection and Flight Recorder

AHP's snapshots, ordered actions and paged turn history are useful evidence. Its monotonic `serverSeq` orders mutations within a host stream, not across the whole factory. Its stateless telemetry channels are not replayed.

The Flight Recorder needs a durable, cross-system history that can correlate:

- original intent and work-item state;
- scheduling and dependency decisions;
- harness sessions and turns;
- tool calls and permission decisions;
- workspace and source-control changes;
- verification results;
- PR, deployment and production evidence;
- retries, recovery and human interventions.

Factory conclusion: preserve raw protocol evidence and normalize important semantic events into the factory history. Do not use AHP's replay buffer as the audit log or event store.

### Ambient operator assistant

AHP fits several assistant interactions well: attach to a live session, stream a response, request input, approve a tool call, cancel a turn, annotate state, or move between clients.

The assistant will also answer and act on factory-level questions such as “why is this feature blocked?”, “what needs my attention?” or “reassign this work remotely.” These require the Factory Engine's dependency graph, policy, evidence and authorization model.

Factory conclusion: the ambient assistant should call factory application capabilities for factory actions and use AHP for shared agent-session interaction. It must not smuggle factory commands through chat prompts when a typed factory operation exists.

AHP is also not a dynamic-UI language. AG-UI, A2UI, MCP Apps or a factory-specific typed presentation contract remain separate candidates for generated investigation panels and interactive visual explanations.

### CLI/TUI, desktop, web/mobile and IDE surfaces

AHP directly addresses a difficult shared-surface problem: different clients can subscribe to the same host-authoritative session state, make optimistic changes and reconcile to a common order after reconnecting.

This is highly aligned with:

- a Tauri desktop client and a later browser client;
- CLI/TUI inspection and control;
- IDE integration;
- remote mobile check-ins;
- concurrent clients viewing the same live agent session;
- transferring attention between devices without transferring ownership of the underlying process.

However, all these surfaces also need factory-native read models and commands. A thin client may therefore consume both standard AHP channels and a factory surface API through one logical gateway.

### Long-running and disconnected execution

AHP gives us a strong client-disconnection model: the host can stay near the workspace while clients come and go, and clients can catch up through replay or snapshots.

It does not solve:

- scheduling and supervising the work itself;
- recovering an interrupted workflow after an engine or host crash;
- retry policy and idempotency;
- durable timers and delayed work;
- runner or sandbox replacement;
- lease ownership and duplicate execution;
- cross-host failover.

Factory conclusion: use AHP for disconnected operation of agent sessions, while the Factory Engine owns durable orchestration and recovery. “The UI disconnected” and “the work stopped” must be separate states.

### Interchangeable coding-agent harnesses

ACP materially reduces integration cost, especially for native implementations. It does not erase capability differences. Agents vary in session loading, model selection, tool execution location, terminal behavior, plans, authentication, permissions, subagents and extension metadata.

Factory conclusion: do not design one giant lowest-common-denominator `Agent` interface. Use capability negotiation and narrow operations. Preserve provider-specific metadata for observability and advanced UX, while keeping required factory behavior independent of any one extension.

## Recommended Factory Engine boundary

### Factory Engine responsibilities

The Factory Engine owns:

- software products, repositories and workspaces;
- work items and dependency graphs;
- readiness, prioritization and executable-frontier calculations;
- orchestration, scheduling, retries, cancellation and recovery;
- workflow and reusable-capability composition;
- sandbox and compute allocation policy;
- deterministic verification and quality-gate authority;
- evidence and artifact relationships;
- intent-to-production traceability;
- persistent factory history and projections;
- trust calibration, attention and exception policy;
- product/engineering lifecycle semantics;
- authorization and auditing of factory-level actions.

### ACP gateway responsibilities

The ACP edge should be responsible for:

- discovering and launching an ACP endpoint;
- negotiating protocol and capabilities;
- creating, loading, prompting and cancelling harness sessions;
- relaying permission and elicitation requests through factory policy;
- receiving high-fidelity session updates;
- translating protocol events into internal observations and evidence;
- retaining source metadata for diagnostics;
- isolating adapter crashes and version incompatibilities.

For a harness without adequate ACP support, a separate provider adapter may implement the same required factory behaviors through its native CLI, SDK or server API. We should not block useful harnesses merely to claim protocol purity.

### Factory AHP Gateway responsibilities

The AHP server edge toward factory-owned UIs should be responsible for:

- publishing available agent backends and shared sessions;
- synchronizing display-ready agent-session state across clients;
- sequencing client mutations within those channels;
- subscriptions, snapshots, replay and reconciliation;
- session-level prompts, approvals, cancellation and annotations;
- terminal and changeset presentation where supported;
- capability/version negotiation and graceful degradation;
- mapping internal session observations into AHP state without leaking provider-specific requirements into clients.

The AHP edge should not become the owner of work items, workflow state or delivery policy. Factory-specific views and actions should use explicit factory contracts rather than pretending to be standard AHP chat/session actions.

An external-host connector has the inverse protocol role: it is an AHP client that subscribes to an external host and translates selected session state/actions into internal observations. Keep it separate from the factory-owned AHP server even if they share protocol libraries.

## Proposed internal seams, not frozen interfaces

Architecture work should validate behavioral seams before naming permanent Go interfaces.

### Harness-control seam

The engine needs a way to ask a harness endpoint what it can do and perform a small set of lifecycle operations. ACP should be one implementation. Provider-native adapters may be others.

Do not freeze the interface until capability matrices from several real harnesses expose the genuinely common operations.

### Harness-observation seam

Raw ACP/provider events should be captured separately from factory semantic events. Translation should be explicit and testable. Unknown provider metadata should survive round trips where practical.

This seam prevents every protocol content block or tool-call state from becoming a core domain type.

### Surface-query and command seam

Operator clients need stable factory read models, subscriptions and authorized commands. These are independent of React, Tauri and any one wire protocol.

AHP can implement the synchronized agent-session portion. A separate factory protocol can expose work graphs, attention, Flight Recorder, runway, traceability and lifecycle operations. The two can share connection management and correlation identifiers without sharing schemas.

### Identity and correlation seam

The factory must correlate without conflating:

- work item;
- orchestration execution or attempt;
- workspace/sandbox allocation;
- harness process;
- ACP session;
- AHP session and chat;
- client connection;
- source-control change/PR;
- verification and deployment artifacts.

The relationships will often be one-to-many or many-to-one. No protocol identifier should be promoted to the universal factory identifier.

### Persistence seam

Factory state, audit history and evidence must survive engine restarts independently of AHP replay buffers and harness session persistence. Protocol snapshots are projections or integration state, not the factory database schema.

## Risks

### AHP instability

Breaking schema changes are explicitly expected. The canonical types are currently developed in TypeScript and client libraries are generated for other languages. Although a Go client exists, the listed reference server is VS Code rather than a reusable Go host implementation.

Mitigation: pin protocol versions in experiments, isolate generated/wire types, use contract fixtures, and do not expose AHP types from domain packages.

### Mistaking presentation state for domain state

AHP has attractive types for sessions, changesets, annotations, terminals and telemetry. Reusing them internally would be expedient but would make future factory behavior conform to an agent UI protocol.

Mitigation: model only factory concepts proven necessary; map into AHP projections at the edge.

### Adapter fidelity and drift

Codex, Claude and Pi adapters may lag their underlying harnesses or omit provider features. Native ACP modes can also expose only part of their interactive product.

Mitigation: record implementation form and version, run capability/conformance probes, retain raw metadata, and permit provider-native escape hatches.

### Lowest-common-denominator UX

Interchangeability can flatten distinctive harness capabilities and hide useful operational information.

Mitigation: establish a small required baseline plus negotiated optional capabilities. Surface differences plainly in operator UX.

### Security and authority split

ACP and AHP both carry permissions and authentication concepts, but the factory also has policies over workspaces, repositories, deployments and irreversible actions.

Mitigation: the Factory Engine remains the policy authority for factory-controlled resources. Protocol permission prompts are inputs to policy, not proof of authorization by themselves.

### Duplicate and conflicting event streams

ACP updates, AHP actions, OpenTelemetry and provider logs may describe the same operation at different levels.

Mitigation: assign provenance, correlation and semantic-event rules. Preserve raw evidence without counting every transport representation as a separate factory event.

### Overextending AHP

AHP's channel model makes new channel types tempting. Encoding work graphs, verification policy or product lifecycle into proprietary AHP extensions would create the coupling this direction is meant to avoid.

Mitigation: require evidence that a concept is genuinely client-session synchronization before adding an AHP extension. Otherwise expose it through the factory surface contract.

## Hypotheses to validate before architecture is finalized

1. Test whether ACP covers the required harness baseline using native Gemini, Kimi and OpenCode endpoints plus the Codex and Claude adapters against the same scenarios.
2. Test whether capability negotiation is sufficient by building a factual matrix for session resume, plans, permissions, terminals, MCP, model selection, subagents, images, usage and extension metadata.
3. Test adapter fidelity by comparing at least one adapter-driven harness with its native TUI or app-server behavior.
4. Exercise two concurrent clients, optimistic actions, disconnect/reconnect, approvals, cancellation and session history to test whether AHP can support shared-session UX without owning factory semantics.
5. Determine whether AHP's Go artifacts support the chosen role: implement a host directly from schemas, reuse generated types, contribute server support, or isolate a temporary sidecar.
6. Demonstrate a work item linked to multiple harness sessions and attempts to test whether factory and protocol histories can be correlated without treating a session as the work item.
7. Kill the surface host or harness during work and verify that durable factory state can classify, retry or escalate the failure.
8. Validate client authentication, host authentication, workspace authority, permission arbitration and audit identity across reconnects.
9. Prototype an assistant-generated investigation whose data comes from factory projections while live agent-session interaction comes through AHP.
10. Verify that a fake harness and fake surface transport can be used in tests without importing AHP/ACP schemas into core packages.

## Questions for future architecture work

- Is the logically separate Factory AHP Gateway packaged in the same Go process as the engine initially, run as a supervised sidecar, or deployed as a separate service?
- What is the minimum internal contract between the protocol-independent engine and the Factory AHP Gateway?
- When connecting to an external AHP host, which sessions are imported, mirrored or merely linked?
- How are ownership, authorization and audit identity preserved when the factory is an AHP client of another host?
- Which factory-native transport serves work graphs, attention, Flight Recorder, traceability and lifecycle commands?
- Can AHP and factory contracts share one authenticated connection without coupling their schemas?
- What is the minimal required ACP capability profile for an autonomous coding harness?
- Which missing ACP capabilities justify provider-native adapters rather than factory-specific ACP extensions?
- Where does harness-process supervision live relative to durable orchestration?
- How are protocol events, OpenTelemetry signals and factory semantic events correlated and deduplicated?
- Which actions can any connected client perform, and which require an operator role, lease or explicit approval?
- How are sessions discovered across multiple local and remote hosts?
- How do we version mappings when AHP, ACP and individual harnesses evolve independently?
- Should the factory publish protocol conformance profiles and reusable adapter test suites?
