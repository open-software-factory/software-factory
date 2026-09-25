# Software Factory operator console design system

Software Factory is an autonomous software engineering product. This document is design guidance for its operator console.

Status: living draft. Product design owns this document, for the operator console.

This document is for anyone building the operator console UI and UX. A builder may be a human or an AI agent. The document states decisions and current candidates. Exploration history lives in `design-explorations.md`. Open items, next steps and review history live in `design-tracker.md`.

Legend for every entry:

- **[E] Established.** A binding constraint or decision from the product documents or the ADR. See §0. Design must honour it.
- **[C] Candidate.** Proposed here, and not yet validated. Test it, revise it, or replace it. Do not treat it as final.
- **[O] Open.** Unresolved, and tracked in `design-tracker.md`. Do not silently assume an answer.

How this document is organised: §1 to §11 set the stage. They cover principles, shell, and the interaction contracts that govern everything. §12 to §17 give element-level detail. This covers type, colour, spacing, icons, states, and accessibility. §18 points to the component catalogue.

Working conventions:

- Prototypes are runnable, self-contained HTML. Their tokens bind to this document, using the same names and the same values [E].
- Fixtures use realistic density, 30 to 100 work items. They are clearly labelled as simulated where not live [E].
- Every new surface is exercised in six states: healthy, busy, blocked, failing, disconnected (stale), and empty [E].
- Direction alternatives are preserved as separate files. Nothing is overwritten during exploration [E].
- Implementation libraries are candidates for the interaction model. The model is designed first [E].

> Do not turn product-document phrases into visible UI copy or labels. The doctrines in §1 are design constraints.

---

## 0. Sources and status

| Source | Role |
|---|---|
| `docs/product/ux/factory-ux-vision.md` | Product model, operator model, six conceptual surfaces [E] |
| `docs/product/ux/design-principles.md` | Visual, interaction, information, factory doctrine [E] |
| `docs/product/decisions/0001-factory-ui-is-an-operator-console.md` | Active-console decision, control requirements [E] |
| `docs/vision/product-lifecycle.md` | Outer loop, human judgment scope, portfolio context [context] |
| `docs/vision/agent-native-operating-model.md` | Two-loop model, product-engineer role, meta-loop [context] |
| `docs/product/ux/open-questions.md` | Non-decisions and open visual/technical questions [O] |

Operating scale is a locked session hypothesis [C]. It assumes a single factory. Work items in flight: 30 to 100. Attention events per day: 5 to 20.

---

## 1. Product character and design principles

### 1.1 Character statement [E]

The console supervises an autonomous software factory. This factory is an agent-executed engineering loop, governed by deterministic verification. Humans supply intent, taste and exception judgment. The console is an **active control surface**: read and control are designed together, over an engine that must remain operable outside the UI. Its temperament is calm when healthy, precise when questioned, and loud only when a human judgment is genuinely required. It is a control room and factory floor, a debugger and flight recorder, and an improvement laboratory. It is not a project-management app, a CI dashboard, an agent-status grid, or a KPI dashboard.

### 1.2 Non-negotiable principles [E]

| # | Principle | Rule |
|---|---|---|
| 1 | Attention is the scarce resource | Rank the interface by intervention value, rather than data availability/chronology. |
| 2 | Healthy is quiet | Routine autonomous work never competes visually with genuine exceptions. |
| 3 | Work before agents | Work items, intent, dependencies and outcomes are durable; agents are execution resources bound to work. Never render a roster of anthropomorphised agents. |
| 4 | Same truth, multiple projections | Every view is a projection of one shared entity/event model; no screen invents its own reality. |
| 5 | Overview, then explanation, then evidence | Every meaningful summary drills to why. Every explanation drills to raw evidence. |
| 6 | Direct manipulation over navigation | Zoom, pan, select, isolate, compare, overlay, scrub time, and drill. Preserve context throughout. |
| 7 | Time is first-class | Current state without history is insufficient. Replay must be visually distinct from live. |
| 8 | Motion encodes real state | Animate state change, causality and continuity. Never animate decorative activity. |
| 9 | Machinery is inspectable | Default to abstraction. Never trap operators behind it. |
| 10 | Flow, queues, constraints over counts | Starvation, congestion and parallelism limits must be visible. |
| 11 | Anomalies and deltas over totals | "What changed?" outranks another KPI. |
| 12 | Cost is contextualised | Show cost per useful outcome, deltas, and causal breakdowns. Never show a naked total. |
| 13 | Confidence only where it matters | Show qualitative confidence at decision points. Never show a meaningless percentage. |
| 14 | Operations and improvement are peers | Run the factory and improve the factory from the same console. |
| 15 | Controls require authority | Actions carry clear authority, feedback and auditability. This holds even when the first release ships read-only [E, ADR]. |
| 16 | Density allowed, clutter not | The console is a professional control surface. It avoids a handful of giant cards. |

### 1.3 What the console is not

- Not a chat home. Conversation is one modality of the ambient operator [E].
- Not a PR page clone. The review surface is a decision-and-evidence surface, bound to intent [C].
- Not a terminal. Mono type serves identity and raw evidence. It does not style the chrome [E].
- Not a fixed dashboard. Factory Intelligence is an exploratory surface [E].
- Not a God Graph, a single graph holding every relationship. The console offers lenses only [E].

---

## 2. Information hierarchy and density

### 2.1 Attention-rank order [E]

Factory-level hierarchy, best first:

1. Intervention-required state (P1 severity). See §11.4.
2. Abnormal or changing state (deltas, drifts, anomalies)
3. Flow and bottlenecks (queues, starvation, congestion)
4. Important operating context. This includes runway, capacity, and cost posture.
5. Routine raw metrics. These are available, and stay quiet.

### 2.2 The visualisation test [E]

Every chart, card or cluster must complete one sentence: *"I look at this when I need to know ______."* If the sentence cannot be completed convincingly, the element does not earn its place. Decorative metrics, repetitive labels and filler charts are banned.

### 2.3 Density contract per surface [C]

| Surface | Default density | Calm-state content | Rich-state content |
|---|---|---|---|
| Command / Attention | Sparse | Status line + runway + deltas | Ranked attention items, one root cause each |
| Factory Floor | Dense, spatial, calm | Whole topology at 10 m with health | Focused cluster at 1 m with queues and agents |
| Work Graphs (lenses) | Dense, structural | One lens active | Isolate/compare/critical-path overlays |
| Flight Recorder | Progressive | Phase-level timeline | Event, then run, then interaction, then tool call, then raw |
| Factory Runway (Stand) | Verbal | Runway verdict + drivers | Breakdown by constraint class |
| Factory Intelligence (Lab) | Dense, investigative | Query/cohort builder | Comparisons, anomalies, generated views |

### 2.4 Display rules [E + C]

- Delta-first. Show change against baseline before the absolute value, or instead of it, where a baseline exists.
- One strong emphasis per viewport [C]. The reserve of high-chroma meaning is spent on at most one primary subject.
- Confidence appears only at decision points: review, gate adjudication, runway estimate [E].
- Raw values never disappear. "Inspect" affordances open the machinery from any summary [E]. This machinery is exact numbers, events and traces.

---

## 3. Application shell and navigation

### 3.1 Shell decision

- The shell starts as **Direction B: Live Floor**. The Factory Floor canvas is home. Other modes are peers (§3.3). **This is a starting point.** It is not a final commitment.
- The shell must let the operator **contextually focus** on the attention queue, and on other surfaces, as the situation demands. The attention centre is a first-class focus state. It is not only a docked layer. How that focus state is entered and exited is open [O].
- Two explored alternatives are archived in `design-explorations.md`: direction A (Command Post) and direction C (Focus Console). The old rule that flipped between direction A and direction B is archived there too.

### 3.2 Shell rules

- The product vision defines six surfaces. These are **Command/Attention** (attention feed), **Factory Floor**, **Work Graphs**, **Flight Recorder**, **Factory Runway** (capacity outlook), and **Factory Intelligence**. §2.3 maps their densities. Whether they collapse into a few modes, as Direction B does today, is **not settled**. Or they may earn nav destinations of their own [O, previously tagged E]. The current working shape is a small set of modes. Everything else is a **lens**, a **layer**, or a **dock**.
- Exactly one home for human attention [C]: the attention strip or layer. Attention never scatters across surfaces.
- Selection is the thread. A focused entity, such as a work item, a cluster, or a gate, persists across lens and mode changes [E same-truth, C mechanism].
- The ambient operator is a cross-cutting modality: command palette, context action, annotation, generated view. It is not a surface and not a chat tab [E]. Its transport is [O].

### 3.3 Mode structure (Direction B) [C]

- **Floor.** Canvas home. The lens switcher re-projects the same model, using execution, dependency, architecture, or traceability. The attention layer sits on the right edge. Docks open beside the canvas.
- **Stand.** Factory Runway. A verbal verdict plus a constraint breakdown. It opens from Floor without losing canvas state.
- **Lab.** Factory Intelligence. An investigative surface. Generated views become transient **dynamic surfaces** [E vision]. They are never permanent nav.
- **Global chrome.** A status and time line: health, time scope, replay indicator. Plus a mode rail and a command palette trigger. Replay mode is visually explicit and disables interventions [C].

### 3.4 Navigation grammar

- Direct manipulation first, navigation second [E]. Every destination must justify itself against "what does this move cost my current context?"
- The focus path, a context breadcrumb, follows causality: intent, spec, work item, run, evidence. This is the same lineage the console is built on [E].
- Keyboard is a first-class navigation path. See §16.3 [C].

---

## 4. Layout and responsive behaviour

### 4.1 Targets [E + C]

- Tauri desktop first [E], with later reuse as web [E]. Tauri is a framework for building desktop apps with a web UI. Avoid anything that couples layout to native chrome. Use a canvas-based core that container-queries [C].
- Reference viewport: 1600 by 1000 px. Minimum usable viewport: 1280 by 800 px [C].
- No horizontal scroll at any supported size. All panning happens inside the canvas. It never happens on the page [E].

### 4.2 Shell geometry: starting defaults [C]

These are first-render defaults. They are not fixed sizes. Every panel is resizeable and closeable. Which panels are open persists per view. A panel the operator moves into a pane stays there across views. Widths persist once, for the whole shell (§5.2). Values refine as the shell matures.

| Zone | Default size | Behaviour |
|---|---|---|
| Mode rail (left) | 48 to 52 px | Modes and the ambient trigger |
| Status/time line (top) | 36 px | Health, time scope, replay badge |
| Content (centre) | Flexible | Canvas or surface of the mode |
| Dock (right) | 360 to 420 px | Recorder, evidence, or review panel. Resizeable up to half the viewport. |
| Timeline (bottom, recorder open) | 48 px | Playback scrubber. Appears only with the recorder. |

### 4.3 Responsive tiers [C]

This document sets three responsive tiers, by viewport width.

| Width | Behaviour |
|---|---|
| 1280 px or more | Three-zone layout, as in §4.2. |
| 1024 to 1279 px | Docks become overlays, anchored right. The canvas stays full-bleed. |
| Under 1024 px | Canvas-only, with overlay surfaces. The mode rail collapses to an icon rail. The attention strip becomes a top card. |

Mobile layouts are redesigns. They are not squeezed versions of the desktop layout [E general contract].

### 4.4 Canvas layout and zoom

- The base layout is deterministic and layered, built on a directed acyclic graph. Positions stay stable across lenses where possible. Lens switches morph the view rather than reload it [C].
- Zoom changes representation. It does more than change scale [E progressive abstraction]. See §10.6.

---

## 5. Shell interaction contract [E]

These rules govern the app shell in every mode: web, desktop, tablet, mobile. They apply at every media size. Apply them at the style layer where possible. Encapsulate each behaviour in a reusable, composable component. `components.md` maps each rule to its owner.

### 5.1 No jarring layout change

1. **A click must never** shove the layout. New information arrives in one of two compliant forms. An **overlay** slides or fades in over the layout. The user may then **pin** it: pinning docks the overlay, and the layout resizes smoothly to make room. Or a **panel** animates into place directly, for a surface already decided to start docked, such as one of the nav's secondary panels.

   A previously pinned overlay is simply a panel, and it stays compliant. An instant reflow is never allowed. Content the operator is reading must not jump, resize, or shift under an unrelated click.

   **The form a surface takes** follows the trigger. It does not follow the surface. The operator asked for it, from the nav, the command palette, or a Move control. So a layout change is expected, and the surface may dock directly. Or the system raised the surface, or a click on content produced it, for example by selecting a node or opening evidence. So it arrives as an overlay. Transient inspection must not rearrange a workspace the operator did not ask to rearrange. Pinning is the act that promotes a transient overlay into a durable panel [C].
2. A full-screen change gets a smooth transition. It never gets an instant swap. This applies to the main content area, a view, or a page, for example a change from canvas to board.
3. Dropdowns, modals, and the global search field animate in. Nothing appears "out of nowhere".
4. Any element that fills most of the central space animates its zoom, pan, and programmatic scroll. This includes a graph, a table, or a list.
5. Motion uses the tokens in §16.5. **Motion is always on.** No host signal turns it off: no media query, no capability check, nothing read at boot or at call time. If reducing motion is ever offered, it is an explicit operator setting and nothing else. Turning it off must announce itself on screen.

### 5.2 Panels and workspace

1. A panel that appears over the layout can be **pinned**. Pinning docks it. The layout then resizes smoothly to make room. Pinning is reversible.
2. Every panel is closeable. Every side panel is resizeable.
3. Per-view panel *presence* persists automatically. This covers which panels are open, which one is active, and whether each is pinned or floating. So the user never needs to re-open or re-pin a view to restore its layout. Region widths and navigation width are global values, set once for the whole shell. A resize states how much room the operator wants on this screen. A seam that moved on every view switch would read as the layout shoving itself (§5.1.1) [E]. This rule was amended on 12 September 2026. Widths used to be set per view. This rule was amended on 25 September 2026. A surface the operator moves into a pane stays there across every view. It stays until the operator closes it or moves it again. Per-view memory covers the panels opened during work.
4. Pinned panels grow into a **workspace** [C]: left and right panes, and potentially a bottom pane, around a tabbed central content area. Panels can host tabs, be rearranged, and **pop out** into separate windows.
5. **Focus mode** [C]. One action collapses every pane. The central content fills the shell, with only subtle affordances to bring panes back. Leaving focus mode restores the exact prior layout.

### 5.3 Ambient assistant

1. A circular floating button. Draggable. Overlays everything in the app.
2. Its menu is configurable by context and viewport. It offers different options and visuals per situation. This is still being iterated on [O].

### 5.4 Input modes

1. Every interface element works with mouse, keyboard, and touch, and moulds to the inputs the device offers. Hover-only semantics must have a touch-reachable equivalent.
2. Voice is a primary input on **all** text inputs and on the ambient assistant.
3. Touch targets follow the platform minimum. This is 44 px on iOS and 48 dp on Android.

### 5.5 Findability and orientation

1. Any list with more than 10 items gets a free-text omni-search. It runs client-side by default, and server-side where possible.
2. Deep drill-downs show where the user is, for example with breadcrumbs. This applies after multiple clicks or zooms in. The user can jump back multiple levels in one action. They are never forced to step back one level at a time.
3. The global search opens with Ctrl+P, the keyboard shortcut for it. It is light-dismissable. Click outside, or press Esc, to close it.
4. **Navigation lists two kinds of thing.** It never gives them the same marker. A **view** is a destination for the central content area. Exactly one view is current at a time, and the nav always shows which one. A **panel** is a surface that opens beside the centre. Any number of panels can be open at once, and opening one never changes which view is current. Views take radio semantics: `aria-current="page"` plus an accent marker. Panels take switch semantics: `role="switch"` plus an open or closed indicator. Panels never take the accent marker. A view that is open in the centre, but is not the active tab, reads as available. It does not read as selected [C].
5. Because panels are not destinations, they need a route of their own. The nav's panel group is the primary route. The command palette is the secondary route. A panel's default slot, left, right, or bottom, is declared by the surface. The nav does not choose it [C].
6. **A rule separates the two sections.** The nav reads as primary navigation above, and secondary surfaces below. The rail carries the same divider [C].
7. **Navigation is not a panel.** No ordinary control may remove it. It is the route back to everything else. So closing a panel, toggling a region, or changing a width, all leave it standing. The region toggles show and hide *panels*. Navigation is the shell's own furniture. The single exception is focus mode. It is an explicit mode, with a visible exit and a keyboard one. It restores navigation on the way out [C].
7. **A surface declares, the slot decides.** Every surface states its width appetite, whether it survives rail width, where it opens from closed, and which slots it accepts. The shell negotiates against those declarations, instead of hard-coding placements. A refused slot is shown disabled rather than hidden, so the rule stays visible. Acceptance is permissive until real views prove a placement wrong [C].
8. **A surface's kind** is its current placement. It is not a fixed property. A surface can be relocated: by drag-and-drop, by a Move control, or by a saved layout. Because of this, the nav derives each item's section from where that surface sits right now. In the centre, a surface is a view. Anywhere else, it is a panel. A surface declares a *home* slot. This slot is used only to place the surface on first open, and to decide which section the surface appears in while closed. A nav item that changes section animates from its old position, rather than jumping (§5.1) [C].

### 5.6 Generated and streaming UIs

1. The console will lean on agent-generated, progressively streaming views, rather than a wall of pre-configured dashboards. These views follow class standards such as A2UI and AG-UI, or JSON-render, a technique that draws the UI directly from structured JSON data. A2UI and AG-UI are emerging protocols that let an agent describe a UI for a client to render.
2. When an agent composes a dynamic view, the user can always **save it as a preset** and reuse it without re-writing the prompt.

### 5.7 Text readability floor

1. Rendered text is readable at every zoom level and on every screen size. No label ever appears on-screen below the smallest step of the type scale (§12.2).
2. When zooming out would push a label below that floor, exactly one of two things happens. **Hold.** The label counter-scales against the zoom, and keeps its on-screen size. Or **hide.** The label disappears, and the next zoom-ladder abstraction (§10.6) carries the meaning instead. For example, one cluster label can replace many node labels.
3. Density reduction removes text. It never shrinks text into illegibility.
4. The owner is MotionViewport, the shared component that handles zoom and pan, defined in `components.md` layer 1. Every zoomable surface inherits this from it.

---

## 6. Content and voice [C]

### 6.1 Voice

- Calm, factual, specific. The console reports. It does not emote, cheer, or apologise reflexively.
- Every message answers up to three things, in order: what happened, what it means, what to do. Omit what does not apply. Never omit "what to do" when an action exists.
- State facts. Do not assign blame. Write "target not found." Never write "you entered an invalid target."
- Sentence case for headings, labels and buttons. ALL-CAPS only in established mono eyebrows (§12.3).
- Buttons and commands start with a verb, such as "Approve" or "Stop run". Destinations are nouns, such as "Runway". This matches navigation grammar (§3.4).
- Uncertainty uses the §11.8 vocabulary: unverified, likely, confirmed. These are words. Never use invented percentages.

### 6.2 Terminology

One canonical term applies per concept. Synonyms are banned from the UI. Domain terms re-derive from the domain model when it lands. See the §11 scope note.

| Term | Meaning | Banned synonyms |
|---|---|---|
| work item | unit of factory work | task, ticket, job, card |
| run | one execution attempt | build, session |
| agent | subordinate AI worker | bot. "Assistant" is reserved for the operator-facing assistant. |
| operator | the human | user, admin |
| gate | automated check that must pass | check, guard |
| evidence | recorded proof of what happened | "logs" as an umbrella term. Logs are one evidence kind. |
| lens | a way of drawing the same model | view, mode |
| blocked | cannot proceed without a decision or dependency | stuck, paused |
| fixture | simulated demo data | None. Never present a fixture as live. |

Health states in UI copy are Healthy, Attention, Blocked, Failure, and Emergency, though internal ids may differ.

**Labels name the thing.** They never name the widget it sits in. Panel, pane, view, tab, sidebar and region are words for how the shell is built. An operator is looking for *what* something is, and *which one*. A label may use one of those words only where the thing has no name of its own.

| Instead of | Write |
|---|---|
| Open Attention panel | Open Attention |
| Inspector | Work item · service-auth#5 (example) |
| Close this view | Close Floor |
| Left panel | Open Attention *(name the panel)* |
| Focus mode: hide every pane | Focus mode |

The same rule governs a header. A pane shows the **type and reference** of what it holds, for example `Work item service-auth#5` or `Run output run-4471`. This is because "Inspector" names the mechanism doing the looking, rather than the thing being looked at. A surface holding a collection, rather than one thing, has no single reference. An attention queue or a recorder are examples. These surfaces keep their own label [C].

### 6.3 Errors

- Shape: what failed · why (if known) · what to do. One sentence each, in that order.
- Prose first. Codes and ids come after, in mono. Never show a bare code alone.
- Every error links to its evidence (recorder, raw output). "Something went wrong" with no route to evidence is banned.
- Unknowns stay unknown. Write "cause not yet determined." Do not write a guess.
- No "Oops", "Whoops", "Uh-oh".

### 6.4 Agent-written text

- Agent output rendered into the UI follows this whole section. StreamSurface, the component that renders streaming agent output, hands composing agents this contract. See §5.6, `components.md` Level 2.
- Long agent text truncates with a route to the full text as evidence. It never reflows the chrome around it.

---

## 7. Feedback and loading [C]

### 7.1 Channel taxonomy

A message lives in exactly one active channel. Escalation moves it. It is never duplicated across channels. Everything transient is recoverable from history. A missed toast is never lost information.

| Channel | Carries | Lifetime | Severity |
|---|---|---|---|
| Toast | Confirmation of the operator's own action. Carries undo (§8). | Auto-dismiss ~4 s. Stacking policy is open [O]. Several toasts at once may be allowed; this will be decided after the component survey. Each toast is dismissible individually, and the whole group is dismissible together. | info |
| Badge / count pill | Passive counts that route to a list | while count > 0 | info to P2 |
| Attention item | Anything needing operator judgement | until resolved; ranked (§11.3) | P2 to P1 |
| Banner | Factory-level condition affecting the whole session | while the condition holds; one at a time, highest wins | P1 |
| Dialog (P0 interruption) | The factory cannot proceed safely without a decision | until decided | P0 |
| History / inbox | Append-only record of all of the above | permanent | all |

### 7.2 Loading and progress

- Nothing renders blank while loading. A skeleton mirrors the final layout. It appears only after 150 ms, so there is no flash on fast loads. It animates subtly, under the motion policy in §5.1.5.
- A known duration gets a determinate bar. An unknown duration gets an activity indicator, plus elapsed time once it passes 5 s.
- Streaming happens through StreamSurface. Parts render as they arrive. Skeletons stand in only for parts not yet arrived. Arrived parts never re-layout (§5.1).
- Optimistic updates apply immediately, for near-certain operator actions. Mark the update as syncing. On failure, roll back visibly through an attention item. Never roll back silently.

---

## 8. Actions: confirmation, undo, recovery [C]

1. The default is to **act immediately, and offer undo.** Confirmation dialogs are reserved for actions that cannot be undone.
2. Judgement verbs, such as approve, request-changes, or clarify, take one click with no confirmation. Each is undoable until a downstream agent or gate consumes the decision. After consumption, the correction is a new action, such as request-changes. It is not undo.
3. Operation verbs, such as redirect or stop, take one confirmation. That confirmation states the consequence in one sentence, for example "Stops the run. Work so far is preserved as evidence." `stop` is not undoable. That is exactly why it confirms.
4. Undo lives in the action's toast (§7.1). It remains available in the item's inspector while the window lasts.
5. Every action, operator or agent, lands in the recorder. A failed or rolled-back action produces an attention item with evidence. It is never a silent revert.
6. No operator action ever destroys evidence. Truly destructive actions do not exist in the console, by design.
7. For form validation, validate on submit or on leaving a field. Never validate on the first keystroke. Error text follows §6.3, placed at the field. Focus moves to the first error.

---

## 9. Time and numbers [C]

### 9.1 Time

- Under 24 hours, use a relative time, such as "4 m ago". Update it per minute. Never update it every second. Beyond 24 hours, use the absolute date and time.
- The full absolute timestamp with UTC offset is always one hover or tap away. A view never mixes timezones. The display zone is the operator's local zone.
- Durations use at most two units, such as "1 h 20 m". Seconds appear only under a minute.
- In replay (§16.7), every time on screen anchors to the replayed moment, and the UI says so.

### 9.2 Numbers

- Tabular, mono numerals appear in tables, tiles and readouts (§12).
- Precision follows what the decision needs. Never show more precision than that. Derived metrics use at most 3 significant digits (extends §2.4).
- Money: show cents under $100, and whole dollars above it.
- Deltas: signed, and coloured with semantic tokens. Colour is never the only carrier (§17).

---

## 10. Graph and canvas visual grammar

### 10.1 One model, many lenses [E]

Nodes and edges are projections of the factory's shared entity and event model. Lenses never invent new entities. This includes the execution, dependency, architecture, and traceability lenses. They change only which edges and levels of detail render. A God Graph, containing every relationship, is forbidden.

### 10.2 Node classes [C]

Glyph characters below are Unicode approximations, so the shape language is visible while reading. The canvas draws its own monoline set. The rendered glyph sheet ships with the node-grammar exploration.

| Class | Glyph | Binding rule |
|---|---|---|
| Work item | ▢ rounded square, state fill | Primary durable entity |
| Subtask | **Unresolved.** Explore before finalising [O]. It must scale to about 20 per item, so no glyph-per-subtask at rest. Candidates on the parent node: a progress bar with a count, such as `▰▰▰▱▱ 3/5`, in the GitHub Projects style. Or a segmented donut with a count, such as `◔ 3/5`, in the style of Linear, a project-management tool. Individual subtasks appear only on focus or zoom-in (§10.6). | Child of work item |
| Agent | ● small filled circle, pinned to work edge | An execution resource. Never first-class. Visible at 1 m zoom or closer. |
| Gate | ⛨ shield | Deterministic verification |
| Review | 웃 person | Only while judgment is in flight |
| Repo / service | # octothorpe | Code structure |
| Module / domain | ▤ rounded doc | Code structure, 1 m zoom |
| Deploy target | ⚑ flag | Destination of a deployment |
| Environment | ⬡ hexagon | Build/test/staging context |
| External dependency | ◇ diamond, dashed border | Out of factory control |
| Cluster | ⬭ rounded region | Aggregation at 10 m. Label plus health. |

### 10.3 Edge classes [C]

| Edge | Style | Meaning |
|---|---|---|
| executes | `────` solid, thin | from work item to agent activity |
| depends-on | `┄┄┄┄` dashed | dependency direction |
| blocks | `┄┄┄┄` dashed, state-fail tone | active blocking (a constraint) |
| produces / emits | `┈┈┈┈` dotted | artifact or event emission |
| verifies | `────` thin, to gate | gate check binding |
| deploys-to | `╌╌╌╌` long-dash | deployment path |
| traced-from | `┈┈┈┈` very faint | traceability. Only rendered in the traceability lens. |

### 10.4 State encoding [C]

| Channel | Meaning |
|---|---|
| Fill | Current state (§11 palettes) |
| Border | Node class or context |
| Halo | Attention only |
| Size | Scope, at most two size steps. Never a size gradient. |
| Heat | Muted edge or node tone, for congestion. Rendered only in the dependency lens. |

### 10.5 Cognitive aids [C]

| Action | Effect |
|---|---|
| Focus a node | Unrelated nodes dim. |
| Isolate | Everything but the selected subgraph recedes. |
| Compare (Lab) | Two contexts appear side by side. |
| Critical path | The path gets a distinct overlay stroke. |
| Blocked clusters | A "why" affordance dims others and highlights the common dependency. The ambient operator does the same thing [E vision example]. |

### 10.6 Zoom ladder [E progressive abstraction, C representation]

| Level | Shows | Interactions |
|---|---|---|
| 10 m | Clusters, health, and attention marks | Read, navigate, focus |
| 1 m | Work items, edges, queues, agents, gates | Select, isolate, expand, open recorder |
| 10 cm | Event chips, tool calls, diffs, raw values | Full evidence, provenance |

Semantic zoom changes representation. It does more than scale glyphs.

### 10.7 Time on canvas [C]

A global scrubber sits at the canvas bottom. In replay, the whole canvas re-lays at *t*, with the recorder playing alongside. Live updates pause visually during replay. A "return to live" affordance is one click away. No motion happens during replay scrubs. State changes jump instead, which is reduced-motion friendly.

### 10.8 Layout [C]

The layout is deterministic and layered, with stable anchoring. Repos, gates, and targets hold their positions. Work items flow between them. Lens switches morph in place. Expanding a cluster preserves spatial orientation [E motion rules].

---

## 11. Status, severity, attention and lifecycle semantics

**Scope note.** The factory's domain and data model is not finalised. The state vocabularies in this section are **provisional stand-ins** for prototyping, derived from the product docs. This covers lifecycle states, health states, blocked causes, gate, deployment and agent states, and attention-item states. This document does not own these vocabularies. They will be re-derived from the domain model when it lands [O].

This document does own the encoding rules, and keeps them regardless. Every state renders as colour, glyph and label together. Shape alone must identify the state. Severity maps to feedback channels (§7). Attention items deduplicate to root cause.

### 11.1 Work lifecycle [C]

| State | Meaning | Indicator: colour, glyph, label |
|---|---|---|
| READY | Spec-validated, executable, queued | idle fill, ▷ arrow, "ready" |
| IN_PROGRESS | Agent(s) executing | active fill, ● steady pulse-dot, "working" |
| VERIFYING | Gates running | active fill, ◌ spinner, "verifying" |
| IN_REVIEW | Judgment review in flight | recovering fill, 웃 person, "review" |
| GATING | All gates / fitness checks | ok fill, ⛨ shield, "gating" |
| DEPLOYING | In flight to target | active fill, ↑ up-arrow, "deploying" |
| DEPLOYED | Live at target | ok fill, ⚑ flag, "deployed" |
| SIGNED_OFF | Outcome recorded, verdict done | faint ok, ✓ check, "done" |
| BLOCKED | Held on a cause (below) | blocked fill, ‖ pause, "blocked · <cause>" |
| FAILED | Terminal failure. It does not recover. | fail fill, ■ stop, "failed" |
| RECOVERING | Auto-retry or remediation in progress | recovering fill, ↻ refresh, "recovering" |
| ABORTED / ROLLED_BACK / PAUSED | Explicit stops | idle or stale fills. ⨯ aborted. ↩ rolled back. ‖ paused. **Paused shares its shape with BLOCKED.** Differentiate them in the glyph sheet. |

Blocked causes [C]:

| Cause | Meaning |
|---|---|
| `DEPENDENCY` | Waiting on another work item |
| `HUMAN` | Waiting on a person |
| `CLARIFICATION` | The spec is nearly ready |
| `AMBIGUOUS` | The agent cannot resolve it |
| `CAPACITY` | Runners are saturated |

### 11.2 Health states [C]

| Health state | Meaning |
|---|---|
| `HEALTHY` | All nominal, and quiet |
| `ACTIVE` | Healthy and working |
| `DEGRADED` | Partial capacity, no intervention needed |
| `BLOCKED` | See §11.1 |
| `FAILED` | See §11.1 |
| `RECOVERING` | See §11.1 |
| `IDLE` | No work available. This is distinct from healthy. |
| `STALE` | No data, or a disconnected signal |
| `OFFLINE` | Not connected |

This uses the same palette as §11.1. Glyphs differ between states, for example a ring dot, a triangular arrow, or a cross. So shape alone identifies the state.

### 11.3 Severity and urgency [C]

Priority score equals impact, multiplied by urgency, multiplied by deviation from baseline. This formula is a candidate. Display the reasoning. Never display a bare number.

| Tier | Meaning | Threshold example (candidate) |
|---|---|---|
| P1 | Act now | Production failing, security exception, irreversible step pending, starvation imminent |
| P2 | Act soon | Review pending, ambiguity, cost/retry anomaly above threshold |
| P3 | Plan | Drift, calibration, improvement signals |
| Noise | Suppressed | Below threshold; visible only in Lab cohorts |

Urgency is approximately time-to-impact. Impact is approximately blast radius, multiplied by reversibility. Every P1 or P2 item must state *why* it is urgent, in one line. For example: "gate unit-tests failing 40 min, 6 items queued behind."

### 11.4 Attention semantics [E classes, C mechanics]

Attention classes [E]:

- Production failure.
- Failed deployment or rollback.
- Security exception.
- Judgment review, for a pull request or an architecture decision.
- Agent blocked on ambiguity.
- Irreversible migration decision.
- Rapidly rising cost or retry loop.
- Approaching work starvation.

Item states [C] flow in order: `NEW`, then `ACKNOWLEDGED`, then `IN_PROGRESS`, then `RESOLVED`. A resolved item carries a decision type, a rationale, and an audit record. The authority, feedback and auditability requirement is [E]. An item can also become `SNOOZED`, with a review time, or `SUPPRESSED`, with a reason.

Deduplication follows one rule. One attention item exists per root cause. The console never creates one item per symptom [C].

### 11.5 Verification semantics [C]

Gate run states: `PENDING`, `RUNNING`, `PASSED`, `FAILED`, `SKIPPED`, and `FLAKY` (a candidate state).

Gate health states: `CALIBRATED`, `NOISY`, and `LOW-YIELD`. `NOISY` means high false positives. `LOW-YIELD` means the gate is correct, but has low actionable value.

Each gate carries an actionable-yield and false-positive history. This is the raw material for escape analysis and Lab investigations [E meta-loop].

### 11.6 Deployment semantics [C]

Deployment states: `QUEUED`, `BUILDING`, `CANARY`, `PROMOTING`, `LIVE`, `ROLLING_BACK`, `ROLLED_BACK`, `FAILED`. Deployments are work items' children, in the execution lens. Rollbacks are first-class events, with their own evidence segments.

### 11.7 Agent and execution-resource semantics [E + C]

Agent states are subordinate. They are never a roster. The states are: `IDLE`, `EXECUTING`, `WAITING`, `TOOL_RUN`, `ERROR`. Dimensions that matter for the operator are: provider, model, skill or prompt version, and runner capacity. Capacity is only surfaced as *capacity versus ready-work supply*, in Stand. It is never surfaced as a count of agents [E show-work-before-agents].

**Render on work.** Actors appear only as a lens [C]. An agent is always identifiable on the work item it serves. This uses an `AgentPin` bound to the node. It is never a standalone row. So the floor never reads as an agent-status grid.

A separate actor-centric projection is possible. It is surfaced as an **Actors** lens that re-groups the same model by agent. This is an experiment, to confirm whether a standalone actor view earns a place. It is not a primary surface by default.

### 11.8 Evidence, provenance, confidence [E + C]

| Claim grade | Meaning |
|---|---|
| OBSERVED | Directly measured instrument output (gate, event, trace) |
| DERIVED | Computed from observed data (rates, deltas, cohorts) |
| REPORTED | Agent or assistant claim. Unflagged in summaries, tagged in evidence. |
| UNVERIFIED | No deterministic confirmation yet |

Rules:

- Every summary exposes an evidence chain, in one click [E]. This chain is source, instrument, event id, and timestamp.
- Causal explanations state what evidence caused the next decision [E flight recorder].
- Confidence is qualitative (`HIGH`, `MEDIUM`, `LOW`), and shown only at decision points [E].

---

## 12. Typography

### 12.1 Families [C]

| Role | Candidate | Alternative | Rule |
|---|---|---|---|
| UI sans (body, controls, narrative) | IBM Plex Sans | Geist; fallback system-ui | Two families total; no display serif initially [C]; dense long-session readability; must not read as terminal [E] |
| Mono: identity, values, code, raw evidence, diffs | IBM Plex Mono | Geist Mono | Scoped by rule. Chrome stays in the sans family. |

Rationale: Plex is an engineering-family with strong small-size legibility and a neutral, non-consumer character. It avoids the generic model-default look while staying calm. Final choice is an open exploration item [O]. Test against alternatives in the first prototype.

### 12.2 Type scale [C]

| Token | Size / line-height | Weight | Use |
|---|---|---|---|
| `--text-caption` | 11 / 1.35 | 400 | Captions, timestamps, provenance notes |
| `--text-label` | 11 / 1.3 | 500 | Field labels, column headers, chips |
| `--text-body` | 13 / 1.45 | 400 | Default UI text |
| `--text-strong` | 13 / 1.45 | 600 | Emphasis inside body (item titles, verdicts) |
| `--text-section` | 15 / 1.4 | 600 | Panel and section titles |
| `--text-prominent` | 18 / 1.3 | 600 | Surface-level headlines (rare) |
| `--text-verdict` | 22 / 1.25 | 600 | Runway verdict, single numbers in Stand |
| `--mono-metric` | 12 / 1.4 | 400 | Numeric data, timings, hashes, ids, in tabular figures |

**UI scale [C].** The sizes above are design units on a 16 px base. The shell renders every size in rem, so one number sets the scale of the whole console. This covers text, spacing, controls, seams, and the orb together.

The default is 125 %. This sets the root font-size to 20 px. Body text then renders at 16.25 px, and captions at 13.75 px.

Model widths in the placement code carry the same scale explicitly. This includes the rail, navigation, pane appetites, and the centre floor.

The owner decided this on 15 September 2026. They found 125 % browser zoom easier to read than 100 % zoom.

### 12.3 Rules

- All numeric data uses tabular figures [E density discipline].
- Sentence case everywhere. No all-caps chrome, no decorative eyebrows [C].
- Minimum body size 13 px in quiet surfaces. 12 px allowed only inside raw-evidence wells [C].
- Text contrast never below AA. See §13.4.

---

## 13. Semantic colour system

### 13.1 Colour as meaning [E]

- Small, coherent palette.
- Backgrounds use toned darks instead of pure black.
- State colours stay consistent across every surface.
- High-chroma is reserved for genuine attention.
- Colour never carries meaning alone. Glyph and text always accompany state.
- Not every work state gets its own saturated colour.
- Contextual tinting for major modes is idea-only [O].

### 13.2 Token groups (semantic) [C]

| Group | Tokens | Semantics |
|---|---|---|
| Backgrounds | `--bg`, `--bg-canvas` | App base; canvas slightly deeper |
| Surfaces | `--surface-1` (panel), `--surface-2` (raised), `--surface-3` (overlay) | Panels, popovers, docks, flat, no glass |
| Text | `--fg`, `--fg-muted`, `--fg-faint` | Primary / secondary / tertiary |
| Borders | `--border-subtle`, `--border-strong` | Dividers; interactive outlines |
| Accent | `--accent`, `--accent-fg` | Selection, focus, primary interactive; ≤ 2 uses per screen |
| State colours | `--state-ok`, `--state-active`, `--state-blocked`, `--state-fail`, `--state-recovering`, `--state-idle`, `--state-stale` | Health/lifecycle, always glyph+text paired |
| Attention | `--attention` | Reserved: intervention-required emphasis, ≤ 1 element per viewport |
| Derived | `--fill-*` per state (node faces, chips), `--heat-*` (congestion, muted) | Node fills ~1.5 tones above bg; heat never saturated |

### 13.3 Starter palette [C, unvalidated]

The base hue family is blue-green (~210), with tint direction set by the design principles. Values are a provisional starting point for variation exploration.

| Token | OKLch |
|---|---|
| `--bg` | `oklch(0.165 0.014 210)` |
| `--bg-canvas` | `oklch(0.145 0.012 210)` |
| `--surface-1` | `oklch(0.200 0.015 210)` |
| `--surface-2` | `oklch(0.240 0.016 210)` |
| `--surface-3` | `oklch(0.280 0.017 210)` |
| `--border-subtle` | `oklch(0.300 0.014 210)` |
| `--border-strong` | `oklch(0.410 0.016 210)` |
| `--fg` | `oklch(0.930 0.008 210)` |
| `--fg-muted` | `oklch(0.710 0.012 210)` |
| `--fg-faint` | `oklch(0.550 0.012 210)` · large/secondary only |
| `--accent` | `oklch(0.780 0.100 170)` |
| `--accent-2` | `oklch(0.770 0.110 160)`, teal, dual-accent / positive |
| `--state-ok` | `oklch(0.760 0.090 150)` |
| `--state-active` | `oklch(0.760 0.080 215)` |
| `--state-blocked` | `oklch(0.780 0.100 75)` |
| `--state-fail` | `oklch(0.720 0.150 25)` |
| `--state-recovering` | `oklch(0.780 0.090 190)` |
| `--state-idle` | `oklch(0.620 0.010 210)` |
| `--state-stale` | `oklch(0.580 0.012 210)` · plus dashed glyph treatment (`╌`) |
| `--attention` | `oklch(0.800 0.150 75)` |

Each state's fill sits at `L −0.50` from the state text colour, with `C −0.04` and the same hue. The `oklch()` colour-mix function generates every derived fill [E technical contract].

**Session direction [C].**

- Blue-green base, with a dual accent, `--accent` blue plus `--accent-2` teal, and a mono-accent variant.
- No purple anywhere.
- Pastel-toned, low-chroma by default. High-chroma is reserved for attention.
- A tasteful dark theme is the default. It uses deep space-navy, a cyan/teal glow, and a subtle canvas-gradient depth only where it earns a plane.
- A light theme exists as a peer.

**Data-visualisation conventions [C].**

- Smooth point-to-point curves on all charts.
- Translucent, filled polygons for multi-axis or radar charts, always filled rather than left as empty outlines.
- A time-sync crosshair across related charts.
- Radar, spider, funnel, and sankey charts appear only where they add genuine signal.

### 13.4 Contrast targets [must verify in prototype]

| Pair | Target |
|---|---|
| `--fg` on `--bg` | ≈ 13:1 |
| `--fg-muted` on `--surface-1` | ≈ 5.5:1 |
| `--fg` on `--surface-3` | ≈ 10:1 |
| State colours on `--bg`, text | ≥ 4.5:1 |
| State colours on `--bg`, large or glyph | ≥ 3:1 |

`--fg-faint` is never used for normal text. All state indicators pair colour with glyph and text [E].

---

## 14. Spacing, borders, radii, elevation, surfaces

### 14.1 Spacing [C]

4 px base grid.

| Token | Value (px) |
|---|---|
| `--space-1` | 4 |
| `--space-2` | 8 |
| `--space-3` | 12 |
| `--space-4` | 16 |
| `--space-5` | 24 |
| `--space-6` | 32 |
| `--space-7` | 48 |
| `--space-8` | 64 |

Dense zones use `--space-1` through `--space-4`. Canvas padding and panel gutters use `--space-4` through `--space-6`, with no one-off values.

### 14.2 Borders [C]

Hairlines are 1 px. `--border-subtle` marks dividers, and `--border-strong` marks interactive outlines. Focus is a 2 px accent ring (see §16.2). Border, never shadow, separates surfaces (calm, flat) [C].

### 14.3 Radii [C]

| Token | Value | Use |
|---|---|---|
| `--radius-control` | 2 px | Inputs, buttons, chips, near-rectangle |
| `--radius-panel` | 4 px | Panels, cards, nodes |
| `--radius-overlay` | 6 px | Modals, popovers |
| Pill | n/a | Reserved for status chips only |

Rectangles and very small radii are the session preference [C]. This avoids the generic rounded-card look common to SaaS, a hosted web-app delivery model. List and content surfaces are separated by type hierarchy, spacing, and hairlines, instead of rounded, bordered tiles [C].

### 14.4 Elevation [C]

| Token | Use |
|---|---|
| `--shadow-1` | Panels above canvas (`0 1px 2px` black 40%) |
| `--shadow-2` | Raised docks, menus |
| `--shadow-3` | Overlays, command palette |
| `--glow-attention` | Attention halos only; 1 per viewport; never a resting state |

Solid surfaces only. Translucency stays restrained [E open-questions bias]. Raw-evidence wells use an inset surface, `--border-subtle`, no shadow, and mono type.

### 14.5 Surfaces [C]

Canvas backdrop is one tone below panels so the spatial model reads as the deepest plane. Panels never float on gradients. Subtle gradients are allowed only for depth cues on the canvas backdrop [E design principles].

---

## 15. Iconography

### 15.1 System [C]

- Monoline, 16 px grid, 1.5 px stroke, rounded caps. One weight is used everywhere, and 20 px appears only in prominent controls.
- Curated subset from one consistent set (Lucide-style). No mixing sources, no emoji [E anti-slop].
- Icons serve actions and entity classes only. A heading never gets an icon "because it looks designed" [E/every element earns its place].

### 15.2 Rules [C]

- Action icons always pair with a label when the action is primary. Icon-only is allowed only for established dense controls with tooltips and `aria-label`.
- State indicators use a dedicated glyph set, kept separate from icons. These are ● dots, › chevrons, and ╌ dashes. See §11. They always accompany colour.
- Graph glyphs are a separate grammar. See §10.2.

---

## 16. Interaction, focus and motion

### 16.1 Component state catalog [E]

Every interactive component defines twelve states. These are default, hover, focus-visible, active, selected, loading, streaming, disabled, warning, error, empty, and stale/disconnected. Streaming and autonomous states receive the same care as loading/error. Hover moves background lightness (`L ±0.06–0.12`), border, or position. It never moves foreground to a lower-contrast token [technical contract].

### 16.2 Focus [C]

Current candidate, to refine: 2 px accent ring, 1 px offset, on every focusable element. It stays visible in dark chrome, and mouse interaction never removes it. Only the underlying accessibility requirement is binding. It requires *some* clearly visible focus indicator on every focusable element (§17) [E].

### 16.3 Keyboard map [C]

| Shortcut | Action |
|---|---|
| `⌘/Ctrl K` | Ambient operator palette (also command nav) |
| `Alt 1/2/3` | Floor / Stand / Lab |
| `L` | Cycle lens (Floor) |
| `⌘/Ctrl F` | Focus search (entities, ids, hashes) |
| `E / R / X` | Open recorder / evidence / review dock for selection |
| `Space` / `← →` | Recorder play-pause / scrub (when recorder open) |
| `Esc` | Back up one context level, close dock |
| `+ / −` | Zoom (canvas) |

### 16.4 Selection semantics [C]

Single focus entity at a time. Multi-select happens only inside Lab comparisons. Selection survives mode and lens changes ("same truth"). Focus path in chrome always reflects the current entity.

### 16.5 Motion tokens [C]

| Token | Value | Use |
|---|---|---|
| `--dur-micro` | 120 ms | Hover, press, chip state |
| `--dur-panel` | 200 ms | Dock open/close, panel emergence |
| `--dur-transform` | 280 ms | Lens morphs, layout transitions |
| `--dur-emphasis` | 400 ms | Attention reveal, ≤ 2 cycles, then steady |
| Easing | `cubic-bezier(0.2 0 0.2 1)` | Standard; ease-out for emphasis |

**Animated, and allowed [E]:**

- Stage move of a work item.
- Graph expansion, preserving orientation.
- Detail panel emerging from the selected object.
- A real event travelling between components.
- Timeline update while live.
- States otherwise easy to miss.

**Forbidden [E]:**

- Decorative particles.
- Fake packet streams.
- Constant pulsing of healthy nodes.
- Ambient movement.
- Animation that delays data access.

### 16.6 Attention emphasis and reduced motion [E + C]

Attention reveal is a short pulse + steady halo, then stillness [C]. Motion currently follows §5.1.5. It stays always on, with no host-preference gate. If an explicit operator-controlled reduced-motion setting is introduced later, its treatment disables non-opacity motion and continues to convey changes by colour/border/glyph. Replay scrubs jump. This is a proposed future treatment, awaiting implementation.

### 16.7 Replay vs live [C]

In replay mode, chrome desaturates and a `REPLAY` badge stays persistent. Time control becomes a playhead, and interventions are disabled, with an explanatory tooltip. The canvas renders exactly the recorded state. Live mode is the default, and it stays visually distinct.

---

## 17. Accessibility

Non-negotiable checklist [E]:

- Full keyboard operability including canvas (pan/zoom/select) and recorder.
- Visible focus everywhere, with at least a 3:1 ring contrast.
- Semantic labels on all controls. Canvas nodes are `role="button"`/list items with names (`"Work item FS-241, VERIFYING, blocked on gate unit-tests"`).
- AA contrast for text and state indicators.
- State never conveyed by colour alone: glyph + text + (where sensible) position.
- Follow the explicit motion policy in §5.1.5. No host preference silently disables motion. Any future reduced-motion control must remain visible and reversible.
- Target sizes ≥ 44 px for isolated actions. Dense-row controls may be 28 px minimum, with adequate spacing [C].
- Screen-reader alternative for graphs and timelines [C]: a per-node text summary, plus an accessible "what changed" change-log table. The table covers time, entity, transition, and cause, the same content the operator reads at 10 cm, serialised.
- Live regions announce attention arrival: severity, class, and entity, without stealing focus [C].

---

## 18. Shared components

The component catalogue lives in **`components.md`**. It lists every component in five layers, which are surface primitives, then input primitives, then shell, then product, then generated UI. It also assigns each §5 contract rule to the one component that owns it, and it sets the build order.
