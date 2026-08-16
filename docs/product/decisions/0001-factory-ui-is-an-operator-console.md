# 0001: Factory UI is an operator console

Status: accepted

Date: 2026-08-16

## Context

The factory is meant to keep software work moving with progressively less routine human supervision. A read-only dashboard would expose state but leave the operator to switch tools whenever judgment or control is required. A chat-first interface would also reduce a spatial, concurrent system to a conversation stream.

## Decision

Treat the Factory UI as an active operator console. It must make the most important intervention visible and actionable, show live work and dependencies, preserve access to raw agent and tool activity, and support controlled changes to factory operation.

Its main conceptual surfaces are attention, the live factory floor, work graphs, the flight recorder, factory runway and factory intelligence. The ambient operator can appear through voice, commands, contextual actions, annotations or generated investigation views; chat is one possible manifestation rather than the product's organising metaphor.

The detailed experience is described in the [`factory UX vision`](../ux/factory-ux-vision.md).

## Consequences

- Read models and controls must be designed together, even when the first implementation exposes only a subset of commands.
- Important actions require clear authority, feedback and auditability.
- Visualisations must derive from real factory state rather than decorative activity.
- The engine remains operable independently of React and the desktop application.
