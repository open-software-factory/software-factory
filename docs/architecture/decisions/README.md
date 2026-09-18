# Architecture decision records

This directory records technical decisions that constrain implementation. It does not replace architecture design: records capture decisions, while detailed designs should be written only for agreed implementation slices.

Each record uses a numbered, lowercase kebab-case filename and states its status, date, context, decision and consequences. A provisional record is an architectural hypothesis to validate before its interfaces are frozen.

A record is normally immutable once accepted, and a later record supersedes it. Records 0001, 0003 and 0005 were revised in place on 2026-09-15, before any code existed that depended on them; each says so in its own date line. That exception ends with the first implementation slice.

| Record | Status | Summary |
|---|---|---|
| [`0001-rust-for-the-factory-engine.md`](0001-rust-for-the-factory-engine.md) | Accepted | The Factory Engine, CLI and verifier runner are one static Rust binary. |
| [`0002-provider-neutral-process-boundaries.md`](0002-provider-neutral-process-boundaries.md) | Accepted | Providers remain replaceable and preferably out of process. |
| [`0003-deterministic-verification-is-authoritative.md`](0003-deterministic-verification-is-authoritative.md) | Accepted | Deterministic checks are authoritative; verifier, reporter, policy and gate are distinct words; a read separates "could not read" from "nothing there"; green is earned. |
| [`0004-protocol-independent-core-with-ahp-acp-edges.md`](0004-protocol-independent-core-with-ahp-acp-edges.md) | Provisional | AHP and ACP are first-class edge protocols at the edge of the factory domain model. |
| [`0005-the-factory-domain-model.md`](0005-the-factory-domain-model.md) | Accepted | One model: work items, runs, verifier and review runs, findings with evidence grades; events are the model; located findings are SARIF; runs are hash-chained and replayable. |
| [`0006-distribution-and-packaging.md`](0006-distribution-and-packaging.md) | Accepted | One repository, four release artifacts under one version; scoped package names and a short binary name; the core installs read-only and is authoritative only in continuous integration; zero-config adoption. |
| [`0007-console-stack.md`](0007-console-stack.md) | Accepted | React and TypeScript on Vite, scaffolded as Tauri from the start; one codebase for web and desktop; the prototype packages move unchanged; the console reads the engine and probes nothing itself. |
