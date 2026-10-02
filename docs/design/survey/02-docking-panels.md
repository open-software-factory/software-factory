# Survey 02: docking and panel-layout systems for SidePanel

SidePanel is the panel primitive that starts as an overlay, then pins into dock. Scope: `DESIGN.md` §5.2 and §4.2. `DESIGN.md` is the product design specification. SidePanel must be reversible, closeable, and resizeable, and its per-view state must persist. The dock zone is 360 to 420 px wide, and it resizes to half the viewport. The owner component is `components.md` Layer 1 SidePanel.

## Honest framing first

Each library surveyed here is a **docking-layout engine**. It models an app that starts already tiled into panes and tabs, and lets the user drag panes into new arrangements. None of them models our actual lifecycle. A panel starts as a **floating overlay**. The user **pins** it. Only then does it become a layout participant. That state, unpinned but visible and overlaying content, is not a first-class concept in any candidate. Adopting one means fighting its state machine to bolt on a phase it doesn't have. It also means paying for features we don't need, such as multi-tab groups, drag-to-any-edge grids, and popout browser windows.

## Scored table

Scale: Good, Partial, or Poor. OKLCH is a perceptually uniform colour format. The theming-fit column checks how well each candidate supports it.

| Candidate | Overlay-first and pin | Animated dock and resize | State serialise and restore | OKLCH theming fit | Framework dependency and weight | Licence and 2026 health |
|---|---|---|---|---|---|---|
| **Dockview**, a docking-layout library | Poor. It is a dock-first grid. Floating panels exist, but they are not "pin from overlay" | Partial. Resize is live, but dock transitions are not eased | Good. It has built-in `toJSON` and `fromJSON` methods | Good. CSS custom property themes, override-friendly | Framework-agnostic core, with React, Vue, and Angular bindings. It is the heaviest of the set, a full IDE grid engine | MIT licence for the core. Version 8.2.0 released about two weeks before this survey. Actively maintained |
| **Golden Layout** | Poor. Same dock-first model, the oldest of the set | Poor. No built-in easing, an older DOM approach | Good. Serialisation is its signature feature | Partial. It is CSS and LESS based, less native to CSS variable theming | Vanilla core, framework wrappers exist. Medium weight | MIT licence. Version 2 is active, but the roadmap says future work lands on an unstable version 3 development branch. Plan around breakage |
| **FlexLayout**, from Caplin | Poor. Dock-first tabsets, popout windows instead of pin | Partial. Resize is live, but no eased dock animation is documented | Good. The layout model serialises and restores | Good. It has explicit `--flexlayout-*` CSS variables, easy to remap to our tokens | React only, a single peer dependency, light | ISC licence, very active. Patch releases within days |
| **Lumino**, used by JupyterLab | Poor. Widget-based dock area, the same approach | Partial. It is functional, but it is not animation forward | Good. Jupyter relies on this for workspace restore | Poor. It is widget and class based CSS, needing more retrofitting to reach OKLCH tokens | Framework-agnostic, but it is a widget model. It is not idiomatic in React, Vue, or Svelte. This is a real integration tax | BSD-3 licence, actively maintained, with patch releases. Backed by the Jupyter organisation |
| **rc-dock** | Poor | Partial | Good, a JSON layout | Partial. Dated CSS | React only | MIT licence, but **stalled**. The latest release is an alpha from about eight months back, with a small download count. Avoid |
| **react-mosaic** | Poor. It is a tiling window manager, with binary or n-ary splits, rather than a pin-and-dock model | Partial. Resize is live | Partial. The tree serialises, but it is bespoke to its split model | Partial. It is SCSS based, themeable with work | React only | Apache 2.0 licence, active. Version 7 beta, from March 2026, added tabs, an n-ary tree, and React 19 support |
| **react-resizable-panels** | Not applicable. This is a pure resizable split-pane primitive rather than a docking library | Good. Resize itself is smooth, with no dock or undock concept to animate | Good. The `autoSaveId` option persists sizes to storage out of the box | Good. Unstyled, trivial to theme | React only, very light | MIT licence, extremely active, with days-old releases. Called the 2026 default resize primitive |
| **Allotment** | Not applicable. Same category as react-resizable-panels, VS Code-derived split panes | Good | Partial. Sizing only, no pin state | Good. Unstyled | React only, light | MIT licence, active. Latest release about March 2026 |
| **paneforge** | Not applicable. A Svelte port of the react-resizable-panels idea | Good | Good | Good | Svelte only, light | MIT licence, active, community maintained |

## Narrative

**Dockview** is the strongest general docking engine on the list. It has a framework-agnostic core, and it is genuinely active in 2026. It is MIT licensed. It is the only one with real CSS custom property theming, plus floating panels and popout windows. But it is sized for a multi-pane IDE workspace, such as Lab's future investigative surface with many panes, rather than one right-edge panel. Using it for SidePanel means importing a grid, tabs, and split-view engine to get drag-resize and persistence for a single panel.

**Golden Layout** pioneered layout serialisation. Its version 3 roadmap explicitly says new work is unstable. Picking it up now means designing against a moving target.

**FlexLayout**, a docking library from Caplin, has the best out-of-the-box theming story, with clean `--flexlayout-*` variable names. It is genuinely lightweight for a React-only project. But it is React only. This is a real constraint here, since the framework is not locked. `DESIGN.md` §4.1 only commits to Tauri and a canvas-based core.

**Lumino** is battle-tested. It is the docking system built for JupyterLab, a data-science notebook application. But its widget-class model is the furthest from any modern component framework. Adopting it costs an integration layer, no matter what framework wins.

**rc-dock** is a real disqualifier case. It is alpha-stalled, with a tiny download count, so do not adopt it.

**react-mosaic** is healthy, and it is actively evolving toward tabs in 2026. But it solves tiling splits. That is a different problem from "one panel that floats, then docks."

**react-resizable-panels**, **Allotment**, and **paneforge** are not docking libraries. They are exactly the primitive our SidePanel actually needs for the "resizeable, persists size" half of the contract, in whichever framework wins. None of them touch pin, unpin, or overlay-versus-docked state. That is not their job.

## Verdict: build SidePanel ourselves, on a resize primitive

Adopting a full docking library for one SidePanel primitive is not worth it. The overlay-first-then-pin lifecycle, `DESIGN.md` rule 5.2.1, is not what any docking engine models. Every candidate needs a custom wrapper regardless. That wrapper would spend most of its effort suppressing features we don't want, such as tabs, multi-pane grids, and popout windows, rather than using them.

Recommended shape:

1. **Overlay phase.** SidePanel mounts on our own Layer 1 `Overlay` primitive, already scoped in `components.md`. It slides and fades using the existing motion tokens, §16.5. No library is needed. This is already our contract.
2. **Resize.** While overlaying or once docked, delegate drag-resize to a small, unopinionated primitive. Use `react-resizable-panels`, or Allotment, if React wins. Use `paneforge` if Svelte wins. Otherwise, hand-roll the same interaction on pointer events. These libraries are thin enough that the framework choice barely matters to weight or risk.
3. **Pin and unpin transition.** This is a state toggle we own. Unpinned means a `position: fixed` overlay above content. Pinned means a flex or grid child inside the content's `PanelGroup`. Animate the swap with a FLIP-style transform, a first-last-invert-play technique, or with a `grid-template-columns` transition using `--dur-panel`, 200 ms, §16.5. This meets the "layout resizes smoothly to make room" requirement without adopting a docking engine's animation model. None of them do that well anyway, as the table shows.
4. **Persistence.** This is a thin store write, side, size, and pinned state, per view, keyed in user preferences. None of the libraries' serialisation formats fit a single panel's three fields. Writing it ourselves is a handful of lines rather than a dependency.

**Revisit this call.** This decision can change later. The SidePanel component contract does not need to change with it. If a second primitive later needs true multi-pane tiling, that is the moment to bring in Dockview. Examples include a Lab surface with many arrangeable panes, or ReviewPanel growing tabs. It is MIT licensed, framework-agnostic, actively maintained, and it has the best theming story of the set. That is a different component with a different contract. It should not retroactively pull SidePanel onto a docking engine it doesn't need.

## Sources

- https://dockview.dev/
- https://github.com/dockview/dockview
- https://www.npmjs.com/package/dockview
- https://dockview.dev/docs/core/theming/
- https://github.com/golden-layout/golden-layout
- https://github.com/golden-layout/golden-layout/releases
- https://golden-layout.github.io/golden-layout/version-2/
- https://github.com/caplin/FlexLayout
- https://www.npmjs.com/package/flexlayout-react
- https://github.com/caplin/FlexLayout/blob/master/README.md
- https://github.com/jupyterlab/lumino
- https://github.com/jupyterlab/lumino/releases
- https://snyk.io/advisor/npm-package/rc-dock@3.0.13
- https://libraries.io/npm/rc-dock
- https://github.com/ticlo/rc-dock
- https://github.com/nomcopter/react-mosaic
- https://github.com/nomcopter/react-mosaic/discussions/241
- https://www.npmjs.com/package/react-resizable-panels
- https://github.com/bvaughn/react-resizable-panels
- https://react-resizable-panels.vercel.app/
- https://github.com/johnwalley/allotment
- https://www.npmjs.com/package/allotment/v/1.17.1
- https://github.com/svecosystem/paneforge
- https://paneforge.com/docs
- https://portalzine.de/docker-layouts-with-goldenlayout/
