# 0016: The review check, review lenses and the reducer

Status: accepted

Date: 2026-09-25, amended 2026-10-03: the review prompt is a file, a reviewer receives metadata and reads a clean copy of the change with read-only tools, each reviewer runs in a CI job of its own with only its own key, and the roster is the agent list in `crates/osf/src/agents.rs`. Amended again 2026-10-03: the review check is advisory for now, a family counts after two answered rounds, a saved run is bound to its pull request and CI run, the work item comes from the linked issue, the shipped prompt is an embedded file, and a reviewer's sandbox must start before it reviews. Amended again 2026-10-05: the codex reviewer job runs on the runner host with a pinned release, and a saved work item is bound to the issue number and the head commit. Amended again 2026-10-05, on the owner's decision: the codex reviewer runs inside the review container again, with its own sandbox and approvals bypass switched on, because the container is the wall. This replaces the runner-host form, the bubblewrap helper pin and the sandbox-start check. Amended again 2026-10-05, on a review of this record: the codex version comes with the review image, which is pinned by digest, and the codex job checks it with `codex --version`, so the workflow has no codex download, no sha256 and no extraction. The bypass applies only inside the factory's container. The workflow is updated in [open-software-factory/software-factory#178 (review check)](https://github.com/open-software-factory/software-factory/pull/178).

## Context

[Decision 0013 (the aggregation check)](0013-the-aggregation-check.md) made a review one check at the pull-request checkpoint, with the evidence grade reported. It did not say how the review runs, what it reads, what shape its answer takes, or how several answers become one verdict.

The first branch built on the verification seam showed why that matters. Every review of it came from one model family, and the same agents wrote its code and its tests. The branch passed every deterministic check and still carried defects that a person found by asking. One was a run where every check was skipped and the checkpoint still passed.

The design is [the verification seam](../verification-seam.md). The research is [model reviews checked by a deterministic reducer](../../research/2026-09-25-verifier-and-reducer-review-patterns.md) and [change risk classification](../../research/2026-09-17-change-risk-classification.md).

### Amendment, 2026-10-03

Before 2026-10-03, osf pasted an excerpt of the change into the prompt for each reviewer. The risk tier chose how much code the excerpt held, and a fixed size cap bounded it. The reviewer prompt was text inside osf, and one CI job held the keys of every reviewer.

The owner changed these things. The prompt frame is a file an adopter can edit. A reviewer receives metadata about the change and reads the rest itself with read-only tools. Each reviewer runs in a CI job of its own, with only its own provider's key. The roster is the agent list osf keeps in `crates/osf/src/agents.rs`.

The reason for the prompt file and for metadata is the owner's observation that a reviewer given only limited context "will raise irrelevant things". A pasted excerpt cuts off the code around a change, so the reviewer raises findings that the surrounding code answers. Giving a reviewer tools makes steered text a larger risk, because a reviewer that reads more reads more text that an agent or a fork wrote. [Decision 0020 (who can post a review result)](0020-who-can-post-a-review-result.md) records how the design contains that risk.

## Options considered

**The shape of the review.**

| Option | What it meant | Outcome |
|---|---|---|
| Several reviewers and a reducer | Each reviewer answers for one review lens in a fixed JSON shape. A deterministic reducer, written in osf, turns the answers into one verdict. | Taken. |
| One second-opinion reviewer | One reviewer from another model family posts findings as prose. | Set aside. Nothing combines answers, and nothing checks the answer's shape. |

**How osf runs a reviewer.**

| Option | What it meant | Outcome |
|---|---|---|
| A coding-agent command-line tool, run headless | osf starts a harness such as codex or Claude Code with the lens prompt. The harness reads the repository with its own tools. | Taken. The harnesses already have the tooling to read and search a repository, and osf does not need to rebuild it. |
| Direct calls to a model provider's interface | osf sends a fixed bundle of context to each provider and enforces the output shape through the provider. | Set aside. It would make osf own tooling the harnesses already have. |

**Where the review prompt lives.**

| Option | What it meant | Outcome |
|---|---|---|
| A default prompt file that osf ships, which a repository can override | The role, the rules and how to answer sit in a file, found by the same precedence as the lens files. | Taken. An adopter edits the frame without an osf release. |
| Prompt text compiled into osf | The frame is a string in the code. | Set aside. An adopter cannot see it or change it, and a wording change needs a release. |
| A default file installed beside the binary | osf finds the default on disk next to itself, and a repository's file comes before it. | Set aside. [Decision 0006 (distribution and packaging)](0006-distribution-and-packaging.md) ships one static binary, and a file beside it would sit outside the files that binary and its checksum wrapper cover. The shipped lens files already travel inside the binary. |

**What a reviewer receives.**

| Option | What it meant | Outcome |
|---|---|---|
| Metadata, and read-only tools over a clean copy of the change | The reviewer gets the facts that identify the change and reads whatever else it needs. The copy holds the change's files and no coding agent's settings. | Taken. |
| A pasted excerpt of the change, under a fixed size cap | osf chooses the code the reviewer sees. | Set aside. A reviewer given only limited context "will raise irrelevant things". |

**How a reviewer with tools stays contained.**

| Option | What it meant | Outcome |
|---|---|---|
| Read-only tools, no shell beyond read-only use, and a network allow-list | The sandbox removes what a steered reviewer could do. The quote check stays. | Taken. |
| Tools with write access, or an open network | The reviewer can edit the checkout or call any host. | Set aside. A steered reviewer could change the code it judges or send out what it can read. |

**Where each reviewer's key lives.**

| Option | What it meant | Outcome |
|---|---|---|
| Each reviewer in its own CI job, holding only its own provider's key | Only the final job, which combines the answers and posts the result, mints the verifier's write token. | Taken. |
| A single job holding every reviewer's key | The reviewers run side by side in one job. | Set aside. A steered reviewer that can read files could reach the key of every other provider. |

**Where the roster lives.**

| Option | What it meant | Outcome |
|---|---|---|
| The agent list in `crates/osf/src/agents.rs`, with adopters choosing reviewers in the `[agents]` section of `osf.toml`, the file of settings a repository gives osf | The list osf keeps of the coding agents it supports is the roster. | Taken. |
| A second list kept by hand in `.devcontainer/agents.json` | The development container's own list of installed agents is the roster. | Set aside. A hand-kept list drifts from the list in the code. |

**The lens catalogue.** The change-risk research and [open-software-factory/software-factory#31 (risk classification and the axis catalogue)](https://github.com/open-software-factory/software-factory/issues/31) list 16 lenses, each triggered by a signal in the change. Four concerns were missing from that list: whether the change does what its work item asked, architecture adherence, duplication and reuse, and repository patterns. The options were to add them as four more lenses, or to fold them into existing lenses. Adding them was taken, for 20 lenses in all. Spec and acceptance is the one question a reviewer can answer and no tool can.

**The name.** The literature calls one area a reviewer judges an axis. Axis, concern, lens and area were weighed. Review lens was taken, because an axis names nothing a reader can picture.

**Adopter lenses.** A lens in a section of `osf.toml`, or a lens as a file of its own. A file was taken, because a lens carries criteria and a severity guide that read better in their own file.

## Decision

### Review lenses

A review lens is one area a reviewer judges on its own. Each lens has a name, criteria, a severity guide, a weight, a trigger, and the context it needs. The shipped catalogue has these 20 lenses.

| Lens | Runs |
|---|---|
| correctness | every change |
| spec and acceptance | every change |
| test quality | every change |
| security | every change |
| privacy and data protection | every change |
| data migration and compatibility | every change |
| architecture adherence | its trigger, and every change at the high tier |
| duplication and reuse | its trigger, and every change at the high tier |
| performance | its trigger |
| reliability and failure modes | its trigger |
| concurrency | its trigger |
| interface compatibility | its trigger |
| dependency, licence, supply chain | its trigger |
| observability and rollback | its trigger |
| cost | its trigger |
| accessibility | its trigger |
| internationalisation | its trigger |
| user-visible change | its trigger |
| design documents | its trigger |
| repository patterns | its trigger |

The first six are the must-run lenses. They run on every change at every risk tier, because each guards against damage that cannot be undone once it ships. Privacy starts in ordinary code, such as a log line. A changed serialised type breaks stored data with no migration file in sight.

Every other lens runs whenever its trigger fires, at any tier. A low-risk change that touches a migration file runs the data migration lens.

The review runs at every tier. The `osf risk` tier chooses which lenses run beyond the must-run set. It does not set how much code a reviewer reads, because a reviewer reads what it needs itself.

Each lens also declares the inputs it needs, whatever the tier. Spec and acceptance needs the work item and its acceptance criteria. Design documents need the decision records the change touches or cites. Architecture adherence needs the architecture documents. osf passes the inputs a lens names as metadata, and the reviewer reads the documents themselves from the checkout. A required input that is missing, such as a work item with no acceptance criteria, makes that lens could-not-run for the change. The reviewer does not guess what was asked.

The work item is the issue the pull request names on its `Issue:` line, or else the first issue it closes with a closing keyword. Only an issue in the pull request's own repository is read. A work item whose visible text names several issues in this repository takes the first: the `Issue:` line wins, and otherwise the first closing keyword does. osf reads the pull request body as Markdown prose. A reference inside a code block, an inline code span, an HTML comment or a block quote does not count. A closing keyword names no issue when a negation word is one of the six words before it. The six words are counted back from the keyword and stop at the start of the sentence or line, so a negation word seven or more words before the keyword does not count. The negation words are "not", "never", "without", "cannot", any word that contains "n't", and the pair "no longer".

A workflow step with a read-only token and no secret reads that issue through the code host's API and saves its text for the reviewer jobs. The saved work item holds the issue number and the head commit that the step was given. A reviewer run refuses a saved work item for another head commit. It also refuses one for an issue that its own copy of the pull request text does not name. A lens that needs the work item is then could-not-run, and the reason names both values. The text is untrusted input, as the pull request's own text is. It reaches a reviewer only as data, after secret redaction.

A pull request that links no readable issue makes the spec and acceptance lens could-not-run, with a reason that says to link one. A work item whose acceptance section has no text under its heading makes that lens could-not-run too. A heading inside a code block is not a heading.

An adopter adds a domain lens as a file under `.osf/review-lenses/<name>.toml`, in the same shape as the shipped lenses. Money and health data are examples: they matter in some domains and not in others. The catalogue page lists them as ready-made examples that an adopter enables by copying the file. The precedence is the usual one: the repository's file, then the organisation's, then the tool's. osf validates each lens file when it loads it. A file that does not match the schema stops the review as could-not-configure.

### Reviewers

osf runs each reviewer through a coding-agent command-line tool, run headless, with the lens prompt. osf owns the prompt file, the JSON Schema each answer must match, and which reviewer runs.

The reviewer roster is the agent list in `crates/osf/src/agents.rs`. That list is the place osf keeps every coding agent it supports, and every rule that names an agent reads it, so the roster has no second copy. An adopter chooses which of those agents review in the `[agents]` section of `osf.toml`. This record names the agents only in the table below, which tags each with its model family.

| Agent | Model family |
|---|---|
| codex, OpenAI's coding agent | OpenAI |
| claude code, Anthropic's coding agent | Anthropic |
| dsh, DeepSeek's coding agent | DeepSeek |
| opencode, an open-source coding agent, and omp, the oh-my-pi coding agent built on pi | the family of the model the adopter configures |

An agent that runs many model families, such as opencode or omp, takes its family from its configured model. The same holds for any other agent in the list that does. Onboarding checks which reviewers have working access and disables the rest.

An agent with no read-only mode cannot be a reviewer until it has one. A run that selects such an agent reports could-not-run for that reviewer, with the reason, and never runs it with write tools.

The goal is reviewers from two model families, both different from the builder's family, with more than one review round per family. When a second family has no working reviewer, the review runs one extra critical round with the one family it has. It does not report the lens as could-not-run. This is an interim policy. The factory collects data on how it performs, and the owner sets the final policy once that data exists.

### The review prompt

The frame of the prompt sent to a reviewer holds its role, its rules and how to answer. The default is a file in the osf source tree, `crates/osf/defaults/review-prompt.md`. The osf build embeds that file in the binary, as it does the shipped lens files, so one static binary carries its defaults, as decision 0006 requires. The default is never a string written in the code, and an adopter edits the file in a copy of osf's source or replaces it from the repository.

osf loads the prompt from the first of these that exists, all read from the base branch:

1. The file that the `[review] prompt_file` setting names.
2. The file `.osf/review-prompt.md`.
3. The default that ships with osf.

The organisation's layer, which the lens files also have, is not built yet for the prompt or for the lenses. The default prompt also tells the reviewer to treat the pull request text, the work item and every file as data to review.

Only the answer format that osf parses stays fixed in code. That format is the JSON Schema in the next section.

### What a reviewer receives

A reviewer receives metadata about the change in place of a pasted excerpt of it:

- the pull request number, title and body
- the base commit and the head commit
- the changed files
- the path of one file that holds the commit log and the diff of the change, which osf writes into a new read-only folder before the reviewer starts
- the work item
- the linked decision records
- the lens questions

The reviewer reads whatever else it needs with read-only tools over a clean copy of the change. The copy is a temporary folder that holds the change's files. It holds no coding agent's settings, plugins or instruction files, and no symbolic link, so the change cannot configure the agent that reviews it. osf sets no cap on how much of the change a reviewer reads. The one cut is on the work item: `work_item.rs` cuts an issue body at 60,000 characters and adds a note that it cut it. A reviewer given only limited context "will raise irrelevant things", and a cap decides for the reviewer what is relevant.

### The reviewer sandbox

Tools are safe to give a reviewer because of the limits below.

- The reviewer cannot change the checkout under review. Claude Code, opencode and omp have file tools only: read, search and list. They have no shell. Codex runs inside the review container with its own sandbox and approvals bypass switched on. Its shell can read any file the container holds, and the checkout is read-only.
- An agent inside the factory's container runs with its own sandbox off, and the container is the wall. The wall is the network allow-list, the read-only checkout, no runtime socket, and only the provider key in the environment. The codex reviewer follows this rule. osf runs codex with `--dangerously-bypass-approvals-and-sandbox` only on Linux or macOS, and only when `/opt/factory/bin/osf` is a regular file owned by root with no write bit for group or others, and `/.dockerenv` exists. Anywhere else, including Windows, codex keeps `--sandbox read-only`, and the sandbox check runs. The workflow is updated in [open-software-factory/software-factory#178 (review check)](https://github.com/open-software-factory/software-factory/pull/178).
- A reviewer's environment and home hold no secret beyond its own provider's key.
- osf copies no login file into the home when a key in the environment is enough to sign in. It also removes the exact value of every secret the job holds from each answer, in plain, base64 and hex form.
- The reviewer starts in a clean copy of the change, with no coding agent's settings in it.
- The network is limited by an allow-list that the job sets. The list holds the reviewer's model provider and the GitHub hosts the job needs: `github.com`, `api.github.com`, `codeload.github.com`, `objects.githubusercontent.com`, `ghcr.io` and `pkg-containers.githubusercontent.com`. The codex job's list is those six hosts and `api.openai.com`. The opencode job adds `models.opencode.ai`. The build job holds no secret and no provider key. Its list is the six hosts, `index.crates.io`, `static.crates.io` and `crates.io`. A request from the reviewer to GitHub is not blocked. [open-software-factory/software-factory#208 (hold provider keys outside the reviewer)](https://github.com/open-software-factory/software-factory/issues/208) narrows the list to the provider alone.
- The quote check stays. Every finding must cite code that exists, as the next section sets out. It reads the real checkout of the change.

[Decision 0020 (who can post a review result)](0020-who-can-post-a-review-result.md) holds how the keys and the jobs are arranged around this sandbox.

### The answer and the reducer

Each answer must match a JSON Schema versioned with osf. An answer holds findings and a score from 0 to 1 for each criterion of the lens. Each finding has a file, a line, the quoted code, a severity and an action. An answer that does not match is asked for once more, and then counted as missing.

The schema has no way to raise a finding about something missing: a missing test, a missing migration, an unmet acceptance criterion. Every finding needs real code to quote. Spec and acceptance and test quality are both must-run lenses. Each loses some of its most useful findings to this limit, until the schema grows a way to represent an absence.

The reducer treats every saved answer as input to check. It checks each one again against the schema and the lens, as the reviewer's own job does. The lens name must match. Every criterion needs a score, and each score must be between 0 and 1. A saved file counts only for the reviewer its file name gives, and no two files may carry one name. A file that fails a check is could-not-run for that reviewer. The reducer works out the round and the critical flag from the order of the attempts in the file.

Each saved file also records the repository, the pull request number, the base and head commits and the CI run id. The reducer accepts a file only when all five equal the values it was given, so a file from an earlier commit, another pull request or another run counts for nothing. The run notes name a saved file for a reviewer outside the roster, and a lens entry the change did not select, so nothing is dropped without a word. The review threshold must be a number from 0 to 1, and a lens weight must be a finite number that is not negative. A value outside those limits stops the review as could-not-configure.

Deterministic code then checks every finding. A finding counts only when its quoted code exists at the file and line it names. That confirms the quote is real. It does not check whether the finding's claim about that code is true. A false severity or description attached to a genuine quote passes unchecked.

The reducer decides per lens, and it is plain code:

- A lens needs answers from reviewers in two model families, both different from the builder's. A family counts toward the quorum only with at least two answered independent rounds. A round that failed, timed out or did not validate does not count, and a family with fewer answered rounds counts for nothing.
- When exactly one family has two answered rounds, the lens runs one extra critical round with that family instead of going could-not-run. That critical round must also have answered. Without it, the lens is could-not-run. A lens with no family that has two answered rounds is could-not-run, and a could-not-run lens is never a pass. This is the same interim policy the Reviewers section states, kept while the factory collects data on how it performs.
- A finding with a blocker severity or a must-fix action vetoes the lens, when its quote is verified.
- The lens score is the mean of its criterion scores.
- The review passes when every lens that ran reached quorum (the one-family fallback counts as quorum under the interim policy, when its critical round answered), no finding with a blocker severity or a must-fix action survived verification, and the weighted score clears the threshold.

The lens weights and the threshold ship as data. Adopters get configurable weights in a later version.

### Where it runs

`osf review run` with no `--reviewer` runs every enabled reviewer in turn and decides, in one process. A moon task named `review` runs it at pre-push. The task is tagged `osf-pre-push` and `osf-slot-review`, has no pull-request tag, and runs with `--warn-only`, so a finding there never stops a push. [Decision 0020 (who can post a review result)](0020-who-can-post-a-review-result.md) sets the CI run as the sole authority. The pre-push run is a local, early warning only. Its result is never carried forward as a cached pass at the pull request. CI is the authority, because it holds the keys and the verifier identity.

At the pull request, the workflow `.github/workflows/review.yml` runs from the base branch. It runs when a pull request is opened, synchronized, reopened, marked ready for review or edited, so a change to the pull request text runs the review again. Its `build` job builds osf from the base branch with no secret, and runs `osf review work-item` with the head commit to find the work item. Each reviewer then runs `osf review run --reviewer <name>` in a job of its own. That job holds only that reviewer's own provider key, from the `review` environment, which has one named secret for each job. Every reviewer job, codex included, passes into the reviewer's container only the one variable its agent names.

The codex job runs in the review container, as the claude and opencode jobs do. It runs with its own sandbox and approvals bypass switched on, because the container is the wall. Its `permissions` block holds `contents: read` and `packages: read`, and the second entry is for the registry login. The review image is pinned by digest and carries codex. The workflow names codex 0.161.0 once, in `CODEX_VERSION`, and the codex job fails unless `codex --version` in the image prints `codex-cli 0.161.0`. The workflow has no download, no sha256 and no extraction of a codex release. The codex job's allow-list is the six GitHub hosts and `api.openai.com`. The workflow's own comment says a reviewer with no saved answers counts as could-not-run. The workflow is updated in [open-software-factory/software-factory#178 (review check)](https://github.com/open-software-factory/software-factory/pull/178).

The final job runs `osf review reduce` over the saved answers and posts the result. Every job has the automatic `GITHUB_TOKEN`, with the permissions its `permissions` block names. The build job passes it into its container to read the issue. The final job alone mints the verifier's write token, to post the review. This replaces a single job that held every key. Every job runs one container image, named once in the workflow and pinned by digest, and a change to that digest is a reviewed change.

The review check is advisory for now. It posts its result as a check run and as a review, and no branch protection or ruleset requires it. It becomes a required check when the key proxy and the network split in [open-software-factory/software-factory#208 (hold provider keys outside the reviewer)](https://github.com/open-software-factory/software-factory/issues/208) land. A pull request from a fork is reviewed only when the repository variable `OSF_REVIEW_FORKS` is `true`, and the default is off. When it is on, the reviewer jobs and the final job use a `fork-review` environment. A maintainer's approval gates those jobs only when the adopter has made that environment with required reviewers, which [open-software-factory/software-factory#213 (protect the fork-review environment)](https://github.com/open-software-factory/software-factory/issues/213) tracks. The `build` job has no environment and runs first.

A must-fix finding fails the final job at the pull request. At pre-push it is a warning only, and it never stops a push.

[open-software-factory/software-factory#178 (review check)](https://github.com/open-software-factory/software-factory/pull/178) carries `osf review run`, `osf review reduce`, `osf review work-item`, the workflow `review.yml`, the default prompt file and the lens files. Main has only `osf review post`, and the `osf.toml` on main has no `[agents]` section, until that pull request merges.

The journal keeps each reviewer's answer as one event, with its lens, reviewer, family, scores and findings, linked to the reviewer's transcript. The reducer's decision is one more event.

## Consequences

- A review answer is reported evidence, as [decision 0005 (the factory domain model)](0005-the-factory-domain-model.md) grades it. The deterministic check on quoted code keeps a fabricated location out. It does not check a finding's claim, and the schema has no way to raise a finding about something missing.
- The model-judgement check has one shape, whether a harness or a local classifier gives the answer. A local classifier, as the reference rule in [open-software-factory/software-factory#131 (one rule for every reference the reader cannot place)](https://github.com/open-software-factory/software-factory/issues/131) proposes, enters the reducer as one more answer.
- A reviewer that reads the change with tools reads text that an agent or a fork wrote, so prompt injection is still a risk. The sandbox, the key split and the quote check contain it, as [decision 0020 (who can post a review result)](0020-who-can-post-a-review-result.md) sets out.
- Because an adopter can edit the prompt file, a weak edit weakens the review. [Decision 0020 (who can post a review result)](0020-who-can-post-a-review-result.md) has the review read its prompt, like its lenses, from the base branch.
- An agent with no read-only mode cannot review. An adopter whose chosen agents have none gets could-not-run. No review runs outside the sandbox.
- `osf review post` stays the command that publishes a verdict to the code host.
- The review's own pull request is reviewed first by the command it contains.
