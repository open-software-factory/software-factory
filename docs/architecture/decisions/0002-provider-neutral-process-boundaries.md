# 0002: Provider-neutral process boundaries

Status: accepted

Date: 2026-08-16, amended 2026-09-18 to name the execution sandbox and the execution platform among the providers

## Context

The factory must work with multiple coding-agent harnesses, work trackers, source forges, execution sandboxes, execution platforms and delivery systems. Making any one provider intrinsic to the kernel would narrow adoption and confuse an integration with the factory domain.

## Decision

Keep provider concepts at the edges. A forge, a particular backlog, a coding agent, the sandbox an agent works in and the platform that sandbox runs on are integrations rather than kernel entities.

Two of those edges are separate and are often confused.

The **sandbox** is the isolation boundary the work runs inside. A container is one implementation. A virtual machine is another. Others exist, and more will.

The **execution platform** is where that sandbox runs. Local, on the machine a person or an agent already sits at. Remote, on a server, in a cloud, or on a hosted virtual machine someone rents.

The two are independent. Any sandbox runs on either platform. The factory ships a container on the local platform first, because that is the cheapest pair to hand an adopter. Nothing in the kernel may assume either choice.

Prefer language-neutral process and protocol boundaries initially, while allowing provider-native adapters when a common protocol is unavailable or loses required capability.

Define interfaces only when a real implementation requires replacement or translation. Do not create a universal provider framework in advance.

## Consequences

- The core owns software-factory semantics, and it holds no replica of a provider's own interface.
- Integrations can evolve or fail independently of the engine.
- Adapters must report capability and fidelity differences honestly.
- The first slice may use one concrete provider without treating its data model as universal.
- The container and the local platform the factory ships are defaults, so an adopter may replace either without the engine noticing.
