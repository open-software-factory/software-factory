# Survey 03: canvas and graph libraries for MotionViewport and the node graph

This survey tracks two things. It tracks the pan-zoom canvas primitive, `MotionViewport`,
which is Layer 1 in `components.md`. It also tracks the node-link graph that canvas hosts,
used by Factory Floor and Work Graphs.

The survey checks each candidate against four `DESIGN.md` rules.

- 5.1.4, animated programmatic moves
- 5.7, the text readability floor
- 10.6, the semantic zoom ladder
- 10.8, deterministic layered layout, stable anchoring, and morph on lens switch

## Scoring table

Score legend:

- 5, built in and ready to use
- 3, possible with moderate custom code
- 1, needs the feature built from scratch

The "fit for 30 to 500 elements" columns give general context. They are not benchmarks. This
survey does not test graphs at 10,000-node scale, because the app never reaches it.

| Library | 1. Screen-space and zoom-threshold labels (5.7) | 2. Animated viewport moves (5.1.4) | 3. Semantic zoom and LOD by level | 4. Custom node rendering freedom | 5. Pluggable DAG layout and morph | 6. Perf at 100 to 500 elements | 7. Framework dependency, licence, and 2026 health |
|---|---|---|---|---|---|---|---|
| **Cytoscape.js** | **5**. `min-zoomed-font-size` hides a label below a pixel floor natively. `text-opacity` handles the fade | 5. `cy.animate({zoom,pan}, {duration,easing})` | 3. Compound nodes plus the `cytoscape.js-expand-collapse` extension give cluster-to-item collapse. Chip-level LOD is custom | 3. Canvas-drawn shapes via style functions. An HTML overlay needs the `cytoscape-node-html-label` extension | 4. The official `cytoscape.js-elk` adapter. Layout re-run animates by default | 5. Canvas renderer, built for graphs, fine at this scale | 5. Framework-agnostic, MIT, active in 2026. Its repo had commits through August 2026 |
| **AntV G6 (v5.1.x)** | 2. No built-in font-size floor. You gate label visibility yourself off the zoom ratio | 4. `graph.zoomTo()` and `fitView()` take easing and duration | 3. Built-in Combo (cluster) nodes with expand and collapse. Level swap by zoom is custom | 5. Native React-node support since version 5.0, plus Canvas/SVG/WebGL renderers | 4. Bundled layouts (dagre-like, force, combo). Some are now Rust-compiled for speed. ELK is pluggable | 5. Rust layout core, comfortable well past our scale | 4. MIT, AntV-backed, active. Version 5.1.1 shipped about four months before this survey. Framework-agnostic core, React idiomatic |
| **xyflow, React Flow and Svelte Flow** | 2. No built-in floor. HTML nodes scale with the pane's CSS transform, so you must read zoom via `useViewport`/`useStore` and counter-scale or hide manually | 4. `fitView({duration})`, `zoomTo`, and `setViewport` animate. `fitView` duration has had rough edges in some releases (open issues) | 2. No representation-swap primitive. Build it off `onMove`/zoom state | 5. Nodes are arbitrary React/Svelte components. This is best-in-class for our glyph grammar in HTML/SVG | 4. No built-in layout. Wire up `dagre`, `elkjs`, or `d3-dag` yourself, a documented pattern for all three. Re-layout can be animated by tweening positions | 4. DOM nodes, fine at our scale. Would need virtualization well before 1,000+ nodes | 5. MIT core, covering React Flow and Svelte Flow. The paid Pro tier covers examples and templates only, so it is not a licence gate on the library itself. Actively maintained into 2026 |
| **Sigma.js (v3)** | **5**. `labelRenderedSizeThreshold` hides a label below a pixel size. `labelDensity` and `labelGridCellSize` pick the "worthiest" labels per frame | 3. Camera has `animate()` with duration and easing, lower-level than the others | 2. No representation-swap primitive. Graphology plus custom reducers are needed | 2. WebGL node and edge "programs". Custom glyphs mean writing shader-like render programs instead of HTML or SVG | 2. No bundled layout. Pair with `graphology-layout`, or run ELK or d3-dag externally and feed positions in | 5. WebGL, built for large graphs, well past our scale | 4. MIT, framework-agnostic. A React wrapper ships via `@react-sigma/core`. Active 2026 development, with v3 alpha label-grid work ongoing |
| **D3 + custom SVG/Canvas** | 3. You own the label-floor logic entirely. It is straightforward to implement correctly, by measuring rendered size and hiding or counter-scaling, but none of it is free | 3. `d3-zoom` plus `d3-transition` give the primitives. You wire `zoomTo`/`fitView` yourself | 3. You own it. This is the "roll your own semantic zoom" path, which is well documented for D3 but is still bespoke code | 5. Full control, SVG or canvas, matches the "canvas draws its own monoline glyph set" instruction in DESIGN.md 10.2 exactly | 3. No bundled layout. Pair with `d3-dag`, `dagre`, or ELK. You own the morph and tween | 4. SVG struggles past roughly 1,000 to 2,000 DOM nodes. Canvas is fine. Our scale is comfortable either way | 5. No framework dependency, BSD/ISC family licences. D3 itself is stable and steadily maintained |
| **PixiJS-based (custom)** | 3. Same as D3, you own it. WebGL text objects can live outside the world-space scale, by not applying camera scale to the label sprite, a clean mechanism once built | 4. `pixi-viewport` ships eased `animate`, `snapZoom`, and `follow` helpers | 3. You own the representation swap. WebGL gives headroom to swap whole node "looks" cheaply | 4. Full custom draw. Glyphs are canvas or WebGL primitives, or pre-rendered sprites, instead of HTML or SVG. This needs extra tooling to keep the glyph sheet in sync | 3. No bundled layout, the same external-layout story as D3 | 5. WebGL, most performance headroom, overkill for 30 to 500 elements | 4. No framework dependency, MIT. PixiJS is well-maintained into 2026, with the highest engineering cost of any option here |
| **tldraw SDK (infinite-canvas)** | 1. It is a whiteboard engine, covering shapes, arrows, and freehand drawing. It has no graph label-floor concept | 5. Camera animation, such as `editor.zoomToBounds` and `animateToShape`, is a core, polished feature | 1. No semantic-zoom or LOD concept. It is a drawing surface, without a data-graph renderer | 4. Custom shape and tool API is flexible, but arrows are hand-drawn connectors, without a graph edge model | 1. No DAG layout concept. You would still need dagre, ELK, or d3-dag, and would be fighting the shape model to apply it | 4. Fine at our scale, but it solves a different problem | 3. React-only. **The watermark shows unless licensed.** The free "hobby" licence keeps the "made with tldraw" mark. A paid commercial licence is required to remove it in production. Actively maintained in 2026 |

## Narrative

**Cytoscape.js** is the sharpest fit for rule 5.7, the text readability floor.
`min-zoomed-font-size` is a one-line style property. It hides a label once it would render
below a floor size, with no custom measurement code needed.

It also ships `cy.animate()` for eased pan and zoom, covering rule 5.1.4. Compound-node expand
and collapse is a real building block for the cluster-to-item swap in rule 10.6.

Its weakness is node rendering. Shapes are drawn through Cytoscape's own canvas style system.
A rich HTML glyph, such as counters or progress bars for `DESIGN.md` 10.2's subtask candidates,
needs an overlay extension instead of being native.

Cytoscape.js pairs cleanly with **ELK**, via the official `cytoscape.js-elk` adapter, for
deterministic layered DAG layout.

**AntV G6 5.x** is the strongest all-round dark horse. It is the only library here with native
React-component nodes, a real cluster and combo model, and an actively modernised layout core.
The layout core is partly written in Rust. All of this ships under MIT, in 2026.

It does not have Cytoscape's or Sigma's built-in label-floor primitive, so rule 5.7 still needs
custom zoom-gated logic. Everything else on the list is closer to adopt than to build.

**React Flow**, from the xyflow project, and **Svelte Flow**, its Svelte-framework equivalent,
give the best raw authoring experience for our exact glyph grammar. Nodes are just React or
Svelte components, so we can use CSS or SVG however `DESIGN.md` 10.2 needs, with no adapter layer.

The cost is that neither the label floor, rule 5.7, nor semantic zoom, rule 10.6, exist out of
the box. Both must be hand-built, by reading the live zoom level and swapping or counter-scaling
the view. This pattern is well documented already in the xyflow ecosystem. It is still work the
other two candidates do not require.

The core library is fully MIT. The paid "Pro" tier covers examples and templates only, so it
does not gate the licence.

**Sigma.js**, a graph-drawing library, matches Cytoscape on label-floor quality, through
`labelRenderedSizeThreshold` and label density or grid picking. It is the most proven option here
for graph scale headroom. It renders through WebGL, a browser graphics API, instead of HTML or
SVG, though. Custom node "programs" are lower-level than markup, which fights the `DESIGN.md`
instruction that node glyphs should be an easily iterated monoline set. This library suits us
better if the graph ever needs to scale an order of magnitude past our 30-to-500 target.

**D3 with custom SVG or canvas**, and **PixiJS-based custom builds**, are the "we own every
pixel" options. Both can satisfy every rule in this brief exactly as written, because you write
the rule's logic yourself. The cost is building the label floor, semantic zoom, viewport
animation, and layout wiring from primitives, instead of adopting them.

`components.md` states a preference. It says to adopt the pieces that match the contract, and to
build one only when nothing else satisfies it. This path is therefore a fallback. Use it only if
Cytoscape or AntV G6, the two leading candidates above, turn out to fight the glyph grammar
during prototyping.

**tldraw SDK** solves a different problem. It is a freeform whiteboard and infinite canvas,
rather than a structured data graph. It scores lowest here on purpose. It has no DAG layout, no
semantic zoom, and a watermark in production without a paid licence.

It is not a fit for the node graph. We do not evaluate it further for MotionViewport either.
MotionViewport's job is to host a data-driven graph, and tldraw's job is freehand drawing.

**Layout engines, scored separately:**

| Engine | Status 2026 | Notes |
|---|---|---|
| **dagre** | Unmaintained since about 2018. Issues are still filed, but no fixes are landing | Widely used, it is the default in most React Flow layout examples, but it is a maintenance risk for new work |
| **elkjs** | Actively maintained. `kieler/elkjs` has had commits through 2026 | Sugiyama-style layered layout, with many algorithm options. Runs off-main-thread via a web worker. About 500 KB, GWT-compiled Java, the heaviest of the three |
| **d3-dag** | Actively maintained. Its npm package was updated in July 2026 | Multiple layering and crossing-minimisation strategies, including an optimal-crossing mode. Ships a dagre-compatible API for easy migration. Far smaller bundle than elkjs |

For our 30-to-100 node scale, bundle size differences between elkjs and d3-dag do not matter
operationally. The deciding factor is maintenance. Both elkjs and d3-dag are maintained, while
dagre is not maintained.

## Recommendation

**Primary: Cytoscape.js with ELK**, via `cytoscape.js-elk`. This is the only pairing here where
the 5.7 label floor is a configuration value instead of custom-built work. It also already has a
cluster and expand-collapse primitive, letting us build the 10.6 zoom ladder on top of it. Accept
the extra work needed for rich glyphs styled like HTML, such as progress bars and counts, via an
overlay extension.

**Strong alternative:** AntV G6 5.x with its bundled layout, or with ELK. Pick this if native
React-component nodes, matching how the rest of the console is built, outweigh Cytoscape's
label-floor convenience. In that case, budget explicit engineering time for the 5.7 zoom-gated
label logic, since G6 does not give it for free.

**Fallback.** If prototyping shows either candidate fighting the glyph grammar, switch to D3
with canvas. SVG is fine too, for up to 500 nodes. Pair it with d3-dag for layout. This path
means full control, more code, and no framework lock-in.

**Avoid for this track.**

- dagre alone. It is unmaintained. Pair it with ELK or d3-dag instead, or use it only behind the
  documented example for React Flow, from the xyflow project, as a stopgap.
- tldraw SDK. It solves the wrong problem, a whiteboard rather than a data graph, and it adds
  watermark and licence friction.

## Sources

- [xyflow, React Flow and Svelte Flow, on GitHub](https://github.com/xyflow/xyflow)
- [React Flow Pro Pricing](https://reactflow.dev/pro/pricing)
- [FitViewOptions in the React Flow docs](https://reactflow.dev/api-reference/types/fit-view-options)
- [ReactFlowInstance in the React Flow docs](https://reactflow.dev/api-reference/types/react-flow-instance)
- [React Flow issue 4803 on GitHub, the fitView duration](https://github.com/xyflow/xyflow/issues/4803)
- [Dagre Tree layout example in React Flow](https://reactflow.dev/examples/layout/dagre)
- [Sigma.js official site](https://www.sigmajs.org/)
- [Sigma.js on GitHub](https://github.com/jacomyal/sigma.js/)
- [Sigma.js issue 495 on GitHub, the label threshold](https://github.com/jacomyal/sigma.js/issues/495)
- [Sigma.js customization docs, label density and grid](https://www.sigmajs.org/docs/advanced/customization/)
- [Cytoscape.js official site](https://js.cytoscape.org/)
- [Cytoscape.js style docs, `min-zoomed-font-size` and `text-opacity`](https://github.com/cytoscape/cytoscape.js/blob/master/documentation/md/style.md)
- [cytoscape.js-elk adapter on GitHub](https://github.com/cytoscape/cytoscape.js-elk)
- [react-cytoscapejs MIT licence](https://github.com/plotly/react-cytoscapejs/blob/master/LICENSE)
- [AntV G6 on GitHub](https://github.com/antvis/g6)
- [G6 official site](https://g6.antv.antgroup.com/en)
- [G6 5.0 announcement on Medium](https://yanyanwang93.medium.com/g6-5-0-a-professional-and-elegant-graph-visualization-engine-11bba453ff4d)
- [G6 ZoomCanvas behavior docs](https://g6.antv.antgroup.com/en/manual/behavior/zoom-canvas)
- [tldraw, get a licence](https://tldraw.dev/get-a-license/hobby)
- [tldraw pricing and licensing](https://tldraw.dev/pricing)
- [tldraw license key docs](https://tldraw.dev/sdk-features/license-key)
- [dagrejs/dagre on GitHub](https://github.com/dagrejs/dagre)
- [Dagre issue 318 on GitHub, a Dagre alternative](https://github.com/dagrejs/dagre/issues/318)
- [kieler/elkjs on GitHub](https://github.com/kieler/elkjs)
- [d3-dag on GitHub](https://github.com/erikbrinkman/d3-dag)
- [d3-dag on npm](https://www.npmjs.com/package/d3-dag)
