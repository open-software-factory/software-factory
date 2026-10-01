# Software Factory: working context for every coding agent

## Cross-everything, without exception

Everything in this repository is cross-platform, cross-operating-system, cross-shell, cross-language-ecosystem, cross-coding-agent, and cross-model. No agent is primary. No platform is the default.

- Never name one coding agent, one operating system, one shell, one package ecosystem, or one model vendor on its own in code, a rule, a test, or a document. If one is named, every supported one is, read from a single shared list.
- Every check, rule, and verifier states what it covers and what it does not. A clean result must never be read as "nothing found" when it means "nothing looked at".
- Prefer structured detection to a hand-written pattern. Parse the URL, parse the path, ask the platform. A regex is the last resort, and where one remains, its tests carry one case per platform and per agent.
- Examples in documentation rotate across agents and platforms, or use a made-up one. No example favours a vendor.
- A change that violates this is wrong even when it works on the author's machine. Review your own diff for it before opening a pull request.

## Public hygiene

- Never let a private person's name, a private company name, a private project name, or a local file path reach this repository. This applies to code, a document, a commit, an issue, and a pull request description alike.
- Before a change goes in, picture a reader with no access to any private history. Would every line still make sense to that reader?
- `osf scan` enforces the denylist, the session-link check, and the local-path check. A rule a scan cannot yet cover still binds. State that plainly wherever the rule is written down.

## Never weaken or bypass a check

- Deterministic checks are authoritative gates. A model's judgment may augment one but never replaces it where a deterministic check exists.
- A change that makes a check catch fewer real problems than before needs a person's written approval, with a reason, recorded on the change.
- Never hide a real failure behind a suppression flag, an ignored error, or a narrowed scope with no reason given.

## Working posture

- This project is moving from vision/research into implementation.
- Prefer explicit contracts and replaceable capabilities over hard-wiring GitHub, a specific coding agent, a specific backlog, or a specific runner.
- Do not build abstractions merely because they appeared in an earlier brainstorm. Introduce interfaces only where a real seam is demonstrated.
- Reuse existing agent workflow/skill systems where they already solve the problem. In particular, study `obra/superpowers` before implementing brainstorming, planning, TDD, debugging, review, or verification workflows ourselves.
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

## Where the other rules live

This file holds only what every task needs. A rule tied to one folder lives in that folder's own `AGENTS.md`. A rule tied to one moment in the work reaches an agent as a skill that loads when the task matches it.

| Folder or skill | Loads |
|---|---|
| [`crates/AGENTS.md`](crates/AGENTS.md) | Working under `crates/` |
| [`.github/workflows/AGENTS.md`](.github/workflows/AGENTS.md) | Working under `.github/workflows/` |
| [`skills/adding-an-osf-command/SKILL.md`](skills/adding-an-osf-command/SKILL.md) | Adding a check, a lint rule, or an `osf` command |
| [`skills/writing-a-workflow/SKILL.md`](skills/writing-a-workflow/SKILL.md) | Adding or editing a workflow file, or handling a secret |
| [`skills/filing-an-issue/SKILL.md`](skills/filing-an-issue/SKILL.md) | Filing or editing an issue |
| [`skills/opening-a-pull-request/SKILL.md`](skills/opening-a-pull-request/SKILL.md) | Opening a pull request, or updating its description |
| [`skills/reviewing-a-pull-request/SKILL.md`](skills/reviewing-a-pull-request/SKILL.md) | Reviewing a pull request, or posting a review result |
| [`skills/running-sub-agents/SKILL.md`](skills/running-sub-agents/SKILL.md) | Dispatching a sub-agent, or a batch of them |
| [`skills/show-me/SKILL.md`](skills/show-me/SKILL.md) | A visual would answer the question better than prose |
| [`skills/pr-outline/SKILL.md`](skills/pr-outline/SKILL.md) | Writing a pull request's change outline |
| [`docs/architecture/decisions/`](docs/architecture/decisions/) | Before any design decision |
