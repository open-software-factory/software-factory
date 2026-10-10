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
- Write an issue body or a pull request description to a file, and run `osf scan --config osf.toml <file>` on it before you post it.
- Name a document the repository owns in lowercase kebab-case, such as `review-check.md`. Keep the names an ecosystem requires, such as `AGENTS.md`, `README.md` and `SKILL.md`.
- Before a change goes in, picture a reader with no access to any private history. Check that every line still makes sense to that reader.

## Never weaken or bypass a check

- Treat a deterministic check as an authoritative gate. A model's judgment may augment one but never replaces it where a deterministic check exists.
- Get a person's written approval, with a reason, recorded on the change, before a change makes a check catch fewer real problems. Never hide a real failure behind a suppression flag, an ignored error, or a narrowed scope with no reason given.
- When a tool must run at a fixed moment, such as before a commit or a push, add it to the git hook or agent hook for that moment. Do not write it as a step in an `AGENTS.md` file or a skill.

## Act as the factory, never as its owner

- Make every GitHub write as the factory app for the role: the builder app, `opensoftwarefactory-builder[bot]`, to push, open a pull request, file or edit an issue, reply, or resolve a thread, and the verifier app, `opensoftwarefactory-verifier[bot]`, to review. A write covers an issue, a comment, a review, a project field, and a push.
- Never write as the owner's login, as the host's signed-in `gh` account, or through an ssh key. A pull request cannot be approved by its own author, and the ruleset also reads the account that pushed last, so the owner cannot approve a pull request the owner authored or pushed.
- Open the pull request as the builder app, from a branch the builder app pushed over https with its token. Read back the author after the first write. If it is not the app, stop and fix it before any other write. A pull request opened under the wrong account is closed and reopened, not repaired.
- Set the app token in the same shell call as each write. An environment variable does not carry over between calls, and a write without it falls back to the owner's login.
- Set the commit identity in every new worktree: run `git config --worktree user.name` and `user.email` to the builder app and its noreply address, `<bot user id>+<slug>[bot]@users.noreply.github.com`. The bot user id is not the app id; with the app id the commit shows as unattributed and needs a second approval. Before you push, check `git log origin/main..HEAD --format=%ae | sort | uniq -c`.
- Never use the owner's provider login for a model call: not a copied login file, not the owner's key. Use the factory's own key, a CI secret, or the harness's own credentials. When none covers the call, stop and say so.
- Carry one `Code-Generator:` trailer in each commit and no `Co-Authored-By` line, as the `opening-a-pull-request` skill says.
- Fill in `.github/PULL_REQUEST_TEMPLATE.md` for every pull request description, and never invent another layout. The tools write the pr-lens, status, and tree blocks, so do not write those by hand.

## Sub-agents

- Name the role, and repeat the rules in "Act as the factory, never as its owner", in every sub-agent prompt that writes to GitHub.
- Tell a sub-agent to run every command in the foreground. A command left running in the background can stall it.
- Report a sub-agent's status only from evidence it leaves, such as files, logs, or command output.

## Before coding

1. Read the documents in [`docs/vision/`](docs/vision/).
2. Read the records in [`docs/architecture/decisions/`](docs/architecture/decisions/) and the current [`architecture open questions`](docs/architecture/open-questions.md).
3. For product or interface work, also read [`docs/product/ux/`](docs/product/ux/) and [`docs/product/decisions/`](docs/product/decisions/). Read [`docs/research/`](docs/research/) only when the task touches that topic, since research is evidence rather than a requirement.
4. Challenge assumptions where the vision and implementation reality conflict.
5. Keep the first implementation small enough that its abstractions can still be deleted cheaply.
6. Add an interface only where a real seam is demonstrated. An earlier brainstorm having one is not a reason by itself. Check whether `obra/superpowers` already covers a workflow, such as brainstorming, planning, TDD, debugging, review, or verification, before building it yourself.

## Folder rules

Rules for a folder live in that folder's own `AGENTS.md`.
