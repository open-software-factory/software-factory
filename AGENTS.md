# Software Factory: working context for every coding agent

## Cross-everything, without exception

Everything in this repository is cross-platform, cross-operating-system, cross-shell, cross-language-ecosystem, cross-coding-agent, and cross-model. No agent is primary. No platform is the default.

- Never name one coding agent, one operating system, one shell, one package ecosystem, or one model vendor on its own in code, a rule, a test, or a document. If one is named, every supported one is, read from a single shared list.
- Every check, rule, and verifier states what it covers and what it does not. A clean result must never be read as "nothing found" when it means "nothing looked at".
- Prefer structured detection to a hand-written pattern. Parse the URL, parse the path, ask the platform. A regex is the last resort, and where one remains, its tests carry one case per platform and per agent.
- Examples in documentation rotate across agents and platforms, or use a made-up one. No example favours a vendor.
- A change that violates this is wrong even when it works on the author's machine. Review your own diff for it before opening a pull request.

## Public hygiene

- Never let a private person's name, a private company name, a private project name, or a local file path reach this repository. This applies to code, a document, a commit, an issue, and a pull request description alike. `osf scan` enforces the denylist, the session-link check, and the local-path check.
- Before a change goes in, picture a reader with no access to any private history. Would every line still make sense to that reader? A rule a scan cannot yet cover still binds. State that plainly wherever the rule is written down.

## Never weaken or bypass a check

- Deterministic checks are authoritative gates. A model's judgment may augment one but never replaces it where a deterministic check exists.
- A change that makes a check catch fewer real problems than before needs a person's written approval, with a reason, recorded on the change. Never hide a real failure behind a suppression flag, an ignored error, or a narrowed scope with no reason given.

## Before coding

1. Read the three documents in [`docs/vision/`](docs/vision/).
2. Read the records in [`docs/architecture/decisions/`](docs/architecture/decisions/) and the current [`architecture open questions`](docs/architecture/open-questions.md).
3. For product or interface work, also read [`docs/product/ux/`](docs/product/ux/) and [`docs/product/decisions/`](docs/product/decisions/). Read [`docs/research/`](docs/research/) only when the task touches that topic, since research is evidence rather than a requirement.
4. Challenge assumptions where the vision and implementation reality conflict.
5. Keep the first implementation small enough that its abstractions can still be deleted cheaply.
6. Add an interface only where a real seam is demonstrated. An earlier brainstorm having one is not a reason by itself. Check whether `obra/superpowers` already covers a workflow, such as brainstorming, planning, TDD, debugging, review, or verification, before building it yourself.

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
| [`skills/show-me/SKILL.md`](skills/show-me/SKILL.md), [`skills/pr-outline/SKILL.md`](skills/pr-outline/SKILL.md) | A visual, or a pull request's change outline, would help |
| [`docs/architecture/decisions/`](docs/architecture/decisions/) | Before any design decision |
