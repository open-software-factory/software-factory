# 0007: Console stack

Status: accepted

Date: 2026-09-15

## Context

Product decision 0001 makes the console an operator control surface and requires that the engine stay operable without it.

## Decision

The console is a React and TypeScript application built with Vite, scaffolded as a Tauri application from the start. One frontend codebase produces two targets: the browser, served by the Vite build, and the desktop, wrapped by Tauri. Tauri's mobile targets are available later from the same source.

The two prototype packages move into the repository unchanged: the headless state model and the React component package. Only the bundler changes, to Vite. The prototype's storage seam becomes the application's binding file: memory for tests, web storage in the browser, the Tauri store on the desktop.

The console reads from the engine. It does not probe the machine itself. Discovery of installed coding agents, toolchains and repositories is the engine's job, exposed as a query, so the browser and the desktop targets stay identical and no desktop-only code path forms.

Every view is a projection of the event stream and the entities decision 0005 defines; no view invents its own state.

## Consequences

- Web and desktop share every screen. A capability one target lacks is absent from both until the engine provides it.
- The engine grows a query surface for the console before the console needs it; that surface is factory-native, and decision 0004 keeps agent-session protocols at their own edge.
- A Tauri desktop build can share crates with the engine, which the revised decision 0001 anticipates, without the web build depending on that.
