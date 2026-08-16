# 0001: Go for the Factory Engine

Status: accepted

Date: 2026-08-16

## Context

The factory needs a small control-plane core that starts quickly, runs natively and can be distributed across Windows, Linux and macOS on x64 and ARM64. Its implementation language must not constrain coding agents, skills or providers in the wider ecosystem.

## Decision

Start the Factory Engine and its CLI in Go. Prefer explicit types, ordinary packages and standard process or protocol boundaries. Do not use Go-specific plugin mechanisms as the primary integration model.

## Consequences

- The initial engine and CLI can ship as native, cross-compiled binaries.
- Integrations may use any language and normally communicate out of process.
- The language choice does not predetermine the UI stack or every future component.
- Go abstractions should be introduced only when an implementation slice demonstrates a real seam.
