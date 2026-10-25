# 0023: Auto-merge

Status: accepted

Date: 2026-10-21

## Context

A low-risk pull request can wait for a person's approval or merge by itself. The factory already writes a deterministic risk section from the change's blast radius and configurable rules. This record decides how that risk reaches the forge's auto-merge.

## Decision

The pull request description carries the risk section produced by an osf command. A separate platform-workflow reads that risk and decides whether to turn on auto-merge. The adopter configures which risk levels qualify; low risk is the case discussed. The forge merges an auto-merge pull request when its own required checks and rules are met. Any other pull request needs a person. The identity that turns on auto-merge is deferred to the auto-merge experiments on low-risk pull requests in the osf repository.

## Consequences

The factory decides only whether to turn on auto-merge, not whether the forge's rules are met. A high-risk change still reaches a pull request; the risk section keeps it from auto-merging.
