# Software Factory: working context for every coding agent

## Cross-everything, without exception

Everything in this repository is cross-platform, cross-operating-system, cross-shell, cross-language-ecosystem, cross-coding-agent, and cross-model. No agent is primary. No platform is the default.

- Never name one coding agent, one operating system, one shell, one package ecosystem, or one model vendor on its own in code, a rule, a test, or a document. If one is named, every supported one is, read from a single shared list.
- Every check, rule, and verifier states what it covers and what it does not. A clean result must never be read as "nothing found" when it means "nothing looked at".
- Prefer structured detection to a hand-written pattern. Parse the URL, parse the path, ask the platform. A regex is the last resort, and where one remains, its tests carry one case per platform and per agent.
- Examples in documentation rotate across agents and platforms, or use a made-up one. No example favours a vendor.
- A change that violates this is wrong even when it works on the author's machine. Review your own diff for it before opening a pull request.

## Working posture

- This project is moving from vision/research into implementation.
- Prefer explicit contracts and replaceable capabilities over hard-wiring GitHub, a specific coding agent, a specific backlog, or a specific runner.
- Do not build abstractions merely because they appeared in an earlier brainstorm. Introduce interfaces only where a real seam is demonstrated.
- Reuse existing agent workflow/skill systems where they already solve the problem. In particular, study `obra/superpowers` before implementing brainstorming, planning, TDD, debugging, review, or verification workflows ourselves.
- Deterministic checks are authoritative gates. LLM judgments may augment them but should not replace deterministic verification when deterministic tooling exists.
- The spec/intent chain and traceability are foundational concepts in the vision.
- Human involvement should trend toward exception-only, earned through measured reliability rather than assumed upfront.
- Name repository-owned documents with lowercase kebab-case filenames. Preserve ecosystem-mandated discovery names such as `AGENTS.md`, `README.md`, and `SKILL.md`.

## Current bootstrap choices

- Backlog: the work lives in the organisation's GitHub project, as issues with native types, parents and fields. The project is where work is planned and tracked. Keeping it out of the factory's own core model is still the rule: a repository the factory serves may use any tracker, reached through an adapter.
- Source control: Git and GitHub are expected adapters, and neither sits at the architectural centre.
- Coding agents: every supported agent is supported equally, and none is primary. The list of supported agents lives in one place in the code, and every rule that names an agent reads it.
- Workflow skills: prefer composition/reuse over re-implementation.
- Compute: local and remote runners, several operating systems and processor architectures, and eventually caching are expected concerns but are not reasons to bloat the first kernel.

## Before coding

1. Read the three documents in [`docs/vision/`](docs/vision/).
2. Read the records in [`docs/architecture/decisions/`](docs/architecture/decisions/) and the current [`architecture open questions`](docs/architecture/open-questions.md).
3. For product or interface work, also read [`docs/product/ux/`](docs/product/ux/) and [`docs/product/decisions/`](docs/product/decisions/).
4. Inspect [`docs/research/`](docs/research/) only when the task touches that topic. Research is evidence, and it sets no requirement on its own.
5. Challenge assumptions where the vision and implementation reality conflict.
6. Keep the first implementation small enough that its abstractions can still be deleted cheaply.
