# Open questions

The factory model in [factory-model.md](factory-model.md) and the develop job in [develop-job.md](develop-job.md) answer the earlier questions about the parts, the terms, the develop job, the review check and the journal. The decisions in [decisions/](decisions/) hold the answers. This page keeps only what is still open.

## The develop job

- The actor grammar: which actor types the journal records, and how a person and an agent appear in one job.
- The operation names: which verbs the journal uses for start, check, review, push and finish.
- The event names: which of the draft event names survive the first runs.
- The journal id format and its retention: how long a journal branch keeps a work item, and when it is compacted.

## The review check

- The fork behavior: whether a fork's pull request is reviewed by default, and how its credentials are isolated.
- The token egress: how a reviewer's provider key stays outside the reviewer container.

## The sandbox and the platform

- The sandbox interface: what stays stable across a container, a virtual machine and the sandbox types that follow.
- The execution platform interface: what stays stable across a local machine, a server, a cloud and a rented virtual machine.
- How Windows, macOS and Linux, and x64 and ARM64, capabilities enter scheduling.
- Where remote caching belongs: the factory, the build system, or the platform.

## The protocol

- The ACP capability profile an agent needs for autonomous factory work.
- How native ACP implementations, ACP adapters and provider-native integrations advertise their different fidelity.
- How AHP session identifiers relate to a work item and a job without becoming factory identifiers.
- Which person-facing concerns fit AHP and which need a factory-native query and command contract.
- How clients and the factory recover differently after a disconnect, an agent crash or a host crash.

See [../research/ahp-acp-architecture-direction.md](../research/ahp-acp-architecture-direction.md) for the research behind these questions.

## The console

- The console's data model, after the two-level Status and Step fields.
- Which projections the console needs first, and which the journal can serve directly.

## Adoption

- How an organisation adopts the factory from nothing, and whether that differs between the factory as its own repository and the factory vendored into one repository.
- Which work-tracker an adopter brings first.
- How the factory detects that a full set of green deterministic checks covered less than the requirement.

## The evidence ledger

- Where the evidence ledger lives: forge-native first, or an external store, and when the crossover is worth it.
- Which events and traces are first-class from day one.
- How false positives, escapes, agent performance and cost are measured.
