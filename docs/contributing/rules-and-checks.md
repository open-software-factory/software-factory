# Rules and checks

This page is for maintainers. It lists each rule that the agent-facing files hold, what enforces the rule today, and what no check covers yet. Agents do not load this page as instructions. The agent-facing files hold only the rules, as plain steps.

The agent-facing files are the root `AGENTS.md`, `crates/AGENTS.md`, `.github/workflows/AGENTS.md`, `skills/README.md`, `.agents/README.md`, and every `SKILL.md` under `skills/` and `.agents/skills/`.

"Judgment" means a reviewer has to decide, and no deterministic check exists. A rule with the label "no issue yet" has no open issue that owns a check for it.

Four decisions that the table names are not merged yet. The open pull request [open-software-factory/software-factory#136 (review check design)](https://github.com/open-software-factory/software-factory/pull/136) carries them:

- Decision 0016 is the review roster.
- Decision 0018 is hook enforcement.
- Decision 0019 is issue fields.
- Decision 0020 is review trust.

| Rule | Stated in | Enforced today | Not enforced yet |
|---|---|---|---|
| Do build, test, and review work in the development container | Root `AGENTS.md` | The container's root-owned git hooks run `osf` at commit and push, as `docs/development.md` describes | Work outside the container is not stopped. Agent hooks: [open-software-factory/software-factory#197 (agent hooks in the container)](https://github.com/open-software-factory/software-factory/issues/197). The second kind of container: [open-software-factory/software-factory#198 (two kinds of container)](https://github.com/open-software-factory/software-factory/issues/198) |
| Run the agent itself in that container | Root `AGENTS.md` | None | [open-software-factory/software-factory#197 (agent hooks in the container)](https://github.com/open-software-factory/software-factory/issues/197) |
| Never name one agent, system, shell, ecosystem, or vendor on its own | Root `AGENTS.md` | None | Judgment. No issue yet |
| Support every agent equally, from one shared list | Root `AGENTS.md` | `crates/osf/tests/scan_rules.rs` keeps the scan rules in step with `osf::agents::AGENTS` | Other rules and documents reading from the list. Judgment. No issue yet |
| State what each check covers and what it does not | Root `AGENTS.md`, `adding-an-osf-command` | Each rule has a Coverage section in its doc text. `osf explain <rule-id>` prints it. `crates/osf/tests/rule_docs.rs` lints the doc texts for style | A new check that leaves out its coverage. Judgment. No issue yet |
| Prefer structured detection to a hand-written pattern | Root `AGENTS.md` | None | Judgment. No issue yet |
| Rotate agents and platforms in examples | Root `AGENTS.md` | None | Judgment. No issue yet |
| Review your own diff against the cross-everything rules | Root `AGENTS.md` | None | Judgment. No issue yet |
| Keep private names and local paths out of the repository | Root `AGENTS.md` | `osf scan` in the CI job `hygiene`: every tracked file and every commit message a pull request adds | Issue bodies and pull request descriptions. No job scans them. No issue yet |
| Name an owned document in lowercase kebab-case | Root `AGENTS.md` | None | Judgment. No issue yet |
| Write for a reader with no private history | Root `AGENTS.md` | None | Judgment. No issue yet |
| Treat a deterministic check as authoritative | Root `AGENTS.md`, decision 0003 | `--gate` on `osf scan` and `osf lint writing` ignores the change's own exclude settings | The CI jobs run the change's own `osf`: [open-software-factory/software-factory#153 (CI decides with the base branch osf)](https://github.com/open-software-factory/software-factory/issues/153) |
| Get written approval before a change weakens a check | Root `AGENTS.md`, `writing-a-workflow` | The CI writing gate runs `--no-suppress --gate`, so a suppression comment cannot hide a finding | Detecting a weakened check: [open-software-factory/software-factory#153 (CI decides with the base branch osf)](https://github.com/open-software-factory/software-factory/issues/153). Decision 0018 |
| Read the vision, decisions, and product notes before coding | Root `AGENTS.md` | None | Judgment. No issue yet |
| Run `osf verify --stage pre-push` before you push | Root `AGENTS.md` | The container's `pre-push` hook runs it. CI runs the same checks as separate jobs | A push with `--no-verify` skips the hook. CI is the authority |
| Lint changed Markdown with `osf lint writing --no-suppress --gate` | Root `AGENTS.md` | The CI step "Check the writing" in the job `rust` | None |
| Keep the `osf` binary a thin shell | `crates/AGENTS.md` | None | Judgment. [open-software-factory/software-factory#191 (split osf into library crates)](https://github.com/open-software-factory/software-factory/issues/191) |
| Put logic in library crates, one per logical group | `crates/AGENTS.md` | None | Judgment. [open-software-factory/software-factory#191 (split osf into library crates)](https://github.com/open-software-factory/software-factory/issues/191) |
| Give each functional group its own crate | `crates/AGENTS.md` | None | Judgment. [open-software-factory/software-factory#191 (split osf into library crates)](https://github.com/open-software-factory/software-factory/issues/191) |
| Give each provider its own crate | `crates/AGENTS.md` | None | Judgment. [open-software-factory/software-factory#191 (split osf into library crates)](https://github.com/open-software-factory/software-factory/issues/191) |
| Keep a functional crate free of any one provider | `crates/AGENTS.md` | None | Judgment. [open-software-factory/software-factory#191 (split osf into library crates)](https://github.com/open-software-factory/software-factory/issues/191) |
| Put a new tool in the crate for its group | `crates/AGENTS.md` | None | Judgment. [open-software-factory/software-factory#191 (split osf into library crates)](https://github.com/open-software-factory/software-factory/issues/191) |
| Write the engine in Rust | `crates/AGENTS.md`, decision 0001 | None | Judgment. No issue yet |
| Pass build, test, clippy, and format checks before you push | `crates/AGENTS.md` | The CI job `rust` runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` and `cargo build --release --bin osf` | None |
| Define merge-deciding and secret jobs on the base branch | `.github/workflows/AGENTS.md`, `writing-a-workflow`, decision 0020 | `status-block.yml` follows the pattern | No lint reads a workflow file for it. No issue yet |
| Never check out pull request code into a job that holds a secret | `.github/workflows/AGENTS.md` | None | Judgment. No issue yet |
| Pin every action to a full commit SHA | `.github/workflows/AGENTS.md` | None | Judgment. [open-software-factory/software-factory#72 (supply-chain allowlists)](https://github.com/open-software-factory/software-factory/issues/72) |
| Grant each job only the permissions it needs | `.github/workflows/AGENTS.md` | None | Judgment. No issue yet |
| Start a secret-holding job with `step-security/harden-runner` in block mode | `.github/workflows/AGENTS.md` | None | Judgment. No issue yet |
| Let a review aid post information only | `.github/workflows/AGENTS.md` | None | Judgment. No issue yet |
| Give a job a short-lived, narrow token | `writing-a-workflow`, decision 0020 | None | Judgment. [open-software-factory/software-factory#160 (bot identity for agent-run forge operations)](https://github.com/open-software-factory/software-factory/issues/160) |
| Take the runner and reviewer credentials from the adopter's settings | `writing-a-workflow`, decision 0020 | None | Judgment. No issue yet |
| Name no fixed runner label and assume no always-on process | `writing-a-workflow`, decision 0020 | None | Judgment. No issue yet |
| Make a rule tell "could not read" from "found nothing" | `adding-an-osf-command`, decision 0003 | `osf hook stop` refuses a turn it could not check (`crates/osf/src/hook.rs`) | A shrinking test count or a new suppression as a finding of its own. Decision 0018. [open-software-factory/software-factory#153 (CI decides with the base branch osf)](https://github.com/open-software-factory/software-factory/issues/153) |
| Give each rule a class and a citation | `adding-an-osf-command` | The rule metadata structs in `crates/osf/src/lints/writing/meta.rs` and `crates/osf/src/scan/meta.rs` require both | Whether a citation is real. Judgment. No issue yet |
| Report a denylist match with only the file and line | `adding-an-osf-command` | `osf scan` rule `scan-denied-name` records no matched text | Other rules. Judgment. No issue yet |
| Read only the touched files by default | `adding-an-osf-command` | The CI writing gate has a `changed` and an `all` scope | A new check that reads everything. Judgment. No issue yet |
| Run a new check on this repository's own files first | `adding-an-osf-command` | None | Judgment. No issue yet |
| Make a check fail loudly | `adding-an-osf-command` | None | Judgment. No issue yet |
| Pass `cargo test --workspace` before a new rule is done | `adding-an-osf-command` | The CI job `rust` | None |
| Set type, parent, blocked-by, priority, and size as native fields | `filing-an-issue`, decision 0019 | None | No issue yet |
| Match an issue body to its type | `filing-an-issue`, decision 0019 | None | [open-software-factory/software-factory#157 (spec lint)](https://github.com/open-software-factory/software-factory/issues/157) |
| Give a Blocked item a written reason | `filing-an-issue`, decision 0019 | None | [open-software-factory/software-factory#159 (project board fields)](https://github.com/open-software-factory/software-factory/issues/159) |
| Read a new issue's number from the API response | `filing-an-issue` | None | Judgment. No issue yet |
| Write a whole new body to a file before you send it | `filing-an-issue` | None | Judgment. No issue yet |
| Scan an issue body before you post it | `filing-an-issue` | `osf scan --config osf.toml <file>` by hand | No job scans bodies. No issue yet |
| Open a pull request ready for review | `opening-a-pull-request` | None | [open-software-factory/software-factory#194 (rules layout)](https://github.com/open-software-factory/software-factory/issues/194) plans a CI check |
| Give each concern its own issue, branch, and pull request | `opening-a-pull-request` | None | Judgment. No issue yet |
| Name the closed issue as `owner/repo#N` with a label | `opening-a-pull-request` | `.github/PULL_REQUEST_TEMPLATE.md` asks for it. `osf scan` rule `scan-foreign-reference` flags another owner's repository | The label. Judgment. No issue yet |
| Close an issue with the merge commit hash and the pull request link | `opening-a-pull-request` | None | Judgment. No issue yet |
| Carry a diagram and an outline in the description | `opening-a-pull-request`, `pr-outline`, `show-me` | The pr-lens workflow draws the diagram. `osf pr section write` writes the outline | That the outline matches the diff. Judgment. [open-software-factory/software-factory#186 (review aids)](https://github.com/open-software-factory/software-factory/issues/186) |
| Do git work in its own worktree | `opening-a-pull-request` | None | Judgment. No issue yet |
| Use `git town` for stacks | `opening-a-pull-request` | `git-town.toml` sets the sync by merge. `git-town.yml` posts the stack view | Judgment. No issue yet |
| Carry no `Co-Authored-By` line and no session link in a commit | `opening-a-pull-request` | `osf scan --commits` in the CI job `hygiene`: rules `scan-coauthor-trailer` and `scan-session-link` | The `Co-Authored-By` rule matches only that exact spelling at the start of a line |
| Carry one `Code-Generator` trailer | `opening-a-pull-request` | None | [open-software-factory/software-factory#194 (rules layout)](https://github.com/open-software-factory/software-factory/issues/194) plans a commit-msg hook |
| Never force-push a branch you did not create | `opening-a-pull-request` | None | [open-software-factory/software-factory#194 (rules layout)](https://github.com/open-software-factory/software-factory/issues/194) plans a harness hook |
| Have a reviewer from another model family review | `opening-a-pull-request`, `reviewing-a-pull-request`, decision 0008 | None | The roster and the two-family quorum in decision 0016. [open-software-factory/software-factory#137 (the review check)](https://github.com/open-software-factory/software-factory/issues/137) |
| Give every change an independent review, and never treat pending as passing | `reviewing-a-pull-request`, decision 0016 | None | [open-software-factory/software-factory#137 (the review check)](https://github.com/open-software-factory/software-factory/issues/137) |
| Read pull request files as data in a review job | `reviewing-a-pull-request`, decision 0020 | `status-block.yml` follows the pattern | No check. No issue yet |
| Resolve a thread in the step that fixes and replies | `reviewing-a-pull-request` | None | Judgment. No issue yet |
| Record review judgment as evidence that augments a check | `reviewing-a-pull-request`, decision 0003 | None | Judgment. No issue yet |
| Run dispatched commands in the foreground | `running-sub-agents` | None | Judgment. No issue yet |
| Report sub-agent status from evidence only | `running-sub-agents` | None | Judgment. No issue yet |
| Keep `.claude/skills` a symlink to `.agents/skills` | `.agents/README.md` | The CI step "Check the Claude Code skills link" in the job `rust` | Which folder each agent reads: [open-software-factory/software-factory#199 (agents find `.agents/skills`)](https://github.com/open-software-factory/software-factory/issues/199) |
| Lint each skill folder with `osf lint skill` | `skills/README.md`, `.agents/README.md` | The CI step "Check the skills" in the job `rust`. The container's `pre-push` hook checks changed skill folders | SkillSpector and SkillEvaluator: [open-software-factory/software-factory#196 (osf lint skill runs SkillSpector and SkillEvaluator)](https://github.com/open-software-factory/software-factory/issues/196) |
| Keep maintainer notes out of agent-facing files | All agent-facing files | `osf lint skill` covers the structure of a skill | The content of a note. Judgment. No issue yet |
| Keep no `CLAUDE.md` file | `.agents/README.md` | None | Judgment. No issue yet |
