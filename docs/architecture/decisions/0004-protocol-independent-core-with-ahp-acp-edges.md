# 0004: Protocol-independent core with AHP and ACP edges

Status: provisional

Date: 2026-08-16

## Context

ACP has meaningful adoption as a boundary between clients and individual coding-agent harnesses. Microsoft's emerging AHP addresses host-authoritative agent-session state shared across operator clients. Neither protocol defines the factory's workflow, dependency graph, policy, verification, evidence, traceability or product-lifecycle semantics, and AHP remains young.

## Decision

Design AHP and ACP as first-class protocol implementations at distinct edges without making Factory Engine domain types depend on either schema.

- Prefer ACP toward individual coding-agent harnesses where its implementation has adequate fidelity.
- Treat factory-owned UIs as AHP clients for agent-session state and as factory-native clients for work, policy, verification and lifecycle state.
- Place the AHP server role in a logically separate Factory AHP Gateway toward factory-owned surfaces.
- Give the Factory Engine no intrinsic AHP role.
- Preserve provider-native harness adapters where ACP is missing or insufficient.
- Consider a separate future connector that acts as an AHP client toward external hosts such as VS Code's Agent Host.

The evidence, role diagrams and alternatives are in [`AHP and ACP architecture direction`](../../research/ahp-acp-architecture-direction.md).

## Consequences

- Protocol schemas are translated at the edges rather than imported into the core domain.
- Agent-session state and factory lifecycle state remain related but distinct.
- Packaging the gateway with the engine is possible, but package and schema boundaries must preserve the logical separation.
- Capability experiments and conformance tests are required before provider interfaces or deployment topology are frozen.
- AHP or ACP can be supplemented or removed without redefining the factory domain.
