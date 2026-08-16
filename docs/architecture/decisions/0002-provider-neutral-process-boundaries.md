# 0002: Provider-neutral process boundaries

Status: accepted

Date: 2026-08-16

## Context

The factory must work with multiple coding-agent harnesses, work trackers, source forges, runners and delivery systems. Making any one provider intrinsic to the kernel would narrow adoption and confuse an integration with the factory domain.

## Decision

Keep provider concepts at the edges. GitHub, a particular backlog, a coding agent and a runner are integrations rather than kernel entities. Prefer language-neutral process and protocol boundaries initially, while allowing provider-native adapters when a common protocol is unavailable or loses required capability.

Define interfaces only when a real implementation requires replacement or translation. Do not create a universal provider framework in advance.

## Consequences

- The core owns software-factory semantics, not replicas of provider APIs.
- Integrations can evolve or fail independently of the engine.
- Adapters must report capability and fidelity differences honestly.
- The first slice may use one concrete provider without treating its data model as universal.
