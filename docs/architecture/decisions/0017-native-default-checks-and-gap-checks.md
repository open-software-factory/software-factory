# 0017: Native default checks, several tasks per slot, and gap checks

Status: accepted. Amends [decision 0012](0012-slots-check-recognisers-and-slot-attestations.md).

Date: 2026-09-25

## Context

[Decision 0012](0012-slots-check-recognisers-and-slot-attestations.md) ships one TOML file of default checks per ecosystem. It left three things open. Which tools fill each slot. Whether a slot can hold more than one tool. And what a slot reports when no tool exists for it.

Decision 0012 answered the last one with a warning for an empty slot. A warning that no one tracks is a way out of a check, which is the failure [decision 0003](0003-deterministic-verification-is-authoritative.md) forbids when it says green must be earned.

The research is [native tools per verification slot](../../research/2026-09-25-native-tools-per-verification-slot.md) and [qlty as a check runner](../../research/2026-09-25-qlty-as-a-check-runner.md).

## Options considered

**Where the default tools come from.**

| Option | What it meant | Outcome |
|---|---|---|
| Ecosystem-native tools | Each slot runs the tool the ecosystem itself ships or standardises on. | Taken. The factory keeps up with each ecosystem's own advances, the same reasoning as running native builds under moon. |
| qlty as the default runner | qlty downloads pinned versions of existing linters as plugins and normalises their output to SARIF. | Set aside as a default. It is a second orchestrator beside moon, it has no .NET plugins, and its picks differ from the native ones in the audit and secret slots. It stays one candidate per slot. |

**How many tools fill a slot.** One task per slot with an osf wrapper around several tools, or several tasks per slot. Several tasks was taken. Each tool keeps its own journal event, its own cache entry and its own SARIF, so a failing tool is visible by name.

**A slot with no tool.** Report the slot empty at warning, fail the slot, or ship a gap check tracked by an issue. The gap check was taken.

## Decision

### Several tasks per slot

A slot is a tag. Every moon task carrying the tag `osf-slot-<slot>` fills that slot, and the slot passes only when every one of them passes. The defaults may ship more than one task for a slot, and an adopter adds another by tagging their own task.

### Candidates and defaults

The shipped defaults hold several candidate tasks for a slot and name one as the default pick. `osf.toml` switches the pick with one key. qlty is a candidate wherever it has a plugin, and never the default.

The first defaults are these.

| Slot | Rust | .NET |
|---|---|---|
| Lint | clippy | Roslyn analysers in `dotnet build`, warnings as errors |
| Format | rustfmt | `dotnet format` |
| Compile | `cargo check` | `dotnet build` |
| Tests | `cargo test` | `dotnet test` |
| Secrets | gitleaks | gitleaks |
| Dependency audit | cargo-audit | `dotnet list package --vulnerable` |
| Licence audit | cargo-deny | nuget-license |
| Security analysis | a gap check | Security Code Scan analysers |
| Architecture | cargo-deny bans, cargo-modules cycle check, cargo-public-api, and a gap check for layering rules | NetArchTest |
| Mutation testing | cargo-mutants | Stryker.NET |

Each ecosystem uses its own native licence tool first. Trivy is the fallback for an ecosystem with none, because it reads lock files directly across ecosystems. cargo-nextest replaces `cargo test` only after it is measured on a real repository.

### Gap checks

A slot with no tool gets a gap check. It is a shipped default task that runs `osf gap <slot>`. On its first run in a repository it creates or updates one issue in the osf repository that tracks the gap. It reports at warning with the link to that issue, and the slot table shows the slot as a gap tracked by that issue. A gap check is never an untracked empty slot.

This replaces the empty-slot warning in decision 0012. A slot only an adopter can fill, such as architecture tests in a codebase the factory does not know, gets the same gap check until the adopter tags a task for it.

### Mutation testing

Mutation testing runs inside pull requests on the changed code. If a run takes more than ten minutes, it moves to a scheduled check.

## Consequences

- The catalogue page shows each slot's default, its candidates, and its gap issue where one exists.
- A gap issue is where the real check for that slot gets built. Closing it replaces the gap check with a tool.
- qlty's licence is the Business Source License, which forbids use in a commercial AI coding service offered to third parties until its change date. A self-run factory is unaffected. Shipping qlty inside a hosted offering needs a legal read first.
