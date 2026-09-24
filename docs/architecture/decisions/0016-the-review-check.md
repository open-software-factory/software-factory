# 0016: The review check, review lenses and the reducer

Status: accepted

Date: 2026-09-25

## Context

[Decision 0013](0013-the-aggregation-check.md) made a review one check at the pull-request checkpoint, with the evidence grade reported. It did not say how the review runs, what it reads, what shape its answer takes, or how several answers become one verdict.

The first branch built on the verification seam showed why that matters. Every review of it came from one model family, and the same agents wrote its code and its tests. The branch passed every deterministic check and still carried defects that a person found by asking. One was a run where every check was skipped and the checkpoint still passed.

The design is [the verification seam](../verification-seam.md). The research is [model reviews checked by a deterministic reducer](../../research/2026-09-25-verifier-and-reducer-review-patterns.md) and [change risk classification](../../research/2026-09-17-change-risk-classification.md).

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

The review runs at every tier. The `osf risk` tier sets its depth. A low tier reads the diff. A normal tier reads the diff and its callers. A high tier reads the whole affected module.

Each lens also declares the context it needs, whatever the tier. Spec and acceptance needs the work item and its acceptance criteria. Design documents need the decision records the change touches or cites. Architecture adherence needs the architecture documents. osf assembles that context. A required input that is missing, such as a work item with no acceptance criteria, makes that lens could-not-run for the change. The reviewer does not guess what was asked.

An adopter adds a domain lens as a file under `.osf/review-lenses/<name>.toml`, in the same shape as the shipped lenses. Money and health data are examples: they matter in some domains and not in others. The catalogue page lists them as ready-made examples that an adopter enables by copying the file. The precedence is the usual one: the repository's file, then the organisation's, then the tool's. osf validates each lens file when it loads it. A file that does not match the schema stops the review as could-not-configure.

### Reviewers

osf runs each reviewer through a coding-agent command-line tool, run headless, with the lens prompt. osf owns the prompts, the JSON Schema each answer must match, and which reviewer runs.

The reviewers come from a roster. The shipped roster lists these harness and model pairs, each tagged with its model family.

| Harness | Model family | Access |
|---|---|---|
| codex | OpenAI | subscription |
| Claude Code | Anthropic | subscription |
| dsh | DeepSeek | API key |
| opencode | GLM, Kimi, Qwen and MiMo | API key |

An adopter changes or extends the roster in configuration. Onboarding checks which reviewers have working access and disables the rest. The ideal is reviewers from several families, all different from the builder's family. One family is allowed, with a warning, so adoption stays easy.

### The answer and the reducer

Each answer must match a JSON Schema versioned with osf. An answer holds findings and a score from 0 to 1 for each criterion of the lens. Each finding has a file, a line, the quoted code, a severity and an action. An answer that does not match is asked for once more, and then counted as missing.

Deterministic code then checks every finding. A finding counts only when its quoted code exists at the file and line it names.

The reducer decides per lens, and it is plain code:

- A lens needs answers from reviewers in two model families. With fewer, the lens is could-not-run, and a could-not-run lens is never a pass.
- A verified blocker vetoes the lens.
- The lens score is the mean of its criterion scores.
- The review passes when every lens that ran reached quorum, no blocker survived verification, and the weighted score clears the threshold.

The lens weights and the threshold ship as data. Adopters get configurable weights in a later version.

### Where it runs

`osf review run` is one command. It runs as a moon task tagged for pre-push and for the pull request, so moon's cache reuses the pre-push result at the pull request when nothing changed. CI is the authority, because it holds the keys and the verifier identity. The local run gives the coding agent the same feedback earlier.

A must-fix finding sends the change back to the coding agent before the pull request.

The journal keeps each reviewer's answer as one event, with its lens, reviewer, family, scores and findings, linked to the reviewer's transcript. The reducer's decision is one more event.

## Consequences

- A review answer is reported evidence, as [decision 0005](0005-the-factory-domain-model.md) grades it. The deterministic check on quoted code is what keeps a fabricated finding out.
- The model-judgement check has one shape, whether a harness or a local classifier gives the answer. A local classifier, as the reference rule in [open-software-factory/software-factory#131 (one rule for every reference the reader cannot place)](https://github.com/open-software-factory/software-factory/issues/131) proposes, enters the reducer as one more answer.
- `osf review post` stays the command that publishes a verdict to the code host.
- The review's own pull request is reviewed first by the command it contains.
