# 0002: Provider-neutral process boundaries

Status: accepted

Date: 2026-08-16, amended 2026-09-18 to name the execution sandbox among the providers

## Context

The factory must work with multiple coding-agent harnesses, work trackers, source forges, execution sandboxes, runners and delivery systems. Making any one provider intrinsic to the kernel would narrow adoption and confuse an integration with the factory domain.

## Decision

Keep provider concepts at the edges. A forge, a particular backlog, a coding agent, a runner and the sandbox an agent works in are integrations rather than kernel entities.

The execution sandbox is a provider like any other. A container is one implementation of it, and the factory ships a container first because it is the cheapest to give an adopter. A worktree on the host, a virtual machine and a remote runner are other implementations of the same edge. Nothing in the kernel may assume which one is in use.

Prefer language-neutral process and protocol boundaries initially, while allowing provider-native adapters when a common protocol is unavailable or loses required capability.

Define interfaces only when a real implementation requires replacement or translation. Do not create a universal provider framework in advance.

## Consequences

- The core owns software-factory semantics, not replicas of provider APIs.
- Integrations can evolve or fail independently of the engine.
- Adapters must report capability and fidelity differences honestly.
- The first slice may use one concrete provider without treating its data model as universal.
- The container the factory ships is a default, so an adopter may replace it without the engine noticing.
