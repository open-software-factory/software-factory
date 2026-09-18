# Architecture decision records

This directory records technical decisions that constrain implementation. It does not replace architecture design: records capture decisions, while detailed designs should be written only for agreed implementation slices.

Each record uses a numbered, lowercase kebab-case filename and states its status, date, context, decision and consequences. A provisional record is an architectural hypothesis to validate before its interfaces are frozen.

| Record | Status | Summary |
|---|---|---|
| [`0001-go-for-the-factory-engine.md`](0001-go-for-the-factory-engine.md) | Accepted | The Factory Engine starts in Go. |
| [`0002-provider-neutral-process-boundaries.md`](0002-provider-neutral-process-boundaries.md) | Accepted | Providers remain replaceable and preferably out of process. |
| [`0003-deterministic-verification-is-authoritative.md`](0003-deterministic-verification-is-authoritative.md) | Accepted | Deterministic checks are authoritative where they exist. |
| [`0004-protocol-independent-core-with-ahp-acp-edges.md`](0004-protocol-independent-core-with-ahp-acp-edges.md) | Provisional | AHP and ACP are first-class edge protocols at the edge of the factory domain model. |
