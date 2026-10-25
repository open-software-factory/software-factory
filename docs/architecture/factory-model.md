# The factory model

Status: accepted for review.
Audience: anyone building, adopting or reviewing the factory.
Scope: the model, the terms, and the decisions that shape the develop job, the review job, the journal, identities and credentials, checks and risk, and adoption. The detailed decisions live in the decision records under docs/architecture/decisions/.

## Purpose

The factory exists to maximise automation and efficiency in producing great software products. It takes a work item to a reviewed pull request through jobs of one general pattern: a job runs on a runner-machine inside a sandbox, and inside the sandbox an agent follows a skill and calls osf commands. The develop job is the first such job; the review job is another.

In unattended mode a person starts a develop job and the builder agent follows the develop skill. The osf commands do the fixed, repeatable steps and reach the work-tracker and the forge only through adapters. Checks and reviews run in the sandbox before the push and again on CI after the pull request opens. The develop job ends at one or more pull requests and does not merge them. An osf command writes a risk section, and a separate platform-workflow decides from that risk whether to turn on auto-merge. Any other pull request needs a person. osf records the work in the journal for people and the factory-console; the journal gates nothing.

In interactive mode a person works with an agent in a coding-agent session, using the same skills and osf commands. A develop session is one example. Others include brainstorming, research, design, debugging and incident investigation.

## Terms

| Term | Definition | Avoid |
|---|---|---|
| develop skill | The one skill an agent follows to take a work item to a pull request. | run skill, engine |
| run | One instance of execution of a job, in the sense GitHub Actions gives it. Not narrowed to the develop skill. The osf run command is a command name, not a run. | engine run |
| unattended mode | Execution that a trigger starts, with no person present while it runs. The develop job is one example of unattended automation: LLM-based agents develop a part of a software product, such as a feature, a bug-fix or a maintenance task. | outside, host side, headless mode, mode (bare) |
| interactive mode | A person works with an agent in a coding-agent session, using the same skills and osf commands as in unattended mode. A develop session is one example. Others include brainstorming, research, design, debugging, incident investigation and other activity in building and operating a software product. Also supported. | local mode, mode (bare) |
| platform job | Retired name. Say 'job': one unit of work that an execution platform runs on a runner-machine. | host, runner machine |
| osf command | One subcommand of osf, such as osf lint, osf verify or osf review run. Its steps are fixed in code. Any model judgement it uses comes from an agent that it starts. | engine step |
| factory | The whole system this project builds: osf, its skills, its github-workflows, its factory images, its adapters, its journal and its factory-console, working together. Its goal is to maximise automation and efficiency in the work of producing great software products, and to leave people the work that needs taste, direction and judgment. Taking a work item to a reviewed pull request is one thing it does. | engine, the platform |
| open-software-factory | The project and its public repository, where the factory's source lives. | - |
| osf | The command-line program, one binary. Not the project and not the whole factory. | osf for the project, osf for the factory |
| skill | A SKILL.md file of instructions that an agent follows. It holds a procedure that needs judgement. | runbook |
| workflow | Plain English only: a process with steps, branches and conditions. A skill can define a workflow for an agent to follow. Never the name of a platform file. | workflow for a file |
| github-workflow | A GitHub Actions workflow file (.yml). | workflow (bare) for the file |
| platform-workflow | The platform-neutral idea behind a github-workflow: a file in any execution platform's own format that starts jobs. | - |
| adapter | osf code that wraps one outside system behind a fixed interface, such as the work-tracker adapter or the forge adapter. | - |
| journal | The record of the work on one work item, for people and the factory-console. Kept in the journal store, managed by the osf journal command, and gates nothing. One journal per work item; events are appended across attempts, and each event records the job run id or session id. | run index, audit log |
| factory-console | The user interface (web or desktop) that shows the factory's parts: the work graph, the status of work items and agents, and anything that needs human attention. A general operating dashboard for the people who build with the factory. | console (bare) |
| agent | One of the supported coding-agent programs: dsh, omp, opencode, codex and claude code (the list in crates/osf/src/agents.rs). | harness (bare) |
| agent runner | osf's module that starts one agent with a task and reads its result. Today called 'harness adapter'. | harness |
| model | The language model that an agent calls. | - |
| model family | The lineage of a model, such as anthropic, openai or deepseek. | - |
| person | A human. The 'owner' is the person who owns the project. | operator |
| builder | The role of the agent, with its model, that writes a change. Its identity on the forge is separate from the reviewer's. On GitHub today, a GitHub App is the mechanism (the builder app). Other forges and execution platforms may have their own mechanisms for separating identities and permissions. | - |
| reviewer | An agent that reviews the change. Preferably from a model family other than the builder's. When none is available it still reviews, with evidence that none was available, and the review says it was not independent. A review that runs on CI is posted under an identity different from the builder agent's. An in-sandbox review is early feedback that the builder must act on; it is not posted. On GitHub today, a reviewer or verifier GitHub App is one mechanism for separating identities and permissions. Other forges and execution platforms may have their own. | - |
| orchestrator | Undefined until the orchestrator model is designed. Where a document says 'orchestrator session' it means a person's own coding-agent session that hands work to sub-agents. | orchestrator in new text |
| execution platform | The automation service that starts and runs jobs when something happens. GitHub Actions today. | platform, substrate |
| job | One unit of work that an execution platform runs on a runner-machine. | platform job |
| runner-machine | The machine that executes a job on an execution platform. | runner (bare), host |
| developer machine | A person's own computer. | host |
| factory image | Any container image that the factory builds to run jobs and agents in. The factory may have more than one, for different kinds of job. | - |
| container | A running container in the ordinary sense, a Docker container today - and is one kind of sandbox. A 'dev container' is one that a person runs on a developer machine. | - |
| sandbox | Any technology that contains an agent's execution within a boundary. By definition every sandbox constrains the processes inside it to specific configured resources: files, network and so on. A container is one type. The factory supports types of sandbox and does not own one. | wall |
| adoption mode | A configuration key that says how an adopter installs the factory. Not the same as unattended or interactive mode. | - |
| develop job | A job in which an execution platform executes the develop skill over one work item. | run (for this) |
| develop session | A person's execution of the develop skill over one work item, in a coding-agent session. One example of interactive mode. | run (for this) |
| work item | The work-tracker's record of something to do: an issue, bug, task or story. | ticket, issue (for the general idea) |
| work-tracker | The service that holds work items. Today GitHub Issues and a project board. The command group is being renamed from osf tracker. | tracker |
| forge | The service that holds git repositories and pull requests. Today GitHub. | - |
| 'Ready' status | A status in the work-tracker meaning the work item is approved to be worked. Always written with the quotes and the word 'status'. The definition does not say who or what sets it. | Ready (bare) |
| dispatchable | A work item with the 'Ready' status that is not blocked and has nothing running on it. Computed, never set. | - |
| trigger | What starts a develop job: a person (manual), an event, or a schedule. | - |
| dispatcher | RETIRED. Not a defined part of the factory. No document designs one and no design session has created one. Do not use it until a design discussion creates it. | dispatcher as a component |
| hard stop | A point where the develop skill must stop. In unattended mode the builder agent's osf command writes the cause to the work item and ends the develop job. In interactive mode it asks the person. A failing check or a high-risk change is not a hard stop; the builder keeps going and fixes it, and the pull request risk decides auto-merge. A failed in-sandbox review is not a hard stop either; the builder reverts or fixes it within the round limit and goes on to the pull request after the limit. | - |
| check | One automated test of a change that passes or fails. Most checks today are deterministic. A check may later call a model, such as a classifier. Where determinism matters, write 'deterministic check'. | - |
| slot | A named kind of check, such as 'Security analysis', that a tool fills. | - |
| checkpoint | A named moment at which osf runs checks: an agent hook, pre-commit, pre-push, pull-request or schedule. osf verify runs one. | stage, phase |
| code-review | A reviewer's judgement of a change's code, against a set of review-lenses. An in-sandbox code-review is early feedback that the builder must act on; the builder fixes its must-fix and should-fix findings and repeats the review up to `[review] in_sandbox_rounds`, three by default and scoped to the develop job, or until none remains. When the limit is reached with a finding open, the builder records it in the journal and goes on to the pull request; the CI review check is the gate. A review that could not run is recorded and does not consume a round. It is not posted. A code-review that runs on CI is posted under the reviewer identity, and its result gates when the ruleset requires the review check. It can add to the checks but never replaces one. | review (bare) |
| spec-review | A reviewer's judgement of a work item's specification. | review (bare) |
| design-review | A reviewer's judgement of a design. | review (bare) |
| review-lens | One question that a reviewer answers in a review of any kind. | lens (bare) |
| verdict | The reviewer's outcome: approve, request changes, comment, or could not run. A CI review that could not run fails the review check; an in-sandbox review that could not run is recorded, does not consume a round, and the builder continues to the pull request. A deterministic after-check failure is a check failure, not a verdict. | - |
| review check | The CI check that carries a review result. It is required when the ruleset names it. A CI review that could not run, or a CI review whose deterministic after-check fails, fails this check. The in-sandbox review has its own after-check; it rejects the in-sandbox review, and the builder reverts the reviewer's changes and runs the review again. | review (bare) |
| gate | A check or a review whose failure stops a change from advancing. Settings choose which ones gate. | - |
| evidence | The recorded results (check results, verdicts, journal events) that a gate reads. | OBSERVED, REPORTED |
| policy | RETIRED. Use settings for configuration, risk rules for auto-merge, review-lens rules for the lens set, gate for pass or fail. | policy |
| authority | RETIRED. Name the specific thing instead: an identity, a permission, or the gate. | authority |
| sandbox type | One kind of sandbox that the factory supports. The first is a Docker container. Ideally many more follow. | - |
| risk | How far a change can reach if it carries a defect, worked out by an osf command from the change's blast radius and a set of rules. Today osf changeset risk gives one of three tiers, low, normal or high, with the reasons, and uses no model. | - |
| auto-merge | A forge feature that merges a pull request by itself once the forge's required checks pass. A platform-workflow turns it on for a pull request whose risk qualifies. The adopter configures which risk levels qualify. | - |
| journal store | The persistent, shared place where a work item's journal is kept. First implementation: an orphan branch on the forge. A local file is allowed as a working copy, but only the journal store is the record that people and the factory-console read. Managed by the osf journal command. | journal sink, journal file (as the record) |

## The parts

| Part | Owns |
|---|---|
| osf commands | Fixed, repeatable steps |
| skills | Procedures that need judgement |
| platform-workflows | When a job starts, where it runs where the execution platform allows a choice, the identity and secrets a job gets, and token minting outside the sandbox. They hold no logic: each step calls an osf command |
| adapters | Contact with one outside system |
| sandbox types | The factory owns no sandbox. It supports types of sandbox, first a Docker container |
| factory images | The software present in a Docker-type sandbox |
| settings | The resolved settings layers: CLI flags, environment, repository files, and user files |
| journal, factory-console | The record of the work, and the interface onto it |

The work-tracker and the forge are outside systems. Write "sandbox" where text is general, and "factory image" or "container" where it is about the Docker type.

## Who calls whom

A trigger starts a develop job. The platform-workflow's setup step mints the builder identity's short-lived token on the runner-machine outside the sandbox and passes only the token in. The setup step starts the builder agent with the develop skill. The skill calls osf commands. The osf commands reach the work-tracker and the forge only through adapters. An osf command starts an agent only to run a review. Hooks call osf commands at checkpoints. CI github-workflows run as separate jobs under identities the builder does not hold. Nothing calls a skill except an agent. The builder agent is the actor inside the sandbox; the platform-workflow's steps are the actors outside it; osf commands are the actors for fixed steps.

## The develop job

A person starts the first develop job by naming one work item. Events and schedules are later triggers for the same github-workflow. The job runs on a runner-machine inside a container made from a factory image; that container is the first sandbox type.

The builder agent follows one develop skill. The skill has an optional planning phase. When a plan already exists in the work item, the planning phase becomes a check that the plan still holds at implementation time. The builder implements in logical slices, may backtrack and replan, and chooses the pull request boundaries at the end from coherent, tested commits. A work item may be an epic that stands for a group; the develop job takes the epic and its sub-items and produces a logical series of pull requests. The line and file count is a soft signal for each pull request, never a gate on the implementation.

A failing check is the builder's to fix. A high-risk change does not stop the job; the pull request risk section decides auto-merge. The builder runs the in-sandbox review up to `[review] in_sandbox_rounds`, three by default and scoped to the develop job, fixing its must-fix and should-fix findings. When the limit is reached with a finding open, the builder records it in the journal and goes on to the pull request. A review that could not run is recorded, does not consume a round, and the builder continues. The in-sandbox review is not posted.

With no must-fix or should-fix finding remaining, the builder agent, through osf commands, creates one or more pull requests, sets the work item's status to show it is waiting, and ends the develop job. A missing approved spec, a decision only a person can make, or a missing credential is a hard stop: the builder agent's osf command writes the cause to the work item and ends the job. The builder may push a draft pull request if any work was done, with the open findings recorded in the journal; the CI review check is the gate. The job does not merge the pull requests.

## The review job

An in-sandbox review runs through one osf command. The command picks a configured prompt and a model from a configured, ordered list of models. Each entry names its agent and its model family. The command takes the first entry whose family differs from the builder's, else the first entry. The reviewer starts with its write tools disabled where the agent supports that, runs read-only, uses the builder's token, and is not posted. A deterministic after-check verifies that the working tree and git state are unchanged; if not, the in-sandbox review is rejected, the builder reverts the reviewer's changes and runs the review again. A different model family is a preference. When no other family is available, the review runs anyway, states that independence was not achieved, and records the evidence that every configured reviewer of another family was tried and failed.

The CI review runs in one review job. It reads repository-level OSF_REVIEW_* variables and secrets, not an environment. It posts under the reviewer's own identity and carries one review check. The review check is required when the ruleset names it. A CI review that could not run, or whose deterministic after-check fails, fails the review check. A pull request from a fork is reviewed only when the adopter enables it and supplies the same repository-level credentials. Review-lenses are chosen by rules that set a floor; a model may add a lens but never remove one. A must-fix finding vetoes; a disagreement is allowed only with deterministic evidence and needs a person.

## The journal

osf records state changes, check results, review verdicts, disagreements and hard stops in the journal. One journal exists per work item, kept in the journal store. The first journal store is an orphan branch on the forge, managed by the osf journal command. Events are appended across attempts, and each event records the job run id or the session id. The journal gates nothing. The review state, the round count and any open must-fix or should-fix finding are recorded in it so a pre-push hook can read them.

## Identities and credentials

The builder and the reviewer have separate forge identities. The platform-workflow's setup step mints the builder's short-lived token outside the sandbox and passes only that token in. The builder app's private key for that step lives in a repository secret read by the setup step, or in the execution platform's own app-token endpoint; it never enters the sandbox.

Provider credentials are provided to the sandbox from outside. The design does not decide whose they are or what they cost; an adopter may use an API key or a subscription, and osf supports both. A reviewer authenticates with an API-key variable or a user-supplied auth file. osf never copies an auth file into the reviewer's home; it mounts a configured auth file read-only. A reviewer with neither is could-not-run and names the missing variable or file.

## Checks, gates and risk

The CI github-workflows own the check graph and the required status checks. osf verify is a command a workflow may call for a specific checkpoint; it is not a second CI. Adopters keep their own CI and call osf commands where useful.

Deterministic checks and reviews run in the sandbox before the push. The builder must fix every failing check and every must-fix or should-fix finding. A pre-push hook may read the journal and refuse a push while a must-fix finding is open, if the adopter enables it; otherwise the loop is procedural and the deterministic after-check is the only mechanical guard. The same checks, and the CI review, run on CI after the pull request opens. The CI review check is a gate when the ruleset requires it.

The pull request description carries a risk section produced by an osf command from the change's blast radius and configurable risk rules. A separate platform-workflow decides from that evidence whether to turn on auto-merge. A pull request whose risk the adopter has configured to qualify is merged by the forge automatically when its required checks are green; any other pull request needs a person.

## Configuration and adoption

osf reads its settings from the resolved layers. The in-sandbox round limit is `[review] in_sandbox_rounds`, default 3, scoped to the develop job. The review prompt, the ordered model list and the family preference are settings. The [agents] section selects the agents and models, and the repository's AGENTS.md files carry the rules for the folders they govern.

An adopter's hooks and checks use the osf binary pinned in the factory image. The osf repository itself, or an adopter that explicitly opts in, may use the worktree-built osf. The override is explicit, and the check that runs on the pull request is the one that decides.

The review workflow is a reusable workflow with one review job. It reads repository-level OSF_REVIEW_* variables and secrets, not an environment. The job runs osf review over the configured reviewers and posts one review check. The fork path uses the same repository-level credentials. The per-agent jobs and the review environment are retired.

The first slice has no osf command that starts a sandbox. For a GitHub Actions workflow the sandbox is provided by the job configuration; for an interactive session on a developer's machine, a dev container is used. An osf command with an adapter per sandbox type arrives when a second sandbox type needs it.

## Decisions

The detail is in the decision records under docs/architecture/decisions/. This table maps the model's decisions to them.

| Decision | Record |
|---|---|
| 1. An unattended job runs inside a sandbox | 0008, 0010 |
| 2. Interactive mode uses the same skills and the same osf commands | 0005 |
| 3. A person starts the first unattended develop job | 0005 |
| 4. The builder holds a narrow, short-lived credential | 0020 |
| 5. Checks and reviews run in the sandbox before the push and again on CI | 0016 |
| 6. The parts and what each owns | 0005 |
| 7. Who calls whom | 0005 |
| 8. The first slice has no osf command that starts a sandbox | 0010 |
| 9. The journal is the record of the work | 0009, 0014 |
| 10. One journal per work item, kept in a journal store | 0009, 0014 |
| 11. A develop job ends at a pull request and does not merge it | 0008 |
| 12. A risk section decides whether auto-merge is turned on | new record |
| 13. In-sandbox reviews run through one osf command | 0016 |
| 14. A different model family is a preference, a same-family review is a last resort with evidence | 0016 |
| 15. Review-lenses: rules set a floor and a model may add | 0016 |
| 16. The builder fixes every finding, and a disagreement needs deterministic evidence | new record |
| 17. The reviewer has its own identity and runs as the builder does | 0020 |
| 18. Credentials are not the factory's concern | 0020 |
| 19. The vocabulary | 0005 |
| 20. The develop skill plans when needed, checks an existing plan, and slices at the pull request | new record |
| 21. The CI workflows own the check graph, and osf verify is a step | 0016 |
| 22. The osf binary is pinned for adopters and overridable for osf itself | 0018 |
| 23. The review workflow is one job over repository-level credentials | 0020 |
| 24. The review check is required | 0016 |

## Risks

- A same-family review counts the same as any other once the evidence is recorded; no gate reads the independence statement by default.
- Read-only enforcement rests on disabling write tools where an agent supports it and on the deterministic after-check. An agent that can write through a tool the command does not know about is caught only by the after-check.
- Auto-merge on a low-risk tier depends on osf changeset risk being right. The tiers have not been measured for false lows.
- The journal store on a forge branch is shared by every attempt and every job on the work item. Concurrent writes need a locking rule.
- The in-sandbox round loop is procedural by default. The journal records its state, but only an adopter-enabled pre-push hook can refuse a push mechanically.
- The develop skill's planning and pull-request slicing are new; a wrong slice produces a reviewable-looking series that is hard to reverse.

## Deferred

- Reviewer health: record each failure by cause, flag a work item after repeated failures, and stop retrying until a probe succeeds.
- Conventional commits: add type prefixes only when something reads them, such as release notes.
- Approval for low-risk pull requests in an unattended run: the owner exempts them with a written approval on that change.
- Staged oversight: how much a person approves, stage by stage, as the factory earns trust.
- The identity that turns on auto-merge, and how the risk result reaches the platform-workflow.
- The self-improvement loop: journals analysed to improve skills, prompts and settings.
- The develop skill's detailed steps, and which model adds review-lenses.
- The ordered statuses a develop job may set, and the work-tracker adapter design.
- An osf command that starts a sandbox, with an adapter per sandbox type.
- A second forge, a second work-tracker or a second execution platform.
- The orchestrator model.
- The adoption layout: private factory repo, watched repositories, caller workflows.

## Open questions

- Who or what sets the 'Ready' status. The process decision says a person sets it; whether work items may later move to it automatically is open.
- Whether the key proxy and per-reviewer network namespace work once the container is the boundary. The repository-level credentials make the key proxy less central.
- The git wrapper surface, against the container being the boundary.

## References

- docs/vision/ — why the factory exists.
- docs/architecture/decisions/ — the decision records.
- docs/architecture/open-questions.md — the open questions.
- docs/development.md — how to work in the container.
- docs/product/decisions/ and docs/product/ux/ — the product and interface decisions.
- skills/ — the skills the factory ships.
