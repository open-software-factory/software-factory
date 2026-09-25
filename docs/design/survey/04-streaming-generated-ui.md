# Survey 04: Standards and frameworks for agent-generated and streaming UI

This survey tracks the StreamSurface primitive, which is Layer 5 in `components.md`.

This survey checks each candidate against four contract points. An agent must compose a view
from our Level-2 vocabulary only. Parts must stream in without re-layout. The target slot must
set each part's density mode, using the Component modes system in `components.md`. A generated
view must be savable as a preset, per `DESIGN.md` section 5.6.

`DESIGN.md` sections 6.4 and 7.2 also apply to the preset contract.

This survey was done in September 2026.

## Comparison table

| Name | What it actually is | Component model | Streaming granularity | Interop | Maturity (Sep 2026) |
|---|---|---|---|---|---|
| **AG-UI** (Agent-User Interaction Protocol) | Transport and event protocol between agent runtime and frontend | No fixed catalog. Carries whatever UI events the app defines | Event stream: state deltas, tool events, generative-UI events, interrupts | Protocol-level, transport-agnostic across HTTP, SSE, and WebSocket. Frontend-agnostic. About 13 agent-framework integrations, including LangGraph, CrewAI, Google ADK, AWS Strands, Claude Agent SDK, Mastra, and Agno | Production-real. MIT license, 12k+ GitHub stars, broad ecosystem adoption |
| **A2UI** (Agent-to-User Interface) | Declarative UI schema an agent emits | **Fixed catalog**: client declares trusted components, such as Card, Button, and TextField. Agent may only reference the catalog | Flat component list with ID references, plus separate structure and data messages, causing progressive fill-in | Protocol-level schema, transport-agnostic, riding AG-UI, A2A, or plain HTTP. Renderers exist for Flutter, Angular, Lit, React, and web components | Real but young. Built by Google, CopilotKit, and the open-source community. Apache-2.0 license, released Dec 2025. v0.9.1 is current, v1.0 is a release candidate |
| **Open-JSON-UI** | OpenAI's internal declarative Generative UI schema, open-sourced | Fixed set (cards/lists/forms) rendered by frontend's own components | Declarative spec streamed similarly to A2UI | Protocol-level schema; younger, smaller ecosystem than A2UI, closely tied to OpenAI's Apps SDK lineage | Early. Newly opened, adoption still forming |
| **MCP Apps** (MCP-UI merged in) | MCP extension: server declares a `ui://` HTML resource per tool | Arbitrary sandboxed HTML/JS per app. No shared component vocabulary | Whole-app iframe preloads, then gets pushed tool results. Re-layout inside the iframe is the app author's problem | Protocol-level, using JSON-RPC over postMessage. This is an official MCP extension, but each app ships its own UI stack, opaque to the host's design system | Production-real. Official MCP extension since Jan 2026. Shipped in Claude, Claude Desktop, VS Code Copilot, Goose, and Postman |
| **Vercel AI SDK**, covering UI hooks, RSC genUI, and AI Elements | TypeScript SDK: `useChat`/`useCompletion` hooks, RSC component streaming, prebuilt AI Elements | Whatever React components the app author streams. No cross-app catalog | React Server Components stream in via Suspense boundaries. Layout stability depends on the app, with no guarantee | **Framework-locked to React and Next.js.** Core streaming hooks are mature. AI SDK RSC, the genUI-via-RSC piece, is explicitly paused by Vercel | Hooks/Core: production-real, with 11.5M weekly downloads. RSC genUI path: stalled, being superseded by AI Elements |
| **json-render** (`@json-render/*`) | OSS library. JSON spec becomes a component tree via a registered catalog | **Fixed catalog** via `defineCatalog()`, Zod-validated props | JSON Patch, per RFC 6902, sends incremental patches to the mounted tree | Framework-agnostic core, `@json-render/core`. React, Vue, and Svelte renderers ship today. Not a network protocol, so you supply the transport | Production-real as a library, but niche, with 44 downstream npm packages. It has not become an industry standard |
| **Thesys C1 / Crayon** | Commercial generative-UI API, plus a React design system | Fixed Crayon component set: charts, tables, forms, and cards | Streams "live UI blocks". Specifics of patch mechanics not published | **Framework-locked to React**, via the Crayon SDK client. The C1 API itself is model-agnostic on the backend | Production-real commercially, with 300+ teams, but proprietary and React-only |
| **CopilotKit** | React framework and toolkit wrapping AG-UI, plus pluggable genUI specs | Delegates to whichever spec it hosts: AG-UI controlled components, A2UI, Open-JSON-UI, or MCP Apps | Delegates to the hosted spec | **Framework-locked to React** at the app layer. The AG-UI layer underneath is not locked | Production-real as a React toolkit. It is useful as a reference implementation rather than a standard |
| **W3C Generative UI Community Group** | Nascent standards body, launched 2026 | Undecided. Exploring intermediate representations | Exploratory, no shipped spec | N/A yet | **Spec-ware.** Community-group stage, with exploratory notes on evaluation, testing, and cross-vendor IR. No deliverable yet |

## Answers

**1. Best fit for "agent picks from a fixed vocabulary, streams a declarative layout":**

A2UI is the closest match. It is built on a client-declared, trusted component catalog. This
pattern is called "controlled generative UI." The agent can only reference components the host
already trusts. A2UI also uses a flat, streaming-friendly JSON structure.

Open-JSON-UI and json-render follow the same pattern of a catalog plus a declarative spec. A2UI
has the widest cross-platform renderer support today.

AG-UI is not a competitor here. It is the pipe that carries A2UI's messages. It is not a
vocabulary format on its own.

**2. Progressive and streaming, and how re-layout is avoided:**
- A2UI separates *structure* messages from *data* messages and gives every component a
  stable ID. New parts attach to a placeholder slot by ID instead of re-parsing the whole
  tree, so already-placed components don't move when later parts arrive.
- json-render streams RFC 6902 JSON Patch operations against the mounted tree. Patches touch
  only the changed node. The whole document does not resend.
- MCP Apps does not stream at component granularity. The iframe is preloaded once, then it
  receives pushed data. What happens inside, including any re-layout, is invisible to the
  host. It is the app author's own problem to solve.
- Vercel AI SDK RSC streams whole React components in via Suspense boundaries. Nothing in the
  SDK guarantees layout stability. Stability depends on how the app author places boundaries.
  The RSC genUI path is now paused by Vercel.

**3. Interop, protocol-level vs framework-locked:**

Protocol-level and transport-agnostic:
- **AG-UI**: an event transport, usable with any frontend.
- **A2UI**: a schema that rides any transport, with renderers for Flutter, Angular, Lit,
  React, and web components.
- **Open-JSON-UI**: a schema.
- **MCP Apps**: JSON-RPC over postMessage, an official MCP extension.

Framework-locked:
- **Vercel AI SDK RSC genUI** and **Thesys C1/Crayon** are both React-only on the client.
- **CopilotKit** is a React toolkit, though the AG-UI layer under it is not locked.

**json-render** sits in between. Its core is framework-agnostic. It is a rendering library you
embed rather than a network protocol. Pair it with AG-UI, or your own transport, rather than
using it standalone end to end.

**4. Maturity, honestly:**

Production-real today:
- **AG-UI** has broad multi-framework adoption, an MIT license, and a mature ecosystem.
- **MCP Apps** is an official MCP extension, shipped in five or more major hosts.
- **Vercel AI SDK core hooks** have a huge install base, but the RSC-genUI path specifically
  is stalled.
- **Thesys C1** has real commercial usage, though it is proprietary.

Real but still young: **A2UI** is nine months old at survey time and pre-1.0. It is backed by
Google and CopilotKit, with working multi-platform renderers already shipping.

Early or niche: **Open-JSON-UI** and **json-render**.

The **W3C Generative UI Community Group** is pure spec-ware. It has exploratory notes only.
It has no deliverable yet, at a pre-standardization stage.

**5. Preset-saving mechanism per approach:**

| Approach | What a preset stores | Replay |
|---|---|---|
| A2UI-style, and json-render | The structure spec: component tree, IDs, and prop bindings, kept separate from the data | Re-run the originating data query, re-mount the same spec against fresh results |
| AG-UI | No native document to save. Capture the tool-call/query that produced the last `STATE_SNAPSHOT`, and optionally the snapshot itself for instant redisplay | Re-issue the same tool call over AG-UI. Show the cached snapshot while the refresh streams in |
| MCP Apps | The tool name and arguments that seeded the `ui://` resource. This is coarse-grained. The app itself is an opaque iframe rather than a structured component tree | Re-invoke the tool. Host re-fetches and re-renders the whole app resource |

## Recommendation for StreamSurface

**Protocol choice:** AG-UI as the transport layer, carrying an A2UI-shaped declarative
schema constrained to our own Level-2 component catalog.

- AG-UI gives us a transport-agnostic, already-adopted event stream. It carries state deltas,
  tool calls, and human-in-the-loop interrupts. This will work no matter which frontend
  framework the console ends up on. This matters because `DESIGN.md` section 4.1 has not
  locked the framework yet.
- On top of that transport, model StreamSurface's message format on A2UI's proven pattern.
  Use a flat component list, stable IDs, and structure and data separated into different
  messages, rather than inventing our own from scratch. We do not need to adopt A2UI as an
  external dependency wholesale. `json-render`'s catalog-plus-patch approach is a viable,
  smaller, framework-agnostic implementation of the same idea. It is worth prototyping first,
  since it is already framework-agnostic OSS with React, Vue, and Svelte renderers today.
- Reject **MCP Apps' iframe-HTML model** for StreamSurface itself. It is production-real and
  well-supported. But its trust and rendering model is the opposite of our contract. MCP Apps
  allows arbitrary sandboxed HTML/JS per app, opaque to the host. Our rule requires that an
  agent never emit arbitrary HTML, and compose only from our vetted Level-2 vocabulary. Keep
  MCP Apps in mind only as a *separate*, later integration path. It could help if the console
  ever needs to embed a third-party MCP tool's own UI. That is a different use case from
  agent-composed StreamSurface views.

**Renderer shape:**
1. A component catalog registering every Layer 1 to 4 component the agent is allowed to
   place, by name, with validated props. This is the Level-2 vocabulary from `components.md`.
2. StreamSurface receives AG-UI events. It applies them as structure-then-data patches against
   a mounted tree keyed by stable component IDs. This design gives us stable placement for
   arrived parts, matching `DESIGN.md` section 7.2, without extra work. No agent view has to
   re-solve this problem on its own.
3. Mode is **never** chosen by the agent. The host slot passes `mode`, meaning full, medium,
   or compact, into the catalog at render time. This uses the Component modes system in
   `components.md`, exactly as the existing contract requires.
4. **PresetSaver** persists two things per preset. It saves the structure spec, meaning the
   catalog-referenced component tree with no data baked in. It also saves the data query or
   tool-call descriptor that produced it. Reopening a preset re-runs the query and re-mounts
   the spec. This is the same overview-then-refresh loop that AG-UI's snapshot and replay
   pattern already implies. No bespoke persistence format is needed.

## Sources

- [AG-UI Overview, Agent User Interaction Protocol](https://docs.ag-ui.com/introduction)
- [GitHub, ag-ui-protocol/ag-ui](https://github.com/ag-ui-protocol/ag-ui)
- [AG-UI: The Future of Agent-Driven User Interfaces, Microsoft Community Hub](https://techcommunity.microsoft.com/blog/appsonazureblog/ag-ui-the-future-of-agent-driven-user-interfaces/4515769)
- [Introducing A2UI: An open project for agent-driven interfaces, Google Developers Blog](https://developers.googleblog.com/introducing-a2ui-an-open-project-for-agent-driven-interfaces/)
- [A2UI.org](https://a2ui.org/)
- [The A2UI Protocol: A 2026 Complete Guide to Agent-Driven Interfaces](https://a2aprotocol.ai/blog/a2ui-guide)
- [AG-UI and A2UI: Understanding the Differences, CopilotKit](https://www.copilotkit.ai/ag-ui-and-a2ui)
- [Open-JSON-UI, OpenAI's Generative UI Standard, CopilotKit docs](https://docs.copilotkit.ai/agno/generative-ui/open-json-ui)
- [The Developer's Guide to Generative UI in 2026, CopilotKit](https://www.copilotkit.ai/blog/the-developer-s-guide-to-generative-ui-in-2026)
- [GitHub, CopilotKit/generative-ui](https://github.com/CopilotKit/generative-ui)
- [MCP Apps overview, Model Context Protocol docs](https://modelcontextprotocol.io/extensions/apps/overview)
- [MCP Apps Now Official, Model Context Protocol Blog](https://blog.modelcontextprotocol.io/posts/2026-01-26-mcp-apps/)
- [GitHub, modelcontextprotocol/ext-apps](https://github.com/modelcontextprotocol/ext-apps)
- [MCP-UI docs](https://mcpui.dev/)
- [AI SDK by Vercel, docs](https://ai-sdk.dev/docs/introduction)
- [Introducing AI SDK 3.0 with Generative UI support, Vercel](https://vercel.com/blog/ai-sdk-3-generative-ui)
- [GitHub, vercel-labs/ai-sdk-preview-rsc-genui](https://github.com/vercel-labs/ai-sdk-preview-rsc-genui)
- [Thesys, The Generative UI Company](https://www.thesys.dev/)
- [Building the First Generative UI API, Thesys blog](https://www.thesys.dev/blogs/generative-ui-architecture)
- [GitHub, thesysdev/openui](https://github.com/thesysdev/openui)
- [The State of Generative UI in 2026: Transports, Formats, and Tradeoffs, OpenUI](https://www.openui.com/blog/state-of-generative-ui-report)
- [json-render, Custom Schema & Renderer docs](https://json-render.dev/docs/custom-schema)
- [@json-render/react, npm](https://www.npmjs.com/package/@json-render/react)
- [Generative UI Community Group, W3C](https://www.w3.org/community/gen-ui/2026/)
- [AI Agent Protocol Community Group, W3C](https://www.w3.org/community/agentprotocol/)
- [Six Agent Protocols Every AI Builder Needs to Know in 2026, MindStudio](https://www.mindstudio.ai/blog/six-agent-protocols-ai-builders-2026)
