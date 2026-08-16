# Software Factory UX references

These references are not visual templates to copy. Each solves a particular operator-cognition problem that is relevant to the Software Factory.

## 1. Factorio

Why it matters: Factorio makes flow, starvation, congestion, throughput, production, consumption and bottlenecks visible across a large autonomous system.

Study:

- production and consumption views,
- visible machine state,
- starved versus blocked versus productive systems,
- buffer/inventory concepts,
- throughput over time,
- zoom from whole factory to local machinery,
- how a complex system remains understandable without turning into a dashboard of counters.

Translate conceptually:

| Factorio concept | Software Factory analogue |
|---|---|
| assembler | agent/run/execution cell |
| recipe | workflow/skill |
| belt | work/event flow |
| starvation | insufficient ready work/context/dependency |
| backed-up output | downstream gate/review/deployment bottleneck |
| items per minute | accepted work throughput |
| power usage | tokens/compute/cost |
| production graph | factory throughput |

Do not reproduce conveyor belts or game visuals unless they prove useful.

## 2. Temporal Web UI

Why it matters: Temporal supports deep inspection of long-running workflows and event history.

Study:

- event-history timeline,
- semantic event grouping,
- live updating history,
- filtering to failures/pending/retries,
- drill-down to event detail,
- retaining workflow context while inspecting one event.

Apply primarily to the Work Item Flight Recorder.

Show semantic events first. Raw execution transcripts are evidence beneath them.

Reference:

- https://temporal.io/
- https://temporal.io/changelog/updated-event-history-timeline-view-is-now-available

## 3. Honeycomb

Why it matters: Honeycomb supports exploratory observability and investigation beyond predefined dashboards.

Study:

- BubbleUp,
- selecting an anomalous population,
- automatically discovering dimensions that distinguish it from baseline,
- high-cardinality exploration,
- rapid pivots during investigation,
- comparison of populations rather than only aggregate metrics.

Apply primarily to the Meta-loop / Factory Intelligence surface.

The factory should help discover questions we did not know to encode as dashboards.

Reference:

- https://www.honeycomb.io/
- https://docs.honeycomb.io/investigate/analyze/identify-outliers/

## 4. Dagster

Why it matters: Dagster provides navigable dependency and lineage graphs with graph-to-detail interaction.

Study:

- asset graph,
- dependency direction,
- graph clustering/grouping,
- state on nodes,
- selection + detail while preserving graph context,
- semantic zoom,
- lineage traversal.

Apply primarily to:

- Work Graphs,
- execution topology,
- traceability graph,
- parts of the Factory Floor.

Reference:

- https://dagster.io/
- https://docs.dagster.io/

## 5. Argo Workflows

Why it matters: Argo Workflows shows DAG execution status, workflow topology and artifact-oriented execution.

Study:

- live DAG state,
- node status,
- dependencies,
- artifacts,
- nested workflows,
- execution progression.

Reference:

- https://argo-workflows.readthedocs.io/

## 6. Linear

Linear is an issue tracker for software teams.

Why it matters: it shows how dense professional interaction design can avoid visual clutter.

Study:

- keyboard-first interaction,
- command palette,
- fast navigation,
- split/context-preserving inspection,
- density,
- subtle hierarchy,
- restrained motion,
- consistent interaction rules.

A design direction should survive all major product surfaces. A beautiful hero screen is insufficient.

Reference:

- https://linear.app/
- https://linear.app/now/how-we-redesigned-the-linear-ui

## 7. Strategy / management games

The factory should not look like a game, but games offer decades of work on helping one person understand and control a large autonomous system without drowning in telemetry.

### RimWorld

Study:

- autonomous workers,
- priorities,
- interrupts,
- exceptions,
- work assignment,
- operator attention.

### Oxygen Not Included

Study:

- diagnostic overlays,
- switching the same world into different information lenses,
- resource flow,
- bottleneck diagnosis.

Keep one topology, then change its visual encoding with overlays such as Work, Agents, Cost, Quality, Risk, Bottlenecks and Rework.

### Cities: Skylines

Study:

- semantic zoom,
- overlays,
- network flow,
- local problems emerging from a large system,
- maintaining spatial orientation while changing diagnostic lenses.

### Prison Architect

Study:

- spatial systems,
- work state,
- alerts,
- layered operational information.

### Frostpunk

Study:

- scarce operator attention,
- urgent decisions,
- consequences and trade-offs,
- maintaining awareness of system-wide health while handling local crises.

### FTL

Study:

- glanceable subsystem health,
- strong prioritisation under failure,
- compact high-value state display.

### StarCraft observer interfaces

Study:

- dense status representation,
- rapid context switching,
- high information throughput,
- visual encoding that remains readable under activity.

## 8. Claude Design / design-agent references

These are references for design craft and agent behaviour. The Software Factory information architecture is covered elsewhere.

### Trystan-SA / claude-design-system-prompt

https://github.com/Trystan-SA/claude-design-system-prompt

Especially useful:

- frontend aesthetic direction,
- wireframe exploration,
- generate variations,
- hierarchy/rhythm review,
- interaction-states pass,
- accessibility audit,
- AI slop check,
- polish pass.

The repo describes itself as reverse-engineered. Its provenance is less important here than the usefulness of its principles and review procedures.

### asgeirtj / system_prompts_leaks: Anthropic claude-design.md

https://github.com/asgeirtj/system_prompts_leaks/blob/main/Anthropic/claude-design.md

Use as another reference implementation of the same broad design philosophy. Nothing in it is verified.

### Anthropic frontend aesthetics guidance

https://platform.claude.com/cookbook/coding-prompting-for-frontend-aesthetics

Useful for understanding how explicit visual direction, real context and iterative design reduce generic model output.

## 9. What to combine

The intended design character combines Factorio's system legibility, Dagster's navigable topology, Temporal's forensic timeline, Honeycomb's exploratory investigation, Linear's interaction polish and density, and strategy games' overlays and operator-attention techniques. No single DevOps, project-management or AI-agent dashboard provides the whole model.
