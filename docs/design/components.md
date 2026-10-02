# Software Factory console: component catalogue

Companion to `DESIGN.md`. This file is design guidance for building the console's components. Build status against the current prototype lives in `docs/design/design-tracker.md`.

`DESIGN.md` §5 is the shell interaction contract. This file maps each rule to **one** component that owns it. A behaviour is coded once, in its owner. Everything above inherits it by composition and never re-implements it.

Components are not all built from scratch: survey existing UI component systems, adopt the pieces that match the contract, and mix-and-match freely across systems.

## How to read this file

Two **ownership levels** sit over the five layers:

- **Level 1: adopted primitives.** It spans layers 1 and 2. These are generic behaviours: overlays, focus, dismissal, inputs, and popovers. The preferred source is a free or open-source component system, picked in the survey and themed to our tokens. Build one ourselves only when nothing available satisfies its contract. Mixing pieces from different systems is allowed and expected.
- **Adoption rule: no requirement compromises.** A survey verdict is provisional. Before an adoption is final, spike the library against every contract rule it must own. If it falls short and can be adapted, the spike proves the adaptation. If it cannot be adapted, fork it, add the capability, and only then adopt the fork. A requirement is never weakened to fit a library.
- **Level 2: our components.** It spans layers 3 to 5. It composes Level 1 and carries everything we have codified. That includes brand, design-system tokens, the §5 behaviours, and density modes. See the Component modes section for the mode list. **This is the vocabulary** design agents see and use. An agent composing a view picks Level 2 components. It does not reach for raw primitives directly.

Layers 1 and 2 carry the behaviours. A composed component needing a behaviour gets it from the primitive. New code never supplies it.

## Layer 0: the placement model

Beneath the components sits a headless model with no React and no DOM: **slots, panes, nav sections, geometry, persistence**. Every shape change in the shell is one action against a single reducer. The nav, the Move control, the region toggles, the command palette, and drag-and-drop cannot each invent their own rules.

Source of truth: `packages/console-model/src/*.ts`, tested with `node --test`. It needs no toolchain and no dependencies. The design lab imports it directly. Nothing re-implements it.

| Module | Owns |
|---|---|
| `types.ts` | `Slot`, `PaneMode`, `SurfaceSpec`, `ShellState`, `Action` |
| `placement.ts` | Where a surface is · which slots it accepts · which nav section that implies |
| `layout.ts` | The reducer: open, close, toggle, move, mode, region, resize, and the collapse rules |
| `geometry.ts` | Breakpoints, rail widths, content-clamped pane widths, the centre's minimum |
| `storage.ts` | The storage **port**: hydrate once, read synchronously, write fire-and-forget |
| `persist.ts` | Versioned payload, merge-on-restore |
| `surfaces.ts` | The registry: label, glyph, width appetite, rail tolerance, home slot, accepted slots |

**Storage is a port.** It is not an API call. Layout must be readable synchronously at first render. The desktop store is async. So the port awaits its backend once at boot. After that, it stays synchronous. Async never reaches the reducer. Each target binds its own backend, and no backend that touches a host API is exported from the package entry point:

| Target | Backend | Why |
|---|---|---|
| Design artifact | memory | Keeps the artifact free of Web Storage, so the preview can serve it from a URL instead of a sandbox |
| Browser | Web Storage (`storage.browser.ts`) | |
| Tauri desktop | `@tauri-apps/plugin-store` | Layout belongs in the OS app-data location. It is not a webview origin |

**Settings are not layout.** Theme lives in `settings.ts`, under its own storage key. It does not live in `ShellState`. Motion is always on. There is no motion setting and no host-preference gate. Earlier policies seeded the theme from the OS and let it toggle at runtime, but `DESIGN.md` §5.1.5 replaces both policies.

## Design system: `packages/console-ui`

| Module | Owns |
|---|---|
| `tokens.ts` | Both palettes as data, and the `[data-theme]` stylesheet generated from them |
| `color.ts` | Converts OKLCh to linear sRGB, then to a WCAG contrast score, so a contrast claim is a test rather than a sentence |
| `overlay.ts` + `overlay.css` | Overlay has two contracts that share one motion lifecycle. The modal contract is the default: React Aria focus containment, a scrim, outside-dismiss, and Escape from anywhere. The other contract, set with `modal={false}`, keeps content live with no scrim and no focus capture. Escape works only from inside it. Placements: left, right, center, top |
| `palette.ts` | CommandPalette is the search-and-run field opened by Ctrl+P or ⌘K. It combines an Overlay at the top with React Aria Autocomplete, SearchField, and ListBox. SearchField is the labelled search box inside it. `usePaletteShortcut` owns the Ctrl+P and ⌘K binding. The palette draws the list that `console-model/src/commands.ts` builds and ranks |
| `popover.ts` | Popover is an anchored, labelled dialog. ChoiceMenu is its single-choice sibling. Both sit on React Aria Popover and Menu. Positioning, flip, focus, and dismissal come from React Aria. Motion comes from the shared tokens, through `data-entering` and `data-exiting` |
| `orb.ts` + `orb.css` | FloatingOrb is a circular control above everything. It drags, flicks, and opens a radial menu on press. The menu is a React Aria Menu on a ring. Submenus form a second ring. Items appear at the orb and radiate outward. `ring="plate"` puts an opaque disc or sector under them. A regular menu, built from React Aria Popover, and the shared non-modal Overlay as a panel, remain as patterns. The host can open the panel from a ring item through `panelOpen`. FloatingOrb is independent of the shell. It takes items and a bounds element, and it returns an id. Physics live in `console-model/src/flick.ts`. Ring placement lives in `console-model/src/radial.ts`. Both are tested in Node. Both were tried in `apps/console-lab/orb.html` |

The modal Overlay keeps its focus scope and backdrop alive until exit motion completes. React Aria owns containment, focus return, background accessibility isolation, Escape, and outside interaction. The adapter owns WAAPI animation and the preview-stage portal boundary. It does not read host motion preferences. The first consumer is the narrow navigation drawer. The second is the command palette. Its commands are intents built from the placement model (`console-model/src/commands.ts`). The view layer only maps an intent to the same API the nav uses. Inspection without a modal, and anchored popovers, need their own contract validation before adoption extends to them. Package dependencies and export paths are declared in `packages/console-ui/package.json`.

Adoption reference: [React Aria Modal documentation](https://react-aria.adobe.com/Modal). The adapter composes three building blocks from that library. ModalOverlay is the modal wrapper. Modal is the positioned surface. Dialog is the focus container. Upstream packages are bundled unmodified. Their license notices stay preserved in the HTML.

**The light theme is derived.** It is not simply inverted. An inverted dark palette gives grey mud and glaring surfaces. Four rules produced the light theme instead. Elevation steps toward the viewer in both themes, but in opposite directions. Text lightness is chosen for contrast against its own background, rather than mirrored from the dark theme. Chroma rises slightly in light, because a tint that reads on a dark surface washes out on a bright one. Shadows in light are a tinted grey, rather than black.

Every pairing we rely on is asserted:

| Text role | Minimum contrast | Where |
|---|---|---|
| Body text | 7:1 | All five surfaces, both themes |
| Secondary text | 4.5:1 | All five surfaces, both themes |
| Faint text and every meaning-carrying colour | 3:1 | All five surfaces, both themes |

**A surface declares, the slot decides.** A `SurfaceSpec` states its width appetite (`min`/`max`), whether it survives rail width (`canRail`), where it starts (`home`), and which slots it will accept (`accepts`). The shell negotiates against those declarations rather than hard-coding placements. `accepts` is permissive today. Every surface takes every slot. That is deliberate, until real views show which placements are wrong.

### One representation per state

A region's **mode** describes its navigation width. The values are `hidden`, `rail`, `nav`, or `expanded`, the last for regions with no navigation of their own. Whether a panel is showing is decided by exactly one thing: whether it is in `panels`.

There is deliberately no mode that hides an open panel. When one existed, it produced two failures at once. A "collapse to rail" control and a "close" control drew the identical picture. Also, the navigation kept reporting a panel as open while nothing was on screen. Navigation width and panel presence are independent. A panel shows beside the rail or beside expanded navigation, and neither hides the other. What stops the left region growing greedy is the centre's minimum (`CENTRE_MIN`). It is not a rule that hides things to make room.

The same principle retired `canRail`. It existed to stop a panel from collapsing into icons. That situation cannot arise once the rail is navigation, and never a panel.

### The boundary rule

The model is framework-free, and stays that way. The test for which side a piece of logic belongs on is one question: **does it need the DOM?**

| Model (`packages/console-model`) | View (React) |
|---|---|
| What exists, where it lives, what is allowed, what persists | Measurement, animation, focus, pointer gestures, portals |
| `move`, `toggleSurface`, collapse rules, `accepts`, restore-merge | FLIP transforms, animated sizing, popover positioning |

Three consequences, in order of how likely they are to be violated:

1. **A drop handler computes a target.** It never decides legality. Drag-and-drop hit-testing is DOM work and belongs in the view. The result is one `dispatch({ type: 'move', id, to })`. `accepts` already decides whether that move is allowed, and it is tested. The moment a drop handler grows its own placement rule, the seam has failed and the tests stop meaning anything.
2. **Layout state must stay serialisable**, because pop-out windows are separate webviews with their own React trees. State held in component state cannot cross that boundary.
3. **Settings are not layout.** Theme is a preference. It is not placement. It lives in a sibling module and does not enter `ShellState`. Chip tabs are the locked treatment. Tab style is no longer a setting.

Adopted behaviour libraries, such as React Aria Components, sit in the view layer. The model never sees them.

## Layer 1: surface primitives (the behaviour carriers)

| Component | Role | Owns contract rule |
|---|---|---|
| **Overlay** | Base for everything that appears on demand: scrim, slide/fade enter and exit, light dismiss, focus capture and return. Light dismiss means an outside click or Esc | 5.1.1, 5.1.3, 5.5.3 |
| **Workspace** (evolves SidePanel) | Workspace is the docking area around the central content. Content-selected or system-raised inspection appears as an Overlay first. The **pin** action docks it, and **unpin** reverses it. Explicit navigation may open a docked panel directly (§5.1.1). Panes host tabs, rearrange, drag-resize, and **pop out** into separate windows. Which panels are open persists per view. Region and navigation widths are global. **Focus mode** collapses every pane to centre-only and restores the exact layout on exit. We own these policies over the placement model. Dockview remains a tiling-engine candidate, not an adopted dependency of the current shell | 5.2.1-5.2.5 |
| **Dialog** | Centred modal on Overlay, for decisions that must interrupt (priority P0) | 5.1.3 |
| **Popover** | Small anchored surface for dropdowns and menus. It flips near edges | 5.1.3 |
| **Toast** | Toast is a transient corner notice that auto-dismisses. It never blocks. Stacking policy is open, tracked as [O]: each instance dismisses on its own, and the group also dismisses in one action | n/a |
| **ScreenHost** | ScreenHost holds the screens. It crossfades on switch. It never swaps instantly | 5.1.2 |
| **MotionViewport** | MotionViewport is a pan and zoom container for a canvas, table, or list. Every programmatic move animates: a zoom button, a focus jump, or a reset. The cursor signals the affordance: grab, grabbing, or pointer. **Text readability floor** (rule 5.7): no label renders below the smallest type-scale step. On zoom-out, a label holds its on-screen size through counter-scale, or it hides in favour of the next zoom-ladder abstraction. It never shrinks into illegibility | 5.1.4, 5.7 |
| **FloatingOrb** | A circular draggable button above everything in the app. It opens a menu shaped by context and viewport. It is built as `packages/console-ui/src/orb.ts`. The ring's entries come from `console-model/src/orbmenu.ts`. They cover views, panels, the selection, and the current view as intents | 5.3.1, 5.3.2 |
| **Breadcrumb** | Shows drill depth. Each level is clickable. The user jumps up many levels in one action | 5.5.2 |
| **ListFilter** | ListFilter is a free-text filter attached to any list with more than 10 items. It runs client-side now. It runs server-side where a backend exists | 5.5.1 |

## Layer 2: input primitives

| Component | Role | Owns contract rule |
|---|---|---|
| **Button** / **IconButton** | IconButton is a button that shows only an icon. Both share all states from `DESIGN.md` §16.1. Both show a visible `:focus-visible` ring. The touch target is at least 44 px | 5.4.1, 5.4.3 |
| **TextInput** | TextInput is a text field with voice input built in. A microphone affordance sits on every instance | 5.4.2 |
| **SearchField** | TextInput plus a shortcut hint and a clear action | 5.4.2, 5.5.1 |
| **Select** | A trigger plus a Popover list. It offers full keyboard and touch operation | 5.4.1 |

## Layer 3: shell components (one instance each)

| Component | Composes |
|---|---|
| **TopBar** | TopBar composes a centred SearchField, plus EnvironmentScope, AttentionIndicator, and ThemeToggle. EnvironmentScope is the environment picker. AttentionIndicator is the attention-count button. ThemeToggle is the light and dark theme switch |
| **ModeRail** | Buttons. Groups. Active state |
| **CommandPalette** | Overlay, SearchField, and ListFilter. Opens with Ctrl+P or ⌘K |
| **ThemeToggle** | IconButton |
| **EnvironmentScope** | Select |
| **AttentionIndicator** | Button + Popover |
| **CriticalBanner** | A persistent strip. It is not an overlay |
| **AmbientAssistant** | AmbientAssistant combines FloatingOrb, a Popover menu, and TextInput. It is voice-first |
| **ZoomControl** | ZoomControl is a set of icon buttons, called IconButtons, plus a readout, docked in MotionViewport |

## Layer 4: product components

Each one composes layers 1 to 3. The "hosts" column names the primitive that gives it its behaviour.

| Component | Hosts / composes | Notes |
|---|---|---|
| WorkItemNode / WorkItemCard | MotionViewport (node) · lists (card) | WorkItemNode is the canvas form. WorkItemCard is the list form. Both share one state grammar |
| AgentPin | WorkItemNode, WorkItemCard | AgentPin is a subordinate agent, bound to work |
| StatusLine | TopBar | StatusLine shows a health summary and a time scope |
| FocusInspector | **Workspace** panel | Entity header, evidence rows, actions |
| FocusHeader | Inside FocusInspector, feeds Breadcrumb | FocusHeader shows the entity and the focus path. The path runs from intent, to spec, to item |
| RecorderTimeline | inside FocusInspector | RecorderTimeline lets the user drill from phase, to event, to run. It has a replay scrubber |
| EvidenceTier | inside FocusInspector | EvidenceTier shows raw wells, mono text, and provenance lines |
| ReviewPanel | ScreenHost screen or pinned Workspace panel | ReviewPanel is the surface for deciding between the diff and the intent |
| AttentionStrip / AttentionItem | ListFilter when > 10 | AttentionStrip lists AttentionItem rows, ranked, each with a one-line reason and an open action |
| GateBadge / GateRunRow | Popover for history | GateBadge shows gate status. GateRunRow lists one run each |
| DeploymentPill | Popover for detail | DeploymentPill shows one deployment, with detail in a Popover |
| RunwaySummary | ScreenHost screen | RunwaySummary gives a verbal verdict. It never shows one bare number |
| CauseChip / SeverityBanner / ProvenanceChip / ConfidenceTag | inline | These are small inline labels, for cause, severity, source, and confidence |
| QueueMeter / BottleneckMarker / ClusterBadge | MotionViewport | These are canvas marks, for queue size, bottlenecks, and clusters |
| EmptyCalmState / StaleIndicator | ScreenHost screens | EmptyCalmState is the healthy empty view. StaleIndicator is the disconnected view |
| TimeControl / Scrubber | TopBar | TimeControl switches between live and replay. It is parked. See `docs/design/design-tracker.md`, question Q6 |

Superseded names from the old inventory: **DockPanel** was an early panel type. **DockManager** was its controller. **SidePanel** was its side dock. All three are now Workspace, in layer 1. **DynamicSurface**, an earlier name for the same idea, is now StreamSurface, the streaming-view container, in layer 5. **LensSwitcher**, an earlier control for switching lenses, is now the lens items in ModeRail, the left navigation rail. Lenses live only in the left nav. **CommandPalette**, the search-and-run field, keeps its old name. It is the layer 3 component described above.

## Layer 5: generated and streaming UI

| Component | Role | Owns contract rule |
|---|---|---|
| **StreamSurface** | Container for agent-generated views, such as the A2UI, AG-UI, or JSON-render class. Renders progressively as parts arrive. Each part is a layer 1-4 component | 5.6.1 |
| **PresetSaver** | "Save this view as a preset" on every StreamSurface; reopen without re-prompting | 5.6.2 |

## Component modes (Level-2 contract)

Every Level-2 component supports density modes: **full · medium · compact**. A component may support a subset, as its spec declares.

| Mode | Typical slot | What it shows |
|---|---|---|
| **full** | Desktop centre content / canvas | Complete information, all interactions |
| **medium** | Side panel, split view, tablet pane | Trims secondary detail; keeps state and actions |
| **compact** | Mobile screen, list row, overview tile | Identity + state + one primary action; the rest behind a tap into a fuller mode |

Rules:

1. **The slot picks the mode.** The component itself does not. The same component renders full in the centre canvas, medium in a right panel, and compact on a phone. The mechanism is a container query, or an explicit `mode` set by the host slot. It is never the global viewport alone, because a desktop side panel can be as narrow as a phone.
2. **What each mode drops** is a design decision, written in the component's spec. The order follows the attention rank (`DESIGN.md` §2.1), rather than whatever happens to fit.
3. A mode change animates (rule 5.1). It never changes the component's meaning or state grammar, only its density.
4. A compact instance always offers a route to its fuller self. It can expand, or open in a panel.
5. **StreamSurface depends on this**: an agent composing a view picks Level-2 components. The target slot then dictates each one's mode. Without modes, generated layouts break the moment they land in a narrow slot.

FocusInspector is the worked example. Its full mode is the centre review surface, with the recorder, evidence tiers, and actions all open. Its medium mode is the side-panel form. Its compact mode is a mobile bottom sheet with a header, state, cause, and one primary action. The canvas zoom ladder (`DESIGN.md` §10.6) already sets this precedent. It applies the same contract to WorkItemNode, the canvas form of a work item.

## Build order

Sequencing and status moved to `design-tracker.md`, under "Next steps", on 11 September 2026. This file stays design guidance about which component owns which rule.

### Naming note: the Ctrl+P surface

It is a **command palette**. VS Code splits the idea in two: *Quick Open* (Ctrl+P) finds a thing, *Command Palette* (Ctrl+Shift+P) runs a thing. Ours does both from one field, so it keeps the single name `CommandPalette` and the single shortcut. It searches work items, runs, and evidence. It also carries commands, including opening a surface into a chosen slot. That makes it the keyboard twin of the Move control.

Build sequencing state and prototype status live in `design-tracker.md`.
