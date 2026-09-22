# 0014: The journal at every checkpoint

Status: accepted. Amends [decision 0009](0009-journal-store-and-sinks.md).

Date: 2026-09-23

## Context

Decision 0009 puts a run's journal in one file under a state directory and names a local sink first, with a forge-native sink to follow. It does not say how a checkpoint on an agent's own machine gets its events off that machine, or what a hook costs when it has to wait on a network. The owner asked that everything log, that the journal be mineable, and that a local run's journal survive the machine. The design is [the verification seam](../verification-seam.md).

## Options considered

**When a local run ships its journal.** On every run, or from a local buffer that a flush sends. The buffer was taken. A hook runs on every prompt and every commit, and a network call on that path slows the agent. The pre-push hook flushes the buffer, and the local loop flushes it on a timer. A hook never waits on the network.

**Which sink holds the off-machine copy.**

| Option | What it meant | Outcome |
|---|---|---|
| The orphan branch on the code host | The journal is pushed to a branch that shares no history with the code. It needs only the repository's own git credentials, which every adopter has. | Taken as the default. |
| An object store with an S3-compatible interface | The interface Amazon's object store made common. It suits retention and queries across repositories. | Taken as an optional second sink and a fast follow. An adopter configures it only when they want one, so adoption needs no cloud account. |
| Both | Each configured sink receives every write. | Taken when both are configured. |

**Raw harness transcripts.** A separate collection path of their own, or the journal's path. The journal's path was taken for the seam-level part. Transcripts share the buffer, the flush and the sinks, keyed by run identifier. The run-started event records the transcript's location, and the run-complete event records its hash. Files are stored as written and scrubbed of secrets before the flush. The collection cadence, the scrub rules, the per-harness readers and the query dataset are a later design of their own.

## Decision

Every checkpoint writes events in the structure of [decision 0005](0005-the-factory-domain-model.md). Each check writes one verification event. Each checkpoint writes one checkpoint-complete event carrying the slot table. Events reach the local journal first. A buffer flushes on push and on a timer to the orphan branch, and to the object store when one is configured. Raw transcripts travel the same path.

A flush that cannot reach its sink keeps the buffer and retries at the next flush. A gap event records the interval at the next successful flush. In the pull-request checkpoint a failed flush fails the aggregation, because evidence must be durable before a state changes.

## Consequences

- Decision 0009 is amended to name the buffer, the flush and the two sinks. Its rule that a journal reaches its sink before the work item changes state stands.
- A local run's journal survives the machine as of its last flush. A machine that dies before a flush loses at most the buffered events, and the journal shows the gap.
- The meta-loop's readers join transcripts and events by run identifier.
- The object store client enters behind the same sink interface as the branch, so a reader sees one journal.
