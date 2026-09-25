# Component and framework survey: synthesis

2026-09-03.

Five tracks are complete. Detail and sources are in files `01` to `05` in this folder. This file maps the recommendations onto the build order. The build order lives in `design-tracker.md`, under "Next steps". This file also names the one decision the survey cannot make for us.

**Later tracks:**

- `06-workspace-spike.md`: the Dockview spike. Dockview is a panel-docking layout library.
- `07-floating-orb.md`, dated 2026-09-12: the floating-orb track. It covers a draggable orb, radial menus, and flick physics. The verdict is to adopt nothing. Instead, compose the orb from React Aria Components, an unstyled accessible UI library, and a tested physics model.

**Current decision note, 2026-09-11:** The team settled on React and TypeScript, with Tauri for the desktop shell, on 2026-09-10. Step 1, Overlay, uses React Aria Components. Dockview remains a candidate alongside the custom shell. Final adoption is not implied by the historical survey verdict in the table that follows. Canvas-engine adoption is on hold for the infinite-canvas concept, item Q17. The comparisons that follow keep their survey-time context.

## Verdict per build-order step

| Step | Was | Now | Basis |
|---|---|---|---|
| 1. Overlay (+ Dialog, Toast) | build | **adopt and theme.** Use React Aria Components if React is locked. Use Zag.js via Ark UI to stay framework-open. Zag.js is a framework-agnostic state-machine library for UI behaviour, and Ark UI is its component layer. Floating UI, a positioning engine, sits under Popover either way | 01 |
| 2. SidePanel, the panel primitive, now Workspace | build | **adopt Dockview.** Verdict revised 2026-09-04. The requirement widened from one side panel to a full workspace. It now needs tabbed panel groups, left, right and bottom panes around a tabbed centre, pop-out windows, and focus mode. This is exactly Dockview's model. Track 02's "build it" answer applied to the old, narrower requirement. We own the overlay-to-pin lifecycle, focus mode, persistence policy and theming. Verify animated layout transitions, rule 5.1, and pop-out inside Tauri | 02 + revision |
| 3a. ScreenHost, the screen-crossfade primitive | build | **build.** Nothing surveyed covers screen crossfade | 01 |
| 3b. MotionViewport, the pan-zoom canvas primitive | build | **adopt and wrap.** Use Cytoscape.js, a graph-visualisation library, with ELK, a layout engine, for layout. Cytoscape's `min-zoomed-font-size` setting implements the §5.7 label floor with zero custom code. It gives native eased pan and zoom. Compound nodes map to the §10.6 zoom ladder. Alternative: AntV G6 5.x, a graph library where the label floor becomes custom code. Avoid dagre, an unmaintained layout engine, and the tldraw SDK, a whiteboard engine with licence friction. React Flow, a React graph library, did not win. See `03-canvas-graph.md` for detail | 03 |
| 4. Popover + Select | build | **adopt.** Use the same library as step 1, Overlay | 01 |
| 5. ListFilter, a filter primitive, and Breadcrumb, a navigation primitive | build | **build.** They are thin, and they are ours | 01 |
| 6. VoiceInputAdapter, the voice-input primitive | build | **build the adapter, adopt the engines.** TextInput's mic defaults, the text-entry primitive's defaults, are fully local. Web Speech runs on-device on Windows, under WebView2, the Windows web-rendering engine. whisper.cpp runs as a sidecar on macOS. Assistant voice defaults to cloud streaming speech-to-text. Cloud is opt-in for inputs, and it is never the default. One adapter interface fronts both engines | 05 |
| 7. StreamSurface and PresetSaver, the streamed-UI and preset primitives | build | **build on standards.** Use AG-UI, an agent-to-UI protocol, as transport. Use an A2UI-shaped declarative schema restricted to our Level-2 catalogue: a flat component list, stable IDs, and structure and data sent as separate messages. This separation is what makes "arrived parts never re-layout" free. Prototype with json-render first. Reject MCP Apps' iframe and arbitrary-HTML model for StreamSurface, since it contradicts §6.4. Keep that model only as a possible later path for embedding third-party tool UIs. PresetSaver stores a structure spec and a data query separately. Reopening it re-runs the query against the same spec | 04 |

## Cross-track observations

Three of five tracks are framework-agnostic in their winners. Cytoscape is vanilla JS. AG-UI is protocol-level. The voice engines are platform-level. The docking verdict is framework-neutral too. Step 1, Overlay, is the only step that forks on the framework choice.

2026 ecosystem shifts recorded: Radix's pace has slowed, and shadcn now defaults to Base UI. Radix and Base UI are React component libraries, and shadcn is a component-distribution tool built on them. dagre is dead. Use ELK or d3-dag instead. A2UI is real but pre-1.0. The W3C generative-UI group is spec-ware, meaning it has produced a draft specification but no working implementation.

The behaviours that define this product are pin-to-dock, density modes, the label floor policy, and constrained generated UI. This holds across all tracks. These are exactly the behaviours no library ships. The catalogue's split between Level 1, generic behaviour, and Level 2, product opinions, held up. We adopt generic behaviour, and we own the opinions.

## The decision the survey can't make

**The frontend framework, resolved 2026-09-10.** The choice is React and TypeScript, with a desktop shell via Tauri. Step 1, Overlay, uses React Aria Components. Ark UI was the framework-open alternative, and it no longer blocks the build.
