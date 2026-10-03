# Rules and checks

This page is for maintainers. It lists each rule that the agent-facing files hold, where the rule is stated, and what enforces it. Where nothing does, the table says "not enforced". Agents do not load this page as instructions.

The agent-facing files are the root `AGENTS.md`, `crates/AGENTS.md`, `.github/workflows/AGENTS.md`, `skills/README.md`, `.agents/README.md`, and every `SKILL.md` under `skills/` and `.agents/skills/`.

| Rule | Stated in | Enforced by |
|---|---|---|
| Do build, test, and review work in the development container | Root `AGENTS.md` | The container's root-owned git hooks run `osf` at commit and push, as `docs/development.md` describes. Work outside the container is not enforced |
| Run the agent itself in that container | Root `AGENTS.md` | Not enforced |
| Never name an agent, system, shell, ecosystem, or vendor on its own | Root `AGENTS.md` | Not enforced |
| Support every agent equally, from the shared list | Root `AGENTS.md` | `crates/osf/tests/scan_rules.rs` keeps the scan rules in step with `osf::agents::AGENTS` |
| State what each check covers and what it does not | Root `AGENTS.md`, `adding-an-osf-command` | Each rule has a Coverage section in its doc text. `osf explain <rule-id>` prints it. `crates/osf/tests/rule_docs.rs` lints the doc texts for style |
| Prefer structured detection to a hand-written pattern | Root `AGENTS.md` | Not enforced |
| Rotate agents and platforms in examples | Root `AGENTS.md` | Not enforced |
| Review your own diff against the cross-everything rules | Root `AGENTS.md` | Not enforced |
| Keep private names and local paths out of the repository | Root `AGENTS.md` | `osf scan` in the CI job `hygiene`, on every tracked file and every commit message a pull request adds. Issue bodies and pull request descriptions are not enforced |
| Name an owned document in lowercase kebab-case | Root `AGENTS.md` | Not enforced |
| Write for a reader with no private history | Root `AGENTS.md` | Not enforced |
| Treat a deterministic check as authoritative | Root `AGENTS.md`, decision 0003 | `--gate` on `osf scan` and `osf lint writing` ignores the change's own exclude settings |
| Get written approval before a change weakens a check | Root `AGENTS.md`, `writing-a-workflow` | The CI writing gate runs `--no-suppress --gate`, so a suppression comment cannot hide a finding. Detecting a weakened check is not enforced |
| Read the vision, decisions, and product notes before coding | Root `AGENTS.md` | Not enforced |
| Keep the `osf` binary a thin shell | `crates/AGENTS.md` | Not enforced |
| Put logic in library crates, a crate for each logical group | `crates/AGENTS.md` | Not enforced |
| Give each functional group its own crate | `crates/AGENTS.md` | Not enforced |
| Give each provider its own crate | `crates/AGENTS.md` | Not enforced |
| Keep a functional crate free of provider-specific code | `crates/AGENTS.md` | Not enforced |
| Put a new tool in the crate for its group | `crates/AGENTS.md` | Not enforced |
| Write everything in Rust by default, including the engine, its command line, and the verifier runner | `crates/AGENTS.md`, decision 0001 | Not enforced |
| Choose another language only for an edge component that a host's or provider's context requires | `crates/AGENTS.md` | Not enforced |
| Write any JavaScript in TypeScript | `crates/AGENTS.md` | Not enforced |
| Pass build, test, clippy, and format checks before you push | `crates/AGENTS.md` | The CI job `rust` runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `cargo build --release --bin osf` |
| Define merge-deciding and secret jobs on the base branch | `.github/workflows/AGENTS.md`, `writing-a-workflow`, decision 0020 | Not enforced. `status-block.yml` follows the pattern |
| Never check out pull request code into a job that holds a secret | `.github/workflows/AGENTS.md` | Not enforced |
| Pin every action to a full commit SHA | `.github/workflows/AGENTS.md` | Not enforced |
| Grant each job only the permissions it needs | `.github/workflows/AGENTS.md` | Not enforced |
| Start a secret-holding job with `step-security/harden-runner` in block mode | `.github/workflows/AGENTS.md` | Not enforced |
| Let a review aid post information only | `.github/workflows/AGENTS.md` | Not enforced |
| Give a job a short-lived, narrow token | `writing-a-workflow`, decision 0020 | Not enforced |
| Take the runner and reviewer credentials from the adopter's settings | `writing-a-workflow`, decision 0020 | Not enforced |
| Name no fixed runner label and assume no always-on process | `writing-a-workflow`, decision 0020 | Not enforced |
| Make a rule tell "could not read" from "found nothing" | `adding-an-osf-command`, decision 0003 | `osf hook stop` refuses a turn it could not check (`crates/osf/src/hook.rs`). A shrinking test total or a new suppression as a finding is not enforced |
| Give each rule a class and a citation | `adding-an-osf-command` | The rule metadata structs in `crates/osf/src/lints/writing/meta.rs` and `crates/osf/src/scan/meta.rs` require a class and a citation. Whether a citation is real is not enforced |
| Report a denylist match with only the file and line | `adding-an-osf-command` | `osf scan` rule `scan-denied-name` records no matched text. Other rules are not enforced |
| Read only the touched files by default | `adding-an-osf-command` | The CI writing gate has a `changed` and an `all` scope. A new check that reads everything is not enforced |
| Run a new check on this repository's own files before adopting it | `adding-an-osf-command` | Not enforced |
| Make a check fail loudly | `adding-an-osf-command` | Not enforced |
| Pass `cargo test --workspace` before a new rule is done | `adding-an-osf-command` | The CI job `rust` |
| Set type, parent, blocked-by, priority, and size as native fields | `filing-an-issue`, decision 0019 | Not enforced |
| Match an issue body to its type | `filing-an-issue`, decision 0019 | Not enforced |
| Give a Blocked item a written reason | `filing-an-issue`, decision 0019 | Not enforced |
| Read a new issue's number from the API response | `filing-an-issue` | Not enforced |
| Write a whole new body to a file before you send it | `filing-an-issue` | Not enforced |
| Scan an issue body before you post it | `filing-an-issue` | `osf scan --config osf.toml <file>` by hand. No job scans bodies |
| Open a pull request ready for review | `opening-a-pull-request` | Not enforced |
| Give each concern its own issue, branch, and pull request | `opening-a-pull-request` | Not enforced |
| Name the closed issue as `owner/repo#N` with a label | `opening-a-pull-request` | `.github/PULL_REQUEST_TEMPLATE.md` asks for it. `osf scan` rule `scan-foreign-reference` flags another owner's repository. The label is not enforced |
| Close an issue with the merge commit hash and the pull request link | `opening-a-pull-request` | Not enforced |
| Carry a diagram and an outline in the description | `opening-a-pull-request`, `pr-outline`, `show-me` | The pr-lens workflow draws the diagram. `osf pr section write` writes the outline. That the outline matches the diff is not enforced |
| Do git work in its own worktree | `opening-a-pull-request` | Not enforced |
| Use `git town` for stacks | `opening-a-pull-request` | `git-town.toml` sets the sync by merge. `git-town.yml` posts the stack view |
| Carry no `Co-Authored-By` line and no session link in a commit | `opening-a-pull-request` | `osf scan --commits` in the CI job `hygiene`: rules `scan-coauthor-trailer` and `scan-session-link`. The `Co-Authored-By` rule matches only that exact spelling at the start of a line |
| Carry a `Code-Generator` trailer | `opening-a-pull-request` | Not enforced |
| Never force-push a branch you did not create | `opening-a-pull-request` | Not enforced |
| Have a reviewer from another model family review | `opening-a-pull-request`, `reviewing-a-pull-request`, decision 0008 | Not enforced |
| Give every change an independent review, and never treat pending as passing | `reviewing-a-pull-request`, decision 0016 | Not enforced |
| Read pull request files as data in a review job | `reviewing-a-pull-request`, decision 0020 | Not enforced. `status-block.yml` follows the pattern |
| Resolve a thread in the step that fixes and replies | `reviewing-a-pull-request` | Not enforced |
| Record review judgment as evidence that augments a check | `reviewing-a-pull-request`, decision 0003 | Not enforced |
| Run dispatched commands in the foreground | `running-sub-agents` | Not enforced |
| Report sub-agent status from evidence only | `running-sub-agents` | Not enforced |
| Keep `.claude/skills` a symlink to `.agents/skills` | `.agents/README.md` | The CI step "Check the Claude Code skills link" in the job `rust`. Which folder each agent reads is not enforced |
| Lint each skill folder with `osf lint skill` | `skills/README.md`, `.agents/README.md` | The CI step "Check the skills" in the job `rust`. The container's `pre-push` hook checks changed skill folders |
| Keep maintainer notes out of agent-facing files | All agent-facing files | `osf lint skill` covers the structure of a skill. The content of a note is not enforced |
| Keep no `CLAUDE.md` file | `.agents/README.md` | Not enforced |
