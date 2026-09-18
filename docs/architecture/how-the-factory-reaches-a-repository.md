# How the factory reaches a repository

Status: provisional design. Date: 2026-09-18.

What the factory installs, where each part lives, and how the parts combine when a repository is opened. One model serves a container that mounts many repositories today, and a factory deployed into each repository later.

## Two layers

**Core.** The commands and their guardrails, baked into a development-container image, root-owned and read-only. The core never ships into a product repository. It is Rust.

The core is a front command named `osf` and a set of part binaries in known locations, the shape git and the dotnet command line use. `osf <command>` finds the part that serves that command and runs it. A new capability adds a binary instead of growing one file, and an uncalled part costs nothing at run time, but every part ships and versions together, at one version per release ([decision 0006](decisions/0006-distribution-and-packaging.md)). The front command owns the command list, the configuration, and the exit codes, so a caller sees one tool.

**Surface.** Runbooks, skills, agent rules, and per-repository metadata such as the stack profile, the commands to build and test, and the trust level. The surface is vendored into each product repository and drift-gated, so it is reviewed in ordinary pull requests.

## Composition

The installer composes the core with the surface of each repository it can see. Both deployment shapes are supported at once, and neither is a later stage:

1. one container mounting many repositories under a workspace directory;
2. the factory deployed into a single repository, or one container per repository.

Flipping between them changes the deploy target and nothing else. The loop over repositories lives only in the installer. Every check acts on the one repository it fires in.

## Precedence

A capability may arrive from more than one place, so the more specific source wins.

| Source | Beats |
| --- | --- |
| the repository's own surface | everything below |
| an organisation-wide surface, where one is configured | the core |
| the core in the image | nothing |

A repository that ships its own skill, runbook or rule replaces the one the factory supplies under the same name. Anything the repository does not name, it inherits. A composed run records which source each capability came from, so a surprising result can be traced without reading three trees.

## Adopter scripts

A check does not have to be a factory command. An adopter registers any script in the repository's surface, and the factory runs it at the point the surface names. The script is free in how it works and owes the factory four things:

- an exit status, where zero is a pass;
- findings on standard output in the agreed shape, or none;
- a log, captured and kept with the run;
- the counters the run needs, such as duration and what it read.

The exact shape settles as the first adopters write real scripts.

## Lifecycle

Author, version, publish the image as the core, deploy the surface through a pull request per repository, verify through the drift gate in each repository's continuous integration, and compose when a container starts.

## What the model implies about the command surface

Installing, syncing, verifying a surface against the pinned version, deploying to a repository, minting a short-lived forge token, and a health probe. Plus a registry of stack adapters that supplies the build, test, lint and format commands per repository, starting with .NET, Java, Dart, TypeScript, Python, Go and Rust.

## Constraints this model must keep

- Provider neutrality (decision 0002): the forge, the container runtime, and the agent harness are adapters, and the composition rule names none of them.
- Deterministic verification is authoritative (decision 0003): the drift gate and the hooks are deterministic checks, so no model judges them.
- Per-repository containers must remain possible, so nothing in the installer may assume every repository shares one filesystem.
- A part binary is replaceable on its own, so no part may require another part to be present.
