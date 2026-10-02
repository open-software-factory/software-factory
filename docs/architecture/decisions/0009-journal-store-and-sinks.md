# 0009: Journal store and sinks

Status: provisional

Date: 2026-09-20

## Context

[Decision 0005](0005-the-factory-domain-model.md) makes the event journal the record: entity tables are projections of it, and a run's journal is hash-chained. It does not say where the journal is written or how a reader other than the writer reaches it. The smallest working engine needs both before its first run.

## Decision

A run's journal is one file of JSON Lines, one event per line, under a state directory: `runs/<run id>.jsonl`. The state directory is a path the engine is given, mounted into the sandbox, so the journal outlives the sandbox. An index file per work item lists its run identifiers in order.

The writer validates every event against the schema before appending it and refuses an invalid one loudly. Each event carries the hash of the event before it, with wall-clock time excluded, and the run-complete event carries the head hash.

Readers compute projections on read. There is no second store and no database until a reader's need is measured. The meta-loop's analytical queries are the expected first such need, and an analytical engine over the same files is the expected answer.

A sink copies a completed run's journal to where other readers can see it. The local sink is the state directory itself and is the first. A forge-native sink, which keeps the journal beside the pull request, follows behind the same interface. A run's journal reaches its sink before the run's work item changes state, so a reader never sees a state without the events that led to it.

## Consequences

- The engine runs with one setting for the state directory, and the default sandbox mounts a volume there.
- A journal can be copied, diffed and replayed with ordinary tools.
- Projections are cheap to change, because nothing is precomputed.
- The choice of an analytical store is deferred to measured need and gets its own record.
