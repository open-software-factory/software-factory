# The smallest working engine

Status: design. The boundary is [decision 0008](decisions/0008-the-smallest-working-engine.md). Date: 2026-09-20.

The smallest working engine is the smallest version of the engine that does one useful job from end to end. This document says what that job is, what stays outside it, which components do it, what passes between them, and which decisions it settles. The work items that build it live in the tracker.

## The job

The engine takes one ready work item and returns one pull request that is verified, reviewed, and ready for a person to merge. No person acts between the start and the pull request unless the engine asks.

| Step | What happens | What is recorded |
|---|---|---|
| Pick | The engine reads one work item the tracker marks as ready, with its title, body, repository and dependencies. | The work item, provider-qualified, and the fact that it was read. |
| Start | The engine starts the default sandbox on the local platform, opens a branch and a worktree for the work item, and starts a run. | A run-started event with the actor: the coding agent, its model, and its model family. |
| Implement | The coding agent works inside the sandbox on the work item, with the repository's own skills and rules composed in. | The agent's session identifier, its cost when known, and the change: branch and head commit. |
| Verify | The engine runs the repository's deterministic checks through `osf verify`. | One verifier-run event per check, with command, tool version, exit status, timing, and whether it ran at all. Located findings in SARIF. |
| Review | A second harness, of a different model family, reads the change and posts findings. | A review-run event with the round number and the scope it read, and one finding event per finding. |
| Report | The engine opens the pull request, writes the status block, and links the run's evidence. | The state change to in review, and the run-complete event carrying the journal's head hash. |
| Hand over | The work item waits for a person, or the engine loops once more on the review's must-fix findings. | Attention raised when the item needs a person. |

The policy is [decision 0003](decisions/0003-deterministic-verification-is-authoritative.md) as written. A change with no declared test command is held. A shrinking test count is a finding. A suppression added in the change is a finding. A check that could not run is recorded as a failure to run, and no such record counts as a pass. Merging stays with a person in this slice.

## What stays outside

These are named so that the engine is judged on the smallest-working-engine job and on nothing else.

| Outside | Why it waits |
|---|---|
| Deployment, containment and recovery | The domain model names them, and nothing in the first job reaches production. |
| More than one work item at a time, scheduling, and remote platforms | The first job proves the model on one item in one sandbox. Parallelism is the second slice. |
| Merging without a person | The gate that allows it needs the calibration the risk-classification work describes. |
| The console's own features beyond reading the journal | The console is a projection of the journal. The first job produces the journal and the queries; the first console view reads them and is its own work. |
| Cost accounting beyond what the harness reports per run | Money and tokens are recorded when the harness gives them. Attribution across runs is meta-loop work. |
| The agent-host protocol gateway | [Decision 0004](decisions/0004-protocol-independent-core-with-ahp-acp-edges.md) keeps it at an edge. The first job correlates a harness session by its identifier and nothing more. |
| Canonical verification through moon | moon is a build task runner. The execution design adopts it for lifecycle checkpoints. The first job uses `osf verify` as it exists, and the verifier runner is the one place moon plugs in later. |

## Components

The engine is a core with providers at its edges, per [decision 0002](decisions/0002-provider-neutral-process-boundaries.md). Each row names the first implementation and whether it is core or a provider.

| Component | Responsibility | First implementation | Kind |
|---|---|---|---|
| Tracker adapter | Read ready work items with their fields and dependencies. Write the work item's state and a recap back. | GitHub Issues and Projects through the API. | Provider |
| Sandbox provider | Start the default sandbox on the local platform, with the repository and the state directory mounted, and run the engine's command in it. | The default container, through the container runtime's command line. | Provider |
| Harness adapter | Run one coding agent on one task without a person, collect its result, its session identifier and its cost. | The installed agents' unattended modes as subprocesses, behind one interface with a capability report. | Provider |
| Verifier runner | Run the repository's deterministic checks and turn each into a verifier-run event and findings. | `osf verify`, with ecosystem detection. | Core |
| Reviewer | Run one review round by a harness of a different model family and post the findings. | `osf review`, extended to record a review-run event. | Core, over a harness adapter |
| Forge adapter | Branch, pull request, status block, review comments, check status. | GitHub through the API, from the existing status and review code. | Provider |
| Policy | Read the run's evidence and decide: advance, hold, or raise attention. | The rules of [decision 0003](decisions/0003-deterministic-verification-is-authoritative.md) and the risk tier from `osf risk`. | Core |
| Journal | Append every event to the run's journal, hash-chained, and validate every event against the schema. | JSON Lines under the state directory, per [decision 0009](decisions/0009-journal-store-and-sinks.md). | Core |
| Sink | Copy a completed run's journal to where other runs and the console can read it. | The local sink: the state directory itself. The forge-native sink follows. | Provider |
| Projections and queries | Compute the work item's state, the attention list and the recorder from the journal, on read, for the command line and the console. | `osf work`, `osf run` and `osf attention` subcommands. | Core |

## What crosses between components

| Data | Shape | Produced by | Read by |
|---|---|---|---|
| Work item reference | Provider-qualified identifier, `github:owner/repo#N`, with title, body, repository, dependencies and the ready mark. | Tracker adapter | Everything. It is the durable unit's name. |
| Run identifier | One identifier per run, with the actor and the work item. | Journal | Harness, verifier runner, reviewer, forge adapter, policy. |
| Change | Branch name and head commit. | Harness adapter | Verifier runner, reviewer, forge adapter. |
| Event envelope | [Decision 0005](decisions/0005-the-factory-domain-model.md)'s envelope: schema version, event type, run, work item, change, actor, timestamp, cost, payload, previous hash. | Every component, through the journal | Policy, projections, sink, console. |
| Findings | SARIF for a located finding. A test result on its own path. | Verifier runner, reviewer | Policy, forge adapter, projections. |
| Verdict | Advance, hold with a reason, or attention with a cause. | Policy | Forge adapter, tracker adapter, projections. |
| Recap | A short text: what the run did, what it found, what it needs. | Projections | Tracker adapter, forge adapter, console. |

The event schema is the first contract written, before any component, and it lives beside the code that emits it.

## The work item lifecycle in this slice

The states are [decision 0005](decisions/0005-the-factory-domain-model.md)'s. This slice uses the ones the job reaches.

| State | Entered when |
|---|---|
| ready | The tracker marks the item ready. The engine reads it. |
| in progress | The run starts. |
| verifying | The agent's change exists and the verifier runner starts. |
| in review | The review round posts and the pull request opens. |
| blocked, with a cause | The agent asks a question a person must answer, a dependency is unmet, or a hold needs a person. The cause is one of: dependency, human, clarification, ambiguous, capacity. |
| failed | The run ends without a change, or the verifier runner could not run. |

Deploying, deployed, signed off, recovering, aborted, rolled back and paused stay defined and unused until a later slice reaches them.

## Attention and recaps

The operator is defined in [product decision 0002](../product/decisions/0002-operator-persona-and-product-vocabulary.md). They look at the factory whenever they choose. They are never expected to watch it. So the engine owes them two things on every arrival: what needs them, and what happened since they last looked.

Both are projections of the journal. The attention list is every work item in a blocked state plus every run the policy held, ordered by the cause and the age. The recap is the recorder for one work item, folded to its state changes and its findings, with a route from each line to the raw event.

The engine writes the recap to the work item as a comment when the run ends, so the operator sees it in the tracker without opening anything else. The same text is what `osf work show` prints and what the console's recorder shows.

## The five vocabulary questions, answered on paper

The [draft vocabulary](draft-factory-vocabulary.md) left five questions for this engine. Each is answered here against [decision 0005](decisions/0005-the-factory-domain-model.md) and this design. Each answer is confirmed or amended against real runs by the work item that names the event vocabulary once the engine runs.

| Question | Answer |
|---|---|
| Does the durable unit map onto work item, agent session, check, deployment and event, or does it need attempt or run? | The durable unit is the work item, which is the tracker's issue. A run is one execution by one actor. A retry is another run that names the run it retries. An agent session is the harness's own identifier, recorded on the run and never a factory entity. A check is a verifier run. A deployment is outside this slice. |
| Is attention an entity the engine owns, or a projection the console derives? | Both halves are true and neither is an entity store. The engine emits an attention event when a policy or a state change needs a person. The attention list is a projection of those events and the blocked states. There is no hand-maintained attention table. |
| Is the catalog and state split still natural when state comes from a real provider? | The split is retired. The journal is the state. What the draft called catalog is what the tracker and the forge say at run start, and the run-started event records it. A snapshot is the projection of events up to a moment, so it holds no future by construction. |
| Do trace links need a relation vocabulary this small, or a richer one? | The seven edges of [decision 0005](decisions/0005-the-factory-domain-model.md) are the vocabulary: executes, depends on, blocks, produces, verifies, deploys to, traced from. The draft's five map onto them: informs is traced from, implements is produces, verifies is verifies, deploys is deploys to, and observes waits for production evidence. A richer vocabulary is added when an edge is needed that these cannot express. |
| Do the six availability values survive contact with real disconnects and partial data? | They leave the engine and stay in the console. The engine exposes the journal's freshness: the time of the last event and whether the sink can be read. The console derives ready, loading, stale, disconnected and empty from that, and streaming from a live run. |

## Decisions this design settles

| Decision | Record |
|---|---|
| The boundary of the smallest working engine, in and out, and what done means | [0008](decisions/0008-the-smallest-working-engine.md), accepted |
| Where the journal lives and how it reaches readers | [0009](decisions/0009-journal-store-and-sinks.md), provisional |
| How the first tracker, sandbox and harness adapters are built | [0010](decisions/0010-first-tracker-sandbox-and-harness-adapters.md), provisional |
| Who the operator is and which words the product uses | [product 0002](../product/decisions/0002-operator-persona-and-product-vocabulary.md), accepted |

Decisions the design leaves open, each tracked as a work item: when a change may merge without a person, how many work items run at once and where, which sink follows the local one, and when the verifier runner moves to moon.

## Done means

- One ready work item in a real repository goes from ready to in review with no person acting between.
- The pull request carries the status block, the check results and the review, and links the run.
- The run's journal validates against the schema, its head hash is reproducible from its events, and `osf work show` prints the recap from it.
- A blocked run raises attention, writes its cause and its recap to the work item, and stops.
- Every check in the run can be re-run from the same inputs and gives the same result.

## The order of work

The event schema and the journal come first, because every other component writes to them. Then the tracker adapter and the forge adapter, which the existing status and review code already half-provide. Then the sandbox provider and the harness adapter, which are new. Then the policy and the projections. The end-to-end run is the last item and the acceptance test of all of them.
