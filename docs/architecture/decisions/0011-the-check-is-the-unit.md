# 0011: The check is the unit, and it runs at five checkpoints

Status: accepted

Date: 2026-09-23

## Context

The verification command this repository ships runs a fixed list of checks in a fixed order at three stages. An adopting repository cannot add a check, drop one, or say where one applies. The stage design in [open-software-factory/software-factory#26 (verify stages as a template of slots)](https://github.com/open-software-factory/software-factory/issues/26) asked for named slots, parallel runs and a cache. The owner asked for something more. An agent should get its findings while it edits, and a gate at the end should never be the first time a check runs.

The full design is [the verification seam](../verification-seam.md). This record holds the decisions about the unit and the points where it runs.

## Options considered

**The unit of verification.**

| Option | What it meant | Why it was set aside |
|---|---|---|
| A stage template file | A file per repository naming every slot, its command, its inputs and whether it blocks. The runner reads it. | It duplicates what a task runner already holds. Inputs, dependencies, hashing and a cache would be written again, in the factory. |
| Checks fixed in the binary, stages as selections | The list stays in code and a stage picks from it. | An adopter cannot add or replace a check without a release. |
| A moon task with tags | Moon, the task runner the execution research chose, already owns inputs, outputs, dependencies, hashing and the cache. A tag names the checkpoints a task runs at and the slot it fills. | Taken. |

**The name for the points where checks run.** Enforcement point, trigger, event and checkpoint were weighed. Event already names a line in the journal. Enforcement point is longer and already names the refusal points in [open-software-factory/software-factory#97 (four enforcement points)](https://github.com/open-software-factory/software-factory/issues/97). Checkpoint was taken.

**Whether moon is required.** Running the factory's checks through moon only where an adopter already has moon, or through moon everywhere. Everywhere was taken. Moon ships in the factory's image, and the factory's tasks live in their own moon project, so the adopter's tree needs no moon of its own.

**The time budget.** A fixed short budget per checkpoint, or no budget with declared inputs and the cache making an untouched check free. No budget was taken. A repository may set a ceiling per checkpoint in its configuration.

## Decision

A check is a moon task. Its tags name the checkpoints it runs at and the slot it fills. Five checkpoints exist. The harness hook, pre-commit, pre-push, the pull request and the schedule. Every checkpoint runs the same way. The tool selects tasks by tag, hands them to moon with the affected filter, and moon runs them in parallel with its cache. Each task writes one verification event, and the tool writes one checkpoint-complete event.

The harness hook checkpoint fires after a tool call that wrote a file and at the end of a turn. It runs every tagged check on the touched files. A check that cannot finish in the hook's time reports skipped with a reason, and the pre-commit checkpoint runs it in full.

The scheduled checkpoint runs the tasks tagged for its cadence. A scheduled check is a check. Its findings become issues, and the engine's own loop works those issues as ordinary changes. Model-driven upkeep, such as documentation refresh or code simplification, is therefore an issue for the engine rather than a check. Two alternatives were set aside. Shipping a code host's agentic workflow files for upkeep, which binds the upkeep to one host and adds a second agent runtime. And one broad maintenance agent on a schedule, whose output has no check behind it.

The factory's own tasks live in a moon project under `.osf/`, rendered from data the tool ships. An adopter's tasks live in the adopter's own moon files and fill slots by carrying the same tags.

## Consequences

- The factory writes no task graph, no hasher and no cache of its own. That stays in moon, as [the execution and verification architecture](../execution-and-verification.md) already says, and that page is amended so the hook checkpoint is one of the points where moon runs.
- The word checkpoint joins the vocabulary in [decision 0003](0003-deterministic-verification-is-authoritative.md).
- An adopter adds a check by adding a tag to a task it already has.
- The refusal of a tool call, such as a commit that skips hooks, stays a separate concern.
