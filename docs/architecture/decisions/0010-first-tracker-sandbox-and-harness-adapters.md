# 0010: The first tracker, sandbox and harness adapters

Status: provisional

Date: 2026-09-20

## Context

[Decision 0002](0002-provider-neutral-process-boundaries.md) keeps the tracker, the sandbox, the platform and the coding agent at the edges and says an interface is defined only when a real implementation requires it. The smallest working engine requires one of each. This record says how the first of each is built, so that the second can replace it.

## Decision

**Tracker.** The first adapter reads and writes GitHub Issues and Projects through the API. A work item is ready when its project status says so. The adapter reads the item's title, body, repository and dependencies, writes its state back to the same status field, and writes the recap as a comment. A read that fails returns unknown and an empty read returns empty, per [decision 0003](0003-deterministic-verification-is-authoritative.md).

**Sandbox.** The first provider is the default container on the local platform, started through the container runtime's command line, with the repository and the state directory mounted. The engine's run command executes inside the sandbox. A thin host-side command starts the sandbox and invokes it, and that is the only host-side step.

**Harness.** The first adapters drive the installed coding agents in their unattended modes as subprocesses: a prompt in, a result and a session identifier out, cost when the harness reports it. Every adapter sits behind one interface and reports its capabilities: whether it returns a structured result, whether it reports cost, whether it can be stopped mid-run, and whether a stop hook checks its messages. The engine picks the harness for a run from configuration and records the actor on the run. An adapter over the agent client protocol is added when a harness supports it with adequate fidelity, per [decision 0004](0004-protocol-independent-core-with-ahp-acp-edges.md), behind the same interface.

**Forge.** The first adapter is GitHub, from the status and review code that exists, moved behind an interface that names branch, pull request, status block, review and check status.

## Consequences

- Each adapter's fidelity is stated in its capability report and nothing is assumed of a second implementation.
- The engine's own code names no provider; a test doubles each interface.
- The interfaces are frozen only after a second implementation of each exists, which is the condition [decision 0002](0002-provider-neutral-process-boundaries.md) sets.
