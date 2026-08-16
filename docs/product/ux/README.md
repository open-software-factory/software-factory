# Software Factory UX and design context

These documents give coding and design agents enough stable context to make coherent UI/UX decisions without fixing implementation details too early.

Read in this order:

1. `factory-ux-vision.md`: what the product should feel like, the operator model, and the six primary surfaces.
2. `design-principles.md`: durable visual, interaction, information and factory-specific design principles.
3. `open-questions.md`: ideas worth exploring that are not architectural commitments yet.

Supporting research:

- [`../../research/ux/design-workflow-landscape.md`](../../research/ux/design-workflow-landscape.md): survey and proposed benchmark for AI-assisted design workflows, with non-designer usability as a hard gate.
- [`../../research/ux/ux-references.md`](../../research/ux/ux-references.md): products, tools, games and interface families to study, plus what to learn from each.

## Relationship to the software-factory vision

These documents are a UI/UX companion to the existing Software Factory documents:

- Agent-Native Operating Model
- Product Lifecycle / Outer Loop
- Automated SPDLC / SDLC Vision

The same rule applies here as in the SPDLC vision: principles should remain stable while implementation mechanisms can evolve.

## Working rule for agents

Do not treat examples, candidate libraries, or visual references as requirements unless a document explicitly says they are. Preserve the intent and interaction model first; choose implementation details second.
