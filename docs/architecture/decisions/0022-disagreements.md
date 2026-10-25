# 0022: Disagreements

Status: accepted

Date: 2026-10-21

## Context

A builder agent can refuse a review finding. Without a rule, a disagreement is a silent waiver, and with too strict a rule the builder cannot correct a wrong finding.

## Decision

The builder fixes every finding by default. A disagreement is allowed only with concrete deterministic evidence, such as a check result or a reproduced run. Reading the code, guessing or pointing at external documents is not evidence. The journal records the finding, the evidence and the why. A disagreement needs a person: in unattended mode the builder records it, pushes the pull request with the disagreement in the journal and the pull request, sets a status that asks for a person, and ends the job. In interactive mode the builder asks the person.

## Consequences

A disagreement is visible evidence, not a hidden suppression. The journal keeps the finding and the evidence for the person who decides.
