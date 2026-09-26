# 0003: Deterministic verification is authoritative

Status: accepted

Date: 2026-08-16, amended 2026-09-15 with the vocabulary and the two rules below, before any verifier code existed. Amended 2026-09-23 with the word checkpoint, from [decision 0011](0011-the-check-is-the-unit.md). Amended 2026-09-26 to point at [decision 0018](0018-hook-enforcement-and-the-pinned-osf.md) for how the hook, pre-commit and pre-push checkpoints are enforced locally.

## Context

Coding agents need fast feedback and the factory needs trustworthy evidence. Many quality properties already have deterministic checks in their language or delivery ecosystem. Asking an LLM to replace those checks would weaken reproducibility and make gates harder to audit.

## Decision

When a property can be checked deterministically, the deterministic result is authoritative. The factory should invoke or consume existing repository, language, CI and delivery tooling rather than reimplement it.

LLM review may interpret evidence, find issues outside deterministic coverage or recommend follow-up work. It does not turn a failing deterministic gate into a passing one.

### Vocabulary

Five words, each with one job:

| Term | Meaning |
|---|---|
| Verifier | Runs one check and emits evidence. It knows nothing about consequences. Any command can be a verifier; the factory ships defaults per language ecosystem. |
| Reporter | Renders evidence onto a surface: a check, a pull-request block, a review, a store. |
| Policy | Reads evidence and decides. A soft cap warns; a hard limit blocks. |
| Gate | The name for the moment a policy hard limit stops the work. |
| Checkpoint | A point where checks run. There are five: the harness hook, pre-commit, pre-push, the pull request and the schedule. A check is what a verifier runs at a checkpoint. |

### Two rules every verifier and every reader of evidence follows

**A read distinguishes "could not read" from "read, and found nothing".** A verifier that could not run reports a failure to run, and no such report ever counts as a pass. A reader of a tracker, a forge or a store returns "unknown" when it could not read and an empty result when it read and there was nothing. The two are different facts and are recorded differently.

**Green must be earned.** A change with no declared test command is held. A test suite whose collected test count shrinks between the base and the change does not pass on count alone; the shrink is a finding. A suppression added in a change (a lint or type-check silencer) is itself a finding for review. An empty result from an empty input is not a pass.

## Consequences

- Gate results must retain command, environment, version, timing and artifact evidence where practical.
- Repository-native checks remain the source of truth for their scope.
- LLM judgments and deterministic results need visibly different evidence types.
- Caching or deduplication must preserve the conditions under which a result remains valid.
