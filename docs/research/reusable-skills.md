# Reusable skills

Various skills are useful in multiple contexts. The software factory should be able to reuse them without re-implementation.

For example, the `obra/superpowers` approach is relevant to this project. The factory should reuse its workflow capabilities where they fit.

Capabilities observed/considered include:

- brainstorming before creative implementation;
- implementation-plan writing;
- test-driven development;
- systematic debugging;
- isolated git worktrees;
- plan execution;
- subagent/parallel-agent development;
- requesting and receiving code review;
- verification before completion;
- finishing/integrating a development branch.

## Architectural implication

The software factory needs two separate concepts.

`Workflow capability` is a reusable piece of engineering method encoded as a skill, tool or instruction set.

`Factory orchestration` selects capabilities, assigns execution contexts and resources, maintains durable state, enforces policy, collects telemetry, coordinates dependencies and parallelism, and escalates exceptions.

Do not create factory-specific implementations of brainstorming, TDD, debugging or review merely because the vision contains corresponding lifecycle phases.

## Questions for implementation

- Can Superpowers skills be treated as opaque capabilities with metadata?
- Is a generic skill/capability manifest useful?
- Does the factory execute skills directly or delegate to an agent harness that understands them?
- How much workflow state belongs to the skill versus the factory run?
- How can multiple skill systems coexist?
- How should deterministic gates override/augment skill-level verification?
