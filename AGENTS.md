# Software Factory: Codex working context

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

- Backlog: deliberately undecided. Use a simple local `todo.md` while bootstrapping; do not make GitHub Issues/Projects part of the core model yet.
- Source control: Git/GitHub is an expected adapter, not the architectural center.
- Coding agents: multiple agents/harnesses must be supportable (Codex, Pi, OpenCode, Claude Code, etc.).
- Workflow skills: prefer composition/reuse over re-implementation.
- Compute: local and remote runners, multiple OSes/architectures, and eventually caching are expected concerns but are not reasons to bloat the first kernel.

## Before coding

1. Read the three documents in [`docs/vision/`](docs/vision/).
2. Read the records in [`docs/architecture/decisions/`](docs/architecture/decisions/) and the current [`architecture open questions`](docs/architecture/open-questions.md).
3. For product or interface work, also read [`docs/product/ux/`](docs/product/ux/) and [`docs/product/decisions/`](docs/product/decisions/).
4. Inspect [`docs/research/`](docs/research/) only when the task touches that topic. Research is evidence, not requirements.
5. Challenge assumptions where the vision and implementation reality conflict.
6. Keep the first implementation small enough that its abstractions can still be deleted cheaply.
