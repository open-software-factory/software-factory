# Software Factory: working context for every coding agent

## Work inside the development container

- Do all build, test, and review work inside this repository's development container, [`.devcontainer/`](.devcontainer/).
- Run the coding agent itself in that container too, so the root-owned git hooks apply.

## Cross-everything, without exception

Everything in this repository is cross-platform, cross-operating-system, cross-shell, cross-language-ecosystem, cross-coding-agent, and cross-model. No agent is primary. No platform is the default.

- Never name one coding agent, one operating system, one shell, one package ecosystem, or one model vendor on its own. This applies to code, a rule, a test, and a document. If you name one, name every supported one, read from a single shared list.
- Support every coding agent equally. Keep the list of supported agents in one place, `crate::agents::AGENTS` (`crates/osf/src/agents.rs`). Make every rule that names an agent read that list.
- State what every check, rule, and verifier covers and what it does not. Never let a clean result read as "nothing found" when it means "nothing looked at".
- Prefer structured detection to a hand-written pattern. Parse the URL, parse the path, ask the platform. Use a regex as the last resort. Where one remains, write one test case per platform and per agent.
- Rotate the agents and platforms in documentation examples, or use a made-up one. Never let an example favour a vendor.
- Review your own diff against these rules before you open a pull request. A change that breaks them is wrong even when it works on your machine.

## Public hygiene

- Never let a private person's name, a private company name, a private project name, or a local file path reach this repository. This applies to code, a document, a commit, an issue, and a pull request description.
- Run `osf scan --config osf.toml` before you push. It checks every file this repository tracks.
- Run `osf scan --config osf.toml --commits origin/main..HEAD` before you push. It checks the commit messages your change adds.
- Write an issue body or a pull request description to a file, and run `osf scan --config osf.toml <file>` on it before you post it.
- Name a document the repository owns in lowercase kebab-case, such as `review-check.md`. Keep the names an ecosystem requires, such as `AGENTS.md`, `README.md` and `SKILL.md`.
- Before a change goes in, picture a reader with no access to any private history. Check that every line still makes sense to that reader.

## Never weaken or bypass a check

- Treat a deterministic check as an authoritative gate. A model's judgment may augment one but never replaces it where a deterministic check exists.
- Get a person's written approval, with a reason, recorded on the change, before a change makes a check catch fewer real problems. Never hide a real failure behind a suppression flag, an ignored error, or a narrowed scope with no reason given.

## Before coding

1. Read the three documents in [`docs/vision/`](docs/vision/).
2. Read the records in [`docs/architecture/decisions/`](docs/architecture/decisions/) and the current [`architecture open questions`](docs/architecture/open-questions.md).
3. For product or interface work, also read [`docs/product/ux/`](docs/product/ux/) and [`docs/product/decisions/`](docs/product/decisions/). Read [`docs/research/`](docs/research/) only when the task touches that topic, since research is evidence rather than a requirement.
4. Challenge assumptions where the vision and implementation reality conflict.
5. Keep the first implementation small enough that its abstractions can still be deleted cheaply.
6. Add an interface only where a real seam is demonstrated. An earlier brainstorm having one is not a reason by itself. Check whether `obra/superpowers` already covers a workflow, such as brainstorming, planning, TDD, debugging, review, or verification, before building it yourself.

## Before you push

1. Run `osf verify --stage pre-push` from the repository root. It checks the files, prose, skill folders, and commit messages your change touches.
2. Run `osf lint writing --no-suppress --gate <file>` on each Markdown file you changed.
3. Fix every error these commands report.

## Where the other rules live

This file holds only what every task needs. A rule tied to one folder lives in that folder's own `AGENTS.md`. A rule tied to one moment in the work reaches an agent as a skill that loads when the task matches it.

[`skills/README.md`](skills/README.md) and [`.agents/README.md`](.agents/README.md) say which skills folder holds what, and how an adopter installs a shipped skill.

| Folder or skill | Loads |
|---|---|
| [`skills/README.md`](skills/README.md) | Installing or shipping a skill for adopters |
| [`.agents/README.md`](.agents/README.md) | Working on a skill used only inside this repository, or asking why there is no `CLAUDE.md` |
| [`crates/AGENTS.md`](crates/AGENTS.md) | Working under `crates/` |
| [`.github/workflows/AGENTS.md`](.github/workflows/AGENTS.md) | Working under `.github/workflows/` |
| [`.agents/skills/adding-an-osf-command/SKILL.md`](.agents/skills/adding-an-osf-command/SKILL.md) | Adding a check, a lint rule, or an `osf` command |
| [`.agents/skills/writing-a-workflow/SKILL.md`](.agents/skills/writing-a-workflow/SKILL.md) | Adding or editing a workflow file, or handling a secret |
| [`.agents/skills/filing-an-issue/SKILL.md`](.agents/skills/filing-an-issue/SKILL.md) | Filing or editing an issue |
| [`.agents/skills/opening-a-pull-request/SKILL.md`](.agents/skills/opening-a-pull-request/SKILL.md) | Opening a pull request, or updating its description |
| [`.agents/skills/reviewing-a-pull-request/SKILL.md`](.agents/skills/reviewing-a-pull-request/SKILL.md) | Reviewing a pull request, or posting a review result |
| [`.agents/skills/running-sub-agents/SKILL.md`](.agents/skills/running-sub-agents/SKILL.md) | Dispatching a sub-agent, or a batch of them |
| [`skills/show-me/SKILL.md`](skills/show-me/SKILL.md), [`skills/pr-outline/SKILL.md`](skills/pr-outline/SKILL.md) | A visual, or a pull request's change outline, would help |
| [`docs/architecture/decisions/`](docs/architecture/decisions/) | Before any design decision |
