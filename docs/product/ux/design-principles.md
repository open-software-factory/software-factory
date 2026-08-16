# Software Factory design principles

## 1. Visual direction

The initial aesthetic direction is:

- dark themed,
- subtle rather than theatrical,
- mild green/blue tint as an exploratory base direction,
- contextual colour shifts may be explored later,
- subtle gradients are acceptable when they serve depth or context,
- strong information hierarchy,
- high information density without clutter,
- smooth, restrained interaction and animation,
- professional engineering control surface rather than sci-fi decoration.

The visual system remains incomplete. Explore typography, the exact palette, motion language and component styling through design variations before fixing them.

## 2. Content discipline

### Every element earns its place

Every visual element should answer a real operator question, advance understanding, expose an action, or create necessary hierarchy.

Remove filler, decorative metrics, repetitive labels and charts with no operational question behind them.

A useful test for every visualisation:

> I look at this when I need to know ______.

If that sentence cannot be completed convincingly, do not build the visualisation.

### Real data, no fake busyness

Never fabricate activity to make the factory appear active.

Animation, movement, flashing state, flowing edges and activity indicators must correspond to real state or meaningful events.

## 3. Information hierarchy

Hierarchy should communicate what deserves attention first, second and third using a deliberate combination of:

- size,
- colour,
- weight,
- position,
- density,
- motion,
- contrast.

Do not make every metric equally prominent.

Factory-level hierarchy should generally favour:

1. intervention-required state,
2. abnormal or changing state,
3. flow and bottlenecks,
4. important operating context,
5. routine raw metrics.

Raw numbers remain accessible, but they should not overwhelm the first glance.

## 4. Visual rhythm and systems

Use a coherent spacing scale, type scale, colour system, border/radius system and motion language.

Avoid arbitrary one-off values and local styling inventions.

Prefer components, tokens and reusable visual grammar over page-specific styling.

## 5. Typography

Use typography with intent rather than falling into generic model defaults.

- Keep the number of font families small.
- Define and follow a type scale.
- Optimise dense operational text for long-session readability.
- Use monospace selectively where machine identity, code, hashes, timings or raw telemetry benefit from it.
- Do not make the product look like a terminal merely because its users are engineers.

## 6. Colour

Colour must carry meaning and remain restrained.

- Start from a small coherent palette.
- Use toned darks rather than pure black where appropriate.
- Semantic colours should remain consistent across surfaces.
- Do not rely on colour alone for state.
- Avoid turning every work state into a different saturated colour.
- Reserve high-chroma colour for things that deserve attention.

Contextual tinting may eventually communicate major modes or system state, but this should be explored carefully and remain subtle.

## 7. Motion

Motion should explain causality, continuity, state change or spatial relationship.

Motion can show:

- a work item moving to a new execution stage,
- a graph expanding while preserving spatial orientation,
- a detail panel emerging from the selected object,
- a real event travelling between components,
- a timeline updating while the item is live,
- a state transition that would otherwise be easy to miss.

Avoid:

- decorative particles,
- fake packet streams,
- constant pulsing of healthy nodes,
- ambient movement with no informational meaning,
- animations that delay access to data.

Respect reduced-motion preferences.

## 8. Interaction states

Every interactive component should have deliberate states where applicable:

- default,
- hover,
- focus,
- active,
- selected,
- loading,
- streaming,
- disabled,
- warning,
- error,
- empty,
- stale/disconnected.

Streaming and autonomous-system states deserve the same design care as conventional loading/error states.

## 9. Accessibility

Accessibility is part of the design from the first sketch.

At minimum:

- keyboard operability,
- visible focus states,
- semantic labels,
- sufficient contrast,
- non-colour state indicators,
- reduced-motion support,
- usable target sizes,
- screen-reader interpretation for graph/timeline alternatives where feasible.

Dense engineering interfaces are not exempt from accessibility.

## 10. Factory-specific UX doctrine

### 10.1 Human attention is the scarce resource

Rank the interface by intervention value.

### 10.2 Healthy systems should be quiet

Routine autonomous operation should not visually compete with genuine exceptions.

### 10.3 Show work before agents

Agents are execution resources. Work items, intent, dependencies, artifacts and outcomes are the durable concepts.

Avoid turning the UI into a roster of anthropomorphised agents.

### 10.4 Visual motion must encode real state change

Never animate merely to create a sense of activity.

### 10.5 Overview, explanation, evidence

Every important summary should drill into why, and every explanation into raw evidence.

### 10.6 Preserve causal lineage

The operator must be able to travel from intent to spec, work, code, verification, deployment and outcome.

### 10.7 Show flow, queues and constraints

Counts alone hide starvation, congestion, parallelism limits and bottlenecks.

### 10.8 Time is a first-class axis

Current state without history is insufficient for understanding autonomous execution.

### 10.9 Confidence is visible only when useful

Surface uncertainty where it affects operator behaviour.

Do not attach meaningless confidence percentages to everything.

### 10.10 Cost must be contextualised

`$3.80` is weak information.

`$3.80: 2.4× baseline because review retried three times` is actionable information.

Prefer cost per useful outcome, cost deltas, cost anomalies and causal breakdowns over naked totals.

### 10.11 Automation explains itself

Important automated decisions should expose rationale and evidence without requiring transcript archaeology.

### 10.12 Operations and improvement are peers

The UI must show both how work is running and where the factory process itself can improve.

### 10.13 Prefer anomalies and deltas to totals

`What changed?`, `What is unusual?`, and `What differs from successful runs?` are often more valuable than another KPI card.

### 10.14 Density is allowed; clutter is not

This is a professional control surface for engineers. Do not dumb it down into a handful of giant cards.

### 10.15 Prefer direct manipulation over navigation

Zoom, pan, select, isolate, compare, overlay, scrub time and drill down while preserving context whenever practical.

### 10.16 Same truth, multiple projections

Every view should project the same shared domain and event model.

### 10.17 Do not hide the machinery

Default to useful abstraction, but always let engineers lift the hood to inspect exact numbers, events, traces and outputs.

### 10.18 Design for calm autonomy

The product should feel capable when nothing needs attention.

## 11. Generic AI-design anti-patterns

Use the design review material in the Trystan-SA Claude Design System Prompt as an explicit anti-slop review reference.

Avoid:

- gratuitous purple/pink/neon gradients,
- decorative emoji,
- generic rounded SaaS cards everywhere,
- arbitrary colours,
- arbitrary spacing,
- default-font choices with no rationale,
- huge KPI tiles with weak information value,
- overuse of glow and glassmorphism,
- fake sci-fi HUD styling,
- diagrams that are visually impressive but operationally useless.

Avoid replacing one design trope with another. The goal is intentional design.

## 12. Design workflow for agents

For meaningful new surfaces:

1. Understand the operator question being answered.
2. Read existing factory/domain context and current visual system.
3. Identify the information hierarchy before styling.
4. Explore multiple materially different interaction/layout directions when the solution is not obvious.
5. Build the smallest useful skeleton early.
6. Validate with realistic data and realistic density.
7. Test the surface in healthy, busy, blocked, failing, disconnected and empty states.
8. Run accessibility, interaction-state, hierarchy/rhythm and AI-slop reviews.
9. Polish only after the information model and interaction model work.
