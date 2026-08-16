# 0003: Deterministic verification is authoritative

Status: accepted

Date: 2026-08-16

## Context

Coding agents need fast feedback and the factory needs trustworthy evidence. Many quality properties already have deterministic checks in their language or delivery ecosystem. Asking an LLM to replace those checks would weaken reproducibility and make gates harder to audit.

## Decision

When a property can be checked deterministically, the deterministic result is authoritative. The factory should invoke or consume existing repository, language, CI and delivery tooling rather than reimplement it.

LLM review may interpret evidence, find issues outside deterministic coverage or recommend follow-up work. It does not turn a failing deterministic gate into a passing one.

## Consequences

- Gate results must retain command, environment, version, timing and artifact evidence where practical.
- Repository-native checks remain the source of truth for their scope.
- LLM judgments and deterministic results need visibly different evidence types.
- Caching or deduplication must preserve the conditions under which a result remains valid.
