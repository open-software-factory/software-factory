# 0001: Rust for the Factory Engine

Status: accepted

Date: 2026-08-16, revised 2026-09-15.

## Context

The factory needs a small control-plane core that starts quickly, runs natively and can be distributed across Windows, Linux and macOS on x64 and ARM64. Its implementation language must not constrain coding agents, skills or providers in the wider ecosystem.

Two things firmed up after the original choice. Product decision 0001 made the console an operator control surface, and its likely first form is a desktop application with a Rust shell and a web front end (decision 0007). And the engine's core job became clear: run verifiers and record evidence that agents cannot forge (decision 0003). A verifier binary that agents can read and reason about is a verifier they can work around; a compiled, read-only binary raises that bar.

## Decision

Build the Factory Engine, its command-line interface and the verifier runner in Rust. Prefer explicit types, ordinary crates and standard process or protocol boundaries. Do not use a Rust-specific plugin mechanism as the primary integration model; integrations stay out of process.

## Consequences

- The engine and CLI ship as native, cross-compiled binaries with no runtime dependency.
- Integrations, the providers at the factory's edges such as the forge, the work tracker, the runner and the coding-agent harness, may use any language and normally communicate out of process, per decision 0002.
- The console may share domain types, event codecs and evidence schemas with the engine as crates, without a serialisation boundary between two languages.
- The first slice is kept deliberately small so it stays cheap to delete.
- A compiled binary is not a security boundary on its own; authority comes from running it in continuous integration under a token the agent never holds, behind branch protection. Decision 0006 covers where the binary is installed and how it is protected.
