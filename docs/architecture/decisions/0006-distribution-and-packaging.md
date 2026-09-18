# 0006: Distribution and packaging

Status: accepted

Date: 2026-09-15

## Context

The factory reaches a workspace as a core in a container image and a surface vendored per repository; `docs/architecture/how-the-factory-reaches-a-repository.md` describes that composition. This record decides what is published, under which names, how it is versioned, and how the core is protected once installed.

"Software factory" is a generic industry term in continuous use since 1968 and stays the project's description. The bare word `factory` is taken on the npm, PyPI and crates.io registries, and a commercial coding-agent company publishes a command-line tool under it. The project name and the organisation name stay as they are; only the published artifacts need distinct names.

## Decision

### One repository, several artifacts

The engine, the contracts, the default skills, the per-ecosystem templates, the console and the container definition live in one repository, in one folder each. A folder becomes its own repository only when an adopter needs to fork that piece alone, or when two pieces' release cadences genuinely conflict. Not before.

Each release publishes:

| Artifact | Form |
|---|---|
| The engine binary | One static binary per platform, attached to the release |
| The skills bundle | An archive of the default agent skills and rules |
| The templates bundle | An archive of the per-ecosystem verifier configurations and workflow templates |
| The container image | Built from the same commit, carrying the binary |

### Names

Published packages use the organisation scope. The binary and its crate carry a short name that is not `factory`; the working name is `osf`. Documentation calls the product "the factory" in prose and uses the binary name only in commands.

### Versions

One version string per release covers every artifact. Tags follow the moving-tag convention: an exact tag receives nothing further, a minor tag receives patches, a major tag receives minor and patch releases, and `latest` always points at the newest. An adopter pins as tightly as it chooses.

### Where the core runs, and how it is protected

The binary installs into the container image read-only and owned by root, under a checksum wrapper that verifies the installed files and the git hooks path before every hook runs. The same binary runs in continuous integration under the workflow's own token, which no agent holds, and branch protection requires that run. Local execution raises the bar against an agent that reads and works around a check; the continuous-integration run is the authority.

### What an adopting repository receives

A repository needs no file at all: the binary detects the ecosystem from its marker files, runs that ecosystem's standard tools and reads their native output. A repository overrides commands in one configuration file when the defaults are wrong. A repository that emits the factory's events from its own tooling needs no wrapping at all.

## Consequences

- A release is one commit and one version across every artifact; nothing is versioned separately.
- The question of whether each folder becomes its own repository is closed until a concrete adopter or cadence conflict reopens it.
- The container image is the only place the core is installed writable; every other copy is read-only.
