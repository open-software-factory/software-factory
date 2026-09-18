# Architecture decision records

This directory records technical decisions that constrain implementation. It does not replace architecture design. A record captures a decision, and a detailed design is written once the work it covers is agreed. What is being built, and when, lives in the work tracker.

Each record uses a numbered, lowercase kebab-case filename and states its status, date, context, decision and consequences. A provisional record is an architectural hypothesis to validate before its interfaces are frozen.

A record may be amended in place when the decision itself holds and the wording no longer serves it. The date line says when it was amended and what changed. A decision that reverses another gets its own record, which supersedes it.

| Record | Status | Summary |
|---|---|---|
| [`0001-rust-for-the-factory-engine.md`](0001-rust-for-the-factory-engine.md) | Accepted | The Factory Engine, CLI and verifier runner are written in Rust. |
| [`0002-provider-neutral-process-boundaries.md`](0002-provider-neutral-process-boundaries.md) | Accepted | Providers remain replaceable and preferably out of process; the sandbox and the platform it runs on are two separate providers. |
| [`0003-deterministic-verification-is-authoritative.md`](0003-deterministic-verification-is-authoritative.md) | Accepted | Deterministic checks are authoritative; verifier, reporter, policy and gate are distinct words; a read separates "could not read" from "nothing there"; green is earned. |
| [`0004-protocol-independent-core-with-ahp-acp-edges.md`](0004-protocol-independent-core-with-ahp-acp-edges.md) | Provisional | AHP and ACP are first-class edge protocols at the edge of the factory domain model. |
| [`0005-the-factory-domain-model.md`](0005-the-factory-domain-model.md) | Accepted | One model: work items, runs, verifier and review runs, findings with evidence grades; events are a core part of the model; located findings are SARIF; runs are hash-chained and replayable. |
| [`0006-distribution-and-packaging.md`](0006-distribution-and-packaging.md) | Accepted | One repository and one version across every artifact; scoped package names and a short binary name; the core installs read-only and is authoritative only in continuous integration; the adopter chooses whether the factory is its own repository or vendored into one. |
| [`0007-console-stack.md`](0007-console-stack.md) | Accepted | React and TypeScript on Vite, scaffolded as Tauri from the start; one codebase for web and desktop; the console reads the engine and probes nothing itself. |
