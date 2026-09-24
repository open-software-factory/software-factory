# The verification seam

Status: proposed design, written from the owner's answers in a questioning session on 2026-09-21 and a design session on 2026-09-22. It waits for the owner's review of this file. The decision records listed at the end are written after that review.

Date: 2026-09-22, amended 2026-09-25 with the review check, several tools per slot and gap checks, from a second design session.

The issue for this design is [open-software-factory/software-factory#26 (verify stages as a template of slots)](https://github.com/open-software-factory/software-factory/issues/26). This design replaces the template file that issue proposed with tagged moon tasks and one configuration file. It also serves [open-software-factory/software-factory#97 (four enforcement points)](https://github.com/open-software-factory/software-factory/issues/97) and [open-software-factory/software-factory#50 (fast native git hooks and agent hooks)](https://github.com/open-software-factory/software-factory/issues/50).

## Purpose

One seam runs the same checks at five checkpoints. An agent gets its findings while it edits. A pull request cannot merge until the factory's aggregation check and the adopter's own required jobs all pass. Every run leaves a journal that survives the machine it ran on.

The seam serves the engineer who owns the outcome, running one issue to one pull request unattended. The first adopter is this repository. The second ecosystem is .NET, and Java, TypeScript, Python and Go follow, one at a time.

## Vocabulary

Each term below has one job in this document.

| Term | Meaning |
| --- | --- |
| Check | One unit of verification. A check is a moon task with tags. Moon is the task runner the execution research chose, in [the moon research note](../research/2026-09-18-moon-as-osf-execution-substrate.md). |
| Checkpoint | A point where checks run. There are five: the harness hook, pre-commit, pre-push, the pull request and the schedule. |
| Slot | A named kind of check the factory expects, such as lint, format, unit tests or architecture tests. A slot is filled by a factory default, by the adopter's own task or job, or by a slot attestation. |
| Check recogniser | The part of the tool that reads an adopter's workflow and project files and judges whether the adopter's own check is at least as strong as the factory's for that slot. |
| Slot attestation | The adopter's written statement, in the configuration file, that a slot is filled, with a reason and a date. It is used for a slot no check recogniser covers. |
| Aggregation | The one required check on a pull request. It reads every other check on the commit and posts the result. |
| Journal | The event record the domain model defines, in [decision 0005](decisions/0005-the-factory-domain-model.md). |
| Suppression | A marker or a configuration entry that silences one finding, with a reason and an expiry. |

The words verifier, reporter, policy and gate keep the meanings [decision 0003](decisions/0003-deterministic-verification-is-authoritative.md) gives them. A check is what a verifier runs. A gate is the moment a policy stops the work. The two rules of that decision apply throughout this design. Could-not-read and read-nothing are different facts, and green must be earned.

## Decisions this design rests on

Each row is an answer the owner gave. The decision records at the end carry the options each row weighed.

| Area | Decision |
| --- | --- |
| Unit | A check is a moon task. Its tags name its checkpoints and its slot. Moon runs under every factory check and ships in the factory's image. |
| Name | The points where checks run are called checkpoints. |
| Cost | There is no fixed time budget per checkpoint. Declared inputs and moon's cache make an untouched check free. A repository may set a ceiling per checkpoint in its configuration. |
| Journal | Every checkpoint writes events in the domain model's structure. A local run buffers events and flushes them on push and on a timer. The orphan branch on the code host is the default sink. An object store with an S3-compatible interface, the interface Amazon's object store made common, is an optional second sink. When both are configured, both receive every write. |
| Required checks | The aggregation and the adopter's own jobs are all required. Checks run in parallel and the aggregation runs last. |
| Slots | A slot counts as filled by the adopter's own check only when the check recogniser reads that it is at least as strong as the factory's. Where no recogniser exists, a slot attestation fills it and is reported as such. A periodic audit compares attestations with completed runs. |
| Empty slots | The factory fills an empty slot with its own default when it has one. A slot with no tool, or one only the adopter can fill such as architecture tests, runs a gap check at warning, tracked by an issue in the osf repository, until a tool or the adopter fills it. |
| Several tools per slot | Every task tagged for a slot fills it, and the slot passes only when all of them pass. [Decision 0017](decisions/0017-native-default-checks-and-gap-checks.md) sets this out with the native default tools. |
| Results | A job's conclusion decides pass or fail. Result files add counts and findings, and the tool finds them by content. |
| Reviews | A review is a check with the evidence grade reported. The code host's setting that requires every review thread to be resolved enforces it. [Decision 0016](decisions/0016-the-review-check.md) sets how it runs: review lenses, reviewers from a roster, a JSON Schema for every answer, and a deterministic reducer. |
| Catalogue | The list of checks per ecosystem is generated from the defaults the tool ships. The order is Rust, .NET, Java, TypeScript, Python and Go. The first two ship together. The scheduled checks are an open list that grows. |
| Suppressions | Both the factory's own marker and each ecosystem's native markers. The factory marker carries a reason and an expiry. Native markers keep working for their tools and the factory reads them. |
| Configuration | One table per slot in `osf.toml`. |
| Defaults | One TOML data file per ecosystem inside the tool, holding the task shape, the slot, the checkpoints and the recogniser rules. TOML is the configuration format the tool already uses. |
| Deployment | The factory renders the files it owns into the adopter's repository. Customisation lives in `osf.toml` and in tags on the adopter's own tasks. A drift gate refuses hand edits to a generated file. |
| Aggregation output | One check run whose summary is the slot table, rendered from one journal event. |
| Scheduled work | Scheduled checks are checks. Model-driven upkeep becomes issues that the engine works through its normal loop. |
| Transcripts | Raw harness transcripts share the journal's buffer, flush and sinks, keyed by run identifier. |

## The shape of the seam

A check is a moon task. Its tags say at which checkpoints it runs and which slot it fills. Every checkpoint runs the same way. The tool selects the tasks by tag and hands them to moon with the affected filter. Moon runs them in parallel with its cache. Each task writes one verification event. Then the tool writes one checkpoint-complete event that carries the slot table.

The tasks come from two places. The factory's own tasks live in a moon project under `.osf/`, rendered from the TOML defaults for the repository's ecosystem. The adopter's tasks live in their own moon files and fill slots by carrying the same tags. Moon sees both as one workspace, so one tag selection spans both. A repository with no moon of its own still has the factory's project.

For this repository the factory's project reads like this.

```yaml
# .osf/moon.yml, rendered by the tool
language: rust
tasks:
  scan:
    command: osf scan --format json
    inputs: ['**/*']
    tags: [osf:hook, osf:pre-commit, osf:pre-push, osf:pull-request, osf:slot:secrets]
  lint-writing:
    command: osf lint writing
    inputs: ['**/*.md']
    tags: [osf:hook, osf:pre-push, osf:pull-request, osf:slot:writing]
  fmt:
    command: cargo fmt --all --check
    inputs: ['**/*.rs']
    tags: [osf:pre-commit, osf:pull-request, osf:slot:format]
  clippy:
    command: cargo clippy --workspace --all-targets -- -D warnings
    inputs: ['**/*.rs', 'Cargo.toml', 'Cargo.lock']
    tags: [osf:pre-push, osf:pull-request, osf:slot:lint]
  test:
    command: cargo test --workspace
    inputs: ['**/*.rs', 'Cargo.toml', 'Cargo.lock']
    deps: [clippy]
    tags: [osf:pre-push, osf:pull-request, osf:slot:unit-tests]
```

The pre-commit checkpoint runs the tasks tagged `osf:pre-commit` on the affected files. The `inputs` line is what makes an unaffected task free. A change to one Markdown file leaves the Rust tasks untouched. The slot tag says which slot the task fills, so the aggregation knows the slot is covered.

The pull-request checkpoint has one more piece. The adopter's existing CI jobs fill slots without becoming moon tasks. The check recogniser reads their workflow and project files and judges each slot. The aggregation runs after every workflow on the commit finishes. It reads each job's conclusion and result files, adds the moon results, and posts the one required check.

The schedule checkpoint runs the tasks tagged for it at their cadence. Their findings become issues, and the engine's loop works those issues as ordinary changes.

Every event reaches the local journal first. A buffer flushes on push and on a timer to the orphan branch, and to the object store when one is configured.

### The harness hook checkpoint

The hook checkpoint fires on two harness events. After a tool call that wrote a file, and at the end of a turn. It runs every tagged check on the touched files, so the agent fixes its edits at once. A check that cannot finish inside the hook's time reports skipped with a reason, and the pre-commit checkpoint runs it in full.

The event to refuse a tool call, such as a commit that skips hooks, is a separate concern. It stays with [open-software-factory/software-factory#97 (four enforcement points)](https://github.com/open-software-factory/software-factory/issues/97).

| Harness | After a file write | End of turn | Refuse a tool call | How it is wired |
| --- | --- | --- | --- | --- |
| Claude Code | the post-tool-use event | the stop event | the pre-tool-use event | its settings file |
| Codex | the same three events, in the same file format | same | same | its hooks file |
| dsh | reads the same hooks file | same | same | its bridge to that file, which passes no reply text yet |
| Copilot CLI | reads the same hooks file | unverified | same | its policy directory in the container |
| OpenCode | a plugin on the after-execute event | a plugin | a plugin on the before-execute event | a ten-line plugin |
| omp | a hook under its hooks directory | a hook | a hook | a ten-line hook |

A harness with no end-of-turn event still gets the after-write event, and the pre-commit checkpoint catches the rest.

## Components

Each component has one job and is testable alone.

| Component | Does | Used by | Depends on |
| --- | --- | --- | --- |
| Checkpoint runner | Selects tasks by tag, calls moon with the affected filter, writes one verification event per task and one checkpoint-complete event. | Every checkpoint. | moon, the journal writer. |
| Defaults and renderer | Holds one TOML file per ecosystem. Renders the `.osf/` moon project, the workflow files, the git hooks and the hook wiring. Rendering the same inputs twice changes nothing. | The sync command, the drift gate, the catalogue page. | `osf.toml`. |
| Check recogniser | Reads the adopter's workflow and project files against the recogniser rules and reports each slot as filled, attested or empty. | The aggregation. | The defaults, `osf.toml`. |
| Aggregation | Reads check conclusions and result files on a commit, adds the moon results and the recogniser's slot states, applies levels and suppressions, posts one check with the slot table. | The pull-request workflow. | The code host adapter, the result-file readers, the journal writer. |
| Result-file readers | Detect a result file by content and return counts and findings. One reader per format. | The aggregation. | Nothing else. |
| Journal writer and sinks | Validates and appends events, hash-chained. Buffers locally. Flushes to the branch, and to the store when configured. | Every component that emits an event. | The state directory, git, an object-store client when configured. |
| Suppression reader | Parses factory markers and native markers, checks reason and expiry, counts them. | The checkpoint runner, the aggregation. | Nothing else. |
| Hook adapters | One small shim per harness that calls the checkpoint runner on the right event. | The harness. | The checkpoint runner. |
| Git hooks | Pre-commit and pre-push scripts that call the checkpoint runner. | git. | The checkpoint runner. |

The existing `osf verify` command becomes the checkpoint runner's entry point, with the checkpoint as its argument. A sync command renders. The shapes of the runner, the recogniser rules and the result readers get their detail in the implementation plan.

## Data flow

One change, followed from the first edit to the merged pull request.

1. **The agent writes a file.** The harness fires its after-write event. The hook adapter calls the runner with the changed path. The runner selects the tasks tagged for the hook checkpoint. Moon's affected filter drops every task whose inputs the path does not touch. What remains runs. Findings return to the agent in the hook's reply. Each task's verification event lands in the local buffer.
2. **The agent commits.** The pre-commit git hook runs the pre-commit checkpoint on the staged files. A failing error-level task refuses the commit. The commit message passes the writing lint.
3. **The agent pushes.** The pre-push checkpoint runs on the whole branch diff. Then the buffer flushes to the orphan branch, and to the object store when one is configured.
4. **The pull request opens.** The adopter's own workflows and the generated factory workflow start together. The factory workflow runs the tasks tagged for the pull-request checkpoint. Each job uploads its result files as artifacts.
5. **A workflow finishes.** The aggregation workflow starts. It lists the check runs on the commit. If one is still running, it stops and waits for the next finish. When every check is done it reads each conclusion and downloads the artifacts. It hands each file to the reader that recognises its content. The check recogniser reports the state of each slot. Levels and suppressions apply. The aggregation writes the checkpoint-complete event with the slot table, posts the one required check, and flushes the journal.
6. **Merge.** The required check, the adopter's jobs and the resolved review threads gate the merge.
7. **On the schedule.** The scheduled workflow runs the tasks tagged for that cadence. Each finding becomes an issue with the native fields set, and the engine's loop takes it from there.
8. **On a factory release.** The daily sync task sees the new version and renders the generated files. It opens a pull request under the builder identity. That pull request goes through the pull-request checkpoint like any other.

### The records

A verification event carries the check name, the slot, the checkpoint, the result, the duration, the cache outcome, the finding count and the evidence grade. A checkpoint-complete event carries the checkpoint, the commit, the slot table and the aggregate result. A slot table row carries the slot, how it was filled, the result, the counts, the grade and the source that supplied it. Findings with a location are written in SARIF, the static-analysis results interchange format, as the domain model already says.

### Raw transcripts

Raw harness transcripts share the journal's buffer, flush and sinks, keyed by run identifier. The run-started event records the transcript's location, and the run-complete event records its hash. Files are stored as written, and scrubbed of secrets before the flush. The collection cadence, the scrub rules, the per-harness readers and the query dataset are a later design of their own.

## Configuration

### The repository file

One table per slot. Everything the factory needs to know about a slot sits under that slot's name. The moon tasks keep their tags, so the file never repeats what moon already holds.

```toml
# osf.toml, in an adopter's repository
ecosystem = "dotnet"

[slot.lint]
job = "Build"                 # the check recogniser reads this job and the project files

[slot.unit-tests]
job = "Unit Tests"
results = "**/*.trx"          # optional, narrows the content scan

[slot.integration-tests]
job = "Integration Tests"

[slot.architecture-tests]
level = "error"               # raised from the warning default

[slot.contract-tests]
job = "Integration Tests"
attested-by = "the suite replays recorded upstream responses"
attested-on = 2026-09-22

[[suppress]]
check = "writing-lint"
rule = "long-sentence"
path = "docs/legal/**"
reason = "licence text is quoted as written"
until = 2027-03-31

[journal]
branch = "osf-journal"
```

An organisation-wide file with the same shape sits above the repository file, and the repository file overrides one key at a time. The precedence is the one [the page on how the factory reaches a repository](how-the-factory-reaches-a-repository.md) sets. The repository's own file, then the organisation's, then the tool's defaults.

### The shipped defaults

One TOML file per ecosystem, inside the tool. Each entry holds the moon task shape, the slot, the checkpoints and the recogniser rules. The recogniser rules come from a small fixed set of predicate kinds. A project property with a value. A package reference. A workflow step that runs a command. A file that exists. A rule that needs a new kind adds the kind to the tool once, with its tests.

```toml
# dotnet.toml, inside the tool
[[check]]
slot = "format"
task = { command = "dotnet format --verify-no-changes", inputs = ["**/*.cs", "**/*.csproj", ".editorconfig"] }
checkpoints = ["pre-commit", "pull-request"]
recognise = [
  { workflow-step-runs = "dotnet format" },
  { workflow-step-runs = "--verify-no-changes" },
]

[[check]]
slot = "lint"
task = { command = "dotnet build -warnaserror", inputs = ["**/*.cs", "**/*.csproj", "Directory.Build.props"] }
checkpoints = ["pre-push", "pull-request"]
recognise = [
  { msbuild-property = "TreatWarningsAsErrors", value = "true" },
  { msbuild-property = "AnalysisLevel", value = "latest-all" },
  { package-reference = "StyleCop.Analyzers" },
]
```

A plain build and test run fills none of the stronger slots. For .NET the lint slot wants analysers at the latest level with warnings treated as errors, plus a style analyser package. The format slot wants a format check that fails on drift. The architecture slot wants a test project that references an architecture-testing library. The integration slot wants a test project marked as integration by trait or by name. Each of those is a fact in a project file, a props file or a workflow file. The check recogniser reads those files and nothing else on a pull request.

The catalogue page is rendered from the same files in CI, so the list has one source and the page cannot drift.

## Deployment into a repository

The core stays in the image and never ships into a product repository. The surface is vendored into each repository and a drift gate checks it, as [the page on how the factory reaches a repository](how-the-factory-reaches-a-repository.md) already says. This design fixes what the surface holds for checks.

The tool renders a small set of files from the TOML defaults and the repository's `osf.toml`.

| Generated file | Holds |
| --- | --- |
| `.osf/moon.yml` | The factory's moon project with its tagged tasks. |
| `.osf/hooks/pre-commit`, `.osf/hooks/pre-push` | The git hooks. The sandbox points git's hooks path here. |
| The factory's pull-request workflow | Runs the tasks tagged for the pull-request checkpoint and uploads their results. |
| The aggregation workflow | Runs after every workflow on the commit finishes and posts the one required check. |
| The scheduled workflow | Runs the tasks tagged for each cadence. |
| The sync workflow | Checks daily for a new factory release and opens the sync pull request. |

Each generated file carries a header naming the version that produced it. Rendering is a pure function of the version and `osf.toml`, so running it twice changes nothing. An adopter customises in two places only. Tags on their own moon tasks fill slots, and keys in `osf.toml` set levels, jobs, attestations and suppressions. The drift gate fails when a generated file differs from what the pinned version renders. Its message names the key or the tag where the edit belongs instead.

The hook wiring for each harness is rendered into the sandbox's managed harness settings, because the harness reads it from there. The sync command reports which harnesses are wired.

## The aggregation

The aggregation is the one required check that reads every other check on the commit and decides. It runs as a workflow in the adopter's repository. A job on the first code host can wait only on jobs inside its own workflow, and an adopter's checks may span several workflows. It re-runs each time a workflow finishes, so it converges without polling. It reads the check runs on the commit through the repository's own token, and it posts its result as one check.

Each check still writes its own verification event. When the aggregation finishes, it writes one checkpoint-complete event that carries the slot table as data. The check run's summary on the code host is a rendering of that event. One source, two views. The pull-request status block gets one line per slot only when the slot's state differs from the base branch. A quiet pull request shows nothing new.

| Slot | Filled by | Result | Findings | Grade |
| --- | --- | --- | --- | --- |
| lint | this repository's Build job, check recogniser confirmed | pass | 0 | observed |
| unit-tests | this repository's Unit Tests job | pass | 412 tests, 0 failed | observed |
| architecture-tests | gap check, tracked by its issue | warning | | |
| contract-tests | slot attestation, 2026-09-22 | pass | | reported |
| review | six must-run lenses and two triggered lenses, two model families each | 1 blocker, verified | 1 | reported |

The result-file readers detect a file by content. The formats they read are JUnit XML, TRX, xUnit XML, Cobertura, LCOV, JaCoCo, SARIF and CTRF. JUnit XML is the test-result format most runners can write. TRX is the .NET test-result format. The coverage formats are Cobertura, LCOV and JaCoCo. JaCoCo is the Java coverage tool's own format. CTRF is a common test-report format in JSON. A glob in `osf.toml` narrows the scan. A job with a known conclusion and no readable file still counts as passed or failed.

A review is a check with the evidence grade reported. The code host's setting that requires every review thread to be resolved enforces it, and a policy never merges on reported evidence alone.

## The review check

`osf review run` reviews a change through review lenses. A review lens is one area a reviewer judges on its own, such as security or data migration, with its own criteria and severity guide. The review runs on every change, as a moon task tagged for pre-push and for the pull request.

- Six must-run lenses run on every change: correctness, spec and acceptance, test quality, security, privacy and data protection, and data migration and compatibility.
- Every other lens runs whenever its trigger fires, at any risk tier.
- The `osf risk` tier sets how much code each reviewer reads. At the high tier, architecture adherence and duplication and reuse also run on every change.
- Each lens declares the context it needs, such as the work item and its acceptance criteria. A missing required input makes that lens could-not-run.
- An adopter adds a domain lens, such as money or health data, as a file under `.osf/review-lenses/`.

osf runs each reviewer through a coding-agent command-line tool, from a roster of harness and model pairs. Every answer must match a JSON Schema shipped with osf. Deterministic code keeps a finding only when its quoted code exists at the file and line it names. A reducer decides per lens: two model families for quorum, a verified blocker vetoes, and a weighted score must clear a threshold. Too few answers is could-not-run. A must-fix finding sends the change back to the coding agent before the pull request.

[Decision 0016](decisions/0016-the-review-check.md) holds the full catalogue, the roster and the reducer rules.

## The scheduled checkpoint

A scheduled check is a moon task tagged for the scheduled checkpoint, with a cadence tag such as daily or weekly. It runs in the generated scheduled workflow, writes its verification event, and its findings become issues with the native fields set. The engine's own loop, one issue to one pull request, picks those issues up under the selection policy. So a documentation-drift check finds the drift and raises the issue, and the engine fixes it as ordinary work. Checks stay deterministic and every model-driven change goes through the same pull-request checkpoint as any other change.

Three kinds of check belong here first. The audit that compares slot attestations with completed runs. Dependency and vulnerability checks whose inputs change without a commit. Trend checks over the journal, such as test-count shrink and check duration growth. The list is open. The next candidates are mutation testing, file-size growth, duplicate-code detection against a committed baseline, stale-branch cleanup and post-deploy smoke tests. Each enters as a data entry in the shipped defaults.

## Suppressions

A suppression silences one finding in place, with a reason and an expiry, and the journal counts it. The factory marker sits on the line above the finding, in the file's own comment syntax, and names the rule, the expiry and the reason.

```rust
// osf:suppress clippy::too_many_arguments until=2027-03-31 reason="mirrors the wire format"
```

```markdown
<!-- osf:suppress long-sentence until=2027-03-31 reason="licence text quoted as written" -->
```

A marker without an expiry or a reason is itself a finding. An expired marker is a finding. The marker's fields are the same as a `[[suppress]]` entry in `osf.toml`, so one parser reads both.

A suppression the ecosystem's own tool understands keeps working for that tool, and the factory reads it. A Rust allow attribute and a Python noqa comment are two such forms. A suppression the team already has at adoption stays in force and is counted. A native suppression that a change adds is a finding for review, which [decision 0003](decisions/0003-deterministic-verification-is-authoritative.md) already requires.

## When things go wrong

| What fails | What happens | What the journal records |
| --- | --- | --- |
| `osf.toml` does not parse, or a slot attestation lacks its reason or date | Every checkpoint refuses to run and prints the parse error. There is no silent fallback to defaults. | A checkpoint-complete event with the result "could not configure". |
| moon is missing or the wrong version | The checkpoint fails as an infrastructure failure. The tasks are marked "could not run". | Verification events with "could not run" and the reason. A pass is impossible. |
| A task cannot finish inside the hook's time | The task reports skipped with the reason. The pre-commit checkpoint runs it in full. | A verification event with "skipped" and the time limit. |
| An adopter's job did not run because its path filter excluded the change | The aggregation reads the filter and the diff. When the diff touches none of the paths, the slot is "not affected" and passes with that note. | The slot row says "not affected by this change". |
| An adopter's job was cancelled, or its check run is absent for any other reason | The slot is unknown. The aggregation fails. | The slot row says "could not read", with the job name. |
| A workflow or project file the check recogniser needs does not parse | The slot is unknown, at error level. | The slot row says "could not read", with the file and the error. |
| A result file is missing or unreadable for a slot that expects one | Warning on the first pull request, error once the adopter confirms the slot. The conclusion still decides pass or fail. | The slot row carries the conclusion and the note "no readable results". |
| A test count shrinks between the base and the change | A finding for review, as the existing rule says. | A finding with both counts. |
| A suppression has expired, or a change adds a native suppression | A finding. | A finding naming the marker and its expiry. |
| A generated file was edited by hand | The drift gate fails and its message names the key in `osf.toml` or the tag on a moon task where the change belongs. | A verification event from the drift check. |
| Rendering would overwrite a file the adopter already has under the same name | The sync command refuses and names the file. | Nothing. The sync did not run. |
| The flush cannot reach the branch or the store | Locally the buffer keeps the events and the next flush retries. In the pull-request checkpoint the aggregation fails, because evidence must be durable before the state changes. | Locally, a gap event at the next successful flush. In CI, the failed aggregation. |
| Two aggregation runs start on the same commit | A concurrency group per commit lets one run at a time, and the later one supersedes. | One checkpoint-complete event per commit. |
| The harness sends no text in its hook reply, as one bridge does today | The findings still reach the journal, and the pre-commit checkpoint refuses the commit with them. | The verification events, unchanged. |

## Testing

Every component has its own tests, and this repository proves the whole by running the seam on itself.

| Component | Test | Passes when |
| --- | --- | --- |
| Defaults and renderer | Render twice from the same inputs. Change one key in `osf.toml` and render again. Validate every ecosystem file against its schema. Regenerate the catalogue page. | Identical bytes the first two times. Only the expected file changes the third. Every file valid. The page equals the committed one. |
| Checkpoint runner | A fixture repository with three tagged tasks. Change one file. Run the same checkpoint twice. Remove moon. Make one task fail. | Only the affected task runs. The second run is all cache hits with no tool started. The missing moon gives "could not run". The failing task gives "failed" and a non-zero exit. |
| Check recogniser | A corpus per ecosystem of workflow and project files, one passing and one failing case per rule. | Each rule reports filled on its positive case and empty on its negative case, and only those. |
| Result-file readers | A corpus of real result files per format, stored without extensions. | Each file is detected by content and its counts match the known values. |
| Aggregation | Recorded code-host responses for each row of the failure table. All pass. A cancelled job. A path-filtered job. A missing artifact. A run that starts while another is running. | The slot table as data matches the expected table, and the rendered summary matches its snapshot. |
| Suppression reader | Markers in every comment syntax the ecosystems use. An expired marker. A marker with no reason. A native marker added in a change. | Each is read, and the last three become findings. |
| Journal and sinks | Replay a run. Cut the network during a flush. Complete a run with a transcript. | The replay reproduces the head hash. The buffer keeps its events and the next flush writes a gap event. The run-complete event carries the transcript hash. |
| Hook adapters | In each harness's container, write a file, end a turn, and commit with the skip flag. | The runner is called on the first two, and the third is refused. The bridge that sends no text still yields journal events. |
| Drift gate and sync | Edit a generated file by hand. Tag a new factory release. | The gate fails and names where the edit belongs. The sync pull request opens unaided under the builder identity. |
| The seam on itself | This repository runs every checkpoint on its own changes, from the first pull request that lands the seam. | Its own pull requests carry the slot table. |
| The second ecosystem | A fixture repository in .NET with existing jobs that fill slots. | Its slot table shows the lint, unit-test and integration-test slots filled by its own jobs, the architecture slot running its tracked gap check, and a deliberate writing-lint failure refused at pre-commit. |

## What this changes in existing documents

- [The execution and verification architecture](execution-and-verification.md) says the factory invokes moon only at lifecycle checkpoints. The hook checkpoint changes that, and the page is amended.
- [Decision 0003](decisions/0003-deterministic-verification-is-authoritative.md) gains the word checkpoint in its vocabulary.
- [Decision 0009](decisions/0009-journal-store-and-sinks.md) is provisional. It gains the local buffer, the flush, the orphan branch as the default sink and the object store as the optional one.
- [open-software-factory/software-factory#26 (verify stages as a template of slots)](https://github.com/open-software-factory/software-factory/issues/26) is updated to point at this design.

## The decision records to write

Each one records the options weighed and the option taken.

| Record | What it settles |
| --- | --- |
| The check is the unit | Tagged moon tasks, five checkpoints, the name checkpoint, moon under every factory check, no fixed budget. |
| Slots, check recognisers and slot attestations | At least as strong, the level of an empty slot, the slot tables in `osf.toml`. |
| The aggregation check | Parallel checks with one final check, run in the adopter's repository, results found by content, reviews as reported checks. |
| The journal at every checkpoint | Local buffer, flush on push and on a timer, orphan branch and object store as sinks, transcripts on the same path. |
| Suppressions | The factory marker and the native markers, each with a reason and an expiry where the form allows. |
| The review check | Review lenses, the must-run set, adopter lenses, the reviewer roster, the answer schema and the reducer. |
| Native default checks and gap checks | Native tools per slot, several tasks per slot, candidates and a default pick, gap checks tracked by an issue, qlty as a candidate. |

## Later

- The object store sink, as a fast follow after the orphan branch.
- The four ecosystems after .NET, one at a time, each driven by a real repository.
- The raw transcript archive's internals.
- The selection policy that chooses which issue the engine works next.
- The confidence model over check results, once the journal holds enough runs to measure one.
