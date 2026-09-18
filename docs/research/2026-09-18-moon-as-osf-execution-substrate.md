---
title: Moon as the OSF Execution Substrate
document_type: research
status: concluded
date: 2026-09-18
---

# Moon as the OSF Execution Substrate

**Status:** Research conclusion  
**Date:** 2026-09-18  
**Scope:** Deterministic build, test and verification orchestration for Open Software Factory

## Executive conclusion

OSF should use moon as an external, native orchestration engine for deterministic engineering tasks. OSF should not build a new build system, embed moon's internal Rust crates, fork moon, or introduce an `osf exec` command at this stage.

The boundary should be:

- language-native tools retain build semantics;
- moon owns the canonical task and action graph, affected calculation, scheduling, hashing and caching;
- OSF owns lifecycle policy, agent coordination, verification checkpoints, evidence and escalation;
- coding agents remain free to use familiar commands such as `dotnet build`, `dotnet test`, `cargo test`, `go test` and `pnpm test` during implementation;
- OSF invokes moon at authoritative checkpoints, where results become factory evidence.

This is not fully transparent execution of every agent command. A command executed directly by an agent does not pass through moon and therefore does not receive moon's caching or graph semantics. That is acceptable. Native build tools already provide useful incremental behaviour during the agent's inner loop. Moon is most valuable at the authoritative verification boundary, where consistency and complete evidence matter more than accelerating every exploratory command.

## Context

The Agent Native Operating Model and Automated SPDLC vision place deterministic tools beneath agent reasoning. Agents implement work; hard gates decide whether the result is safe to advance. The execution layer therefore needs to be fast, cross-platform, language-agnostic, observable and capable of growing toward shared caching and stronger isolation without taking ownership of each language's build semantics.

The original search started with Bazel and Buck2 because they provide sophisticated dependency graphs, hermetic actions, content-addressed caching and remote execution. The investigation changed direction after separating two concerns:

1. **Build semantics:** how .NET, Rust, Go, JavaScript and other ecosystems compile, restore, test and package software.
2. **Execution semantics:** which tasks should run, in what order, for which changes, under which policy, and whether their results can be reused.

OSF needs the second. Existing ecosystem tools already solve the first.

## Requirements

The useful subset of Bazel-like behaviour is:

- a project and task dependency graph;
- affected-only execution;
- content-derived task identity;
- local and shared caching;
- parallel topological scheduling;
- declared inputs and outputs;
- cross-platform native execution;
- arbitrary language and tool support;
- machine-readable graphs and run results;
- an incremental path toward controlled, isolated, hermetic and remote execution.

OSF does not need:

- a new compiler abstraction;
- replacement dependency resolution for NuGet, Cargo, npm, pnpm or other ecosystems;
- a universal build-rule language;
- migration of ordinary development into an OSF-specific command vocabulary;
- ownership of a second remote cache, CAS, scheduler or worker fleet.

## What moon actually is

Moon is a native Rust application rather than a TypeScript task runner. Its repository describes it as a repository management and orchestration tool written in Rust, with incremental adoption, smart hashing, remote caching, an integrated toolchain, project and dependency graphs, and a parallel action pipeline. It is distributed under the MIT licence. [Moon repository](https://github.com/moonrepo/moon)

Moon tasks are deliberately generic. A task is a binary or system command executed as a child process in the project directory. Tasks may invoke any command available on `PATH`, even where moon does not provide first-class toolchain support for that language. [Creating moon tasks](https://moonrepo.dev/docs/create-task) [Moon feature comparison](https://moonrepo.dev/docs/comparison)

For example:

```yaml
tasks:
  build:
    command: dotnet build --no-restore
    inputs:
      - "src/**/*"
      - "Directory.Build.*"
      - "global.json"
      - "packages.lock.json"
    outputs:
      - "src/**/bin/**/*"
      - "src/**/obj/**/*"

  test:
    command: dotnet test --no-restore
    deps:
      - build
```

Moon does not replace `dotnet`; it launches `dotnet`. The equivalent applies to Cargo, Go, pnpm, pytest, Semgrep, Terraform and future tools.

When invoked through `moon run`, moon executes targets and dependencies in topological order and incrementally caches each task. `moon check` runs the build and test tasks for selected projects. `moon ci` filters tasks to those affected by changed files and writes a structured execution report. [moon run](https://moonrepo.dev/docs/commands/run) [moon check](https://moonrepo.dev/docs/commands/check) [moon ci](https://moonrepo.dev/docs/commands/ci)

Moon also exposes its action graph as JSON or DOT. The graph includes more than task-to-task relationships: actions can include toolchain setup, dependency installation, project synchronisation and task execution. [Moon action graph](https://moonrepo.dev/docs/commands/action-graph)

## The direct command boundary

There is one important limitation:

```text
dotnet test
```

and:

```text
moon run api:test
```

ultimately execute the same ecosystem tool, but only the second invocation participates in moon's task graph, hashing and cache.

Moon does not monitor the operating system for arbitrary compiler processes. It only manages tasks invoked through its action pipeline. Therefore, making every direct agent command transparently acquire moon semantics would require an interception mechanism such as:

- shell aliases or functions;
- `PATH`-precedence shims;
- coding-agent tool hooks;
- an OSF-managed shell or process host;
- explicit agent instructions to use moon targets.

None is currently justified merely to improve caching.

## Recommended operating model

Separate execution into two loops.

### Agent inner loop

The agent uses the commands it already understands:

```text
dotnet build
dotnet test
cargo test
go test ./...
pnpm test
```

These commands are immediate, familiar, debuggable and supported by the underlying ecosystems. Their results are useful working feedback but are not authoritative factory evidence.

### Factory verification loop

At lifecycle checkpoints, OSF invokes moon targets:

- before an implementation task is marked complete;
- before a pull request is considered ready;
- after integration or conflict resolution;
- in continuous integration;
- before deployment or other risk-sensitive transitions.

Moon then applies the canonical task graph, affected calculation, dependencies, hashing, cache and concurrency. OSF records the resulting evidence against the work item and decides whether the lifecycle may advance.

This preserves native agent behaviour while ensuring that every consequential transition passes through the same deterministic verification graph.

## Why `osf exec` is not currently needed

An `osf exec -- dotnet test` command would only be valuable if OSF needed to mediate every subprocess. Current requirements do not establish that need.

OSF already knows when a verification checkpoint occurs. It can invoke `moon check`, `moon run`, `moon exec` or `moon ci` as a child process and consume moon's exit status and structured reports. Adding a second generic execution CLI would create another command vocabulary, another configuration surface and an ambiguous division of responsibility.

The decision should be revisited only if OSF must enforce policy on all agent-launched processes, for example:

- filesystem or network isolation;
- environment-variable filtering;
- mandatory audit of every command;
- resource quotas;
- secret brokering;
- remote execution of arbitrary agent commands.

Even then, the required seam may belong in the agent runtime or sandbox rather than in a public `osf exec` command.

## Integration boundary

Moon should remain an independently versioned process dependency.

OSF should:

- locate or provision a pinned moon binary;
- invoke stable moon CLI commands;
- consume exit codes, run reports and graph exports;
- correlate moon actions with OSF work items, specifications and gates;
- surface moon failures as verification evidence rather than reimplementing their semantics.

OSF should not:

- link moon's private Rust crates;
- duplicate moon's project or task graph;
- maintain a patched moon fork;
- rewrite moon's scheduler, cache or hasher;
- generate a second task identity independent of moon.

Moon's public feature comparison currently states that its underlying runner cannot be customised. That reinforces a process boundary rather than an embedded-library design. [Moon feature comparison](https://moonrepo.dev/docs/comparison)

The moon configuration should initially remain the source of truth for canonical engineering tasks, inputs, outputs and dependencies. OSF policy may name moon targets and specify when they are required, but should not duplicate their definitions.

## Why Buck2 is a reference rather than a dependency

Buck2 is a fast, hermetic, multi-language build system written in Rust and dual-licensed under Apache-2.0 and MIT. It provides valuable reference designs for incremental computation, explicit actions, hermeticity, remote execution and graph introspection. [Buck2 repository](https://github.com/facebook/buck2)

It is nevertheless the wrong integration boundary for OSF:

- adopting Buck2 would move OSF toward owning build semantics;
- its rule and provider model is intentionally richer than OSF requires;
- embedding Buck2 internals would couple OSF to a large internal crate graph rather than a stable execution SDK;
- .NET and JavaScript development would require more build-system-specific ownership than simply invoking their mature native tools.

OSF should mine Buck2 for concepts while depending on no part of its engine.

## REAPI and remote execution

The Bazel Remote Execution API is more strategically relevant than Bazel or Buck2 themselves. REAPI represents a command, declared environment, input tree, output paths, platform and timeout as a content-addressed action. A client uploads the command and input tree to CAS and may ask a compatible service to execute the action. [REAPI protocol](https://github.com/bazelbuild/remote-apis/blob/main/build/bazel/remote/execution/v2/remote_execution.proto)

Moon already uses the cache portion of REAPI. Its remote-cache documentation requires Action Cache, CAS, SHA-256 and gRPC support. It does not document support for the REAPI `Execute` service. [Moon remote caching](https://moonrepo.dev/docs/guides/remote-cache)

The correct long-term boundary is therefore:

```text
OSF -> moon -> REAPI-compatible service
```

not:

```text
OSF -> NativeLink-specific API
```

NativeLink is a credible optional backend because a single configurable Rust binary can provide CAS, action cache, scheduler and worker roles. [NativeLink architecture](https://docs.nativelink.com/explanations/architecture-deep-dive)

Two qualifications matter:

1. NativeLink explicitly states that hermeticity is the client's responsibility. Undeclared inputs and network access can make a cache entry incorrect even when content addressing works perfectly. [NativeLink correctness and hermeticity](https://docs.nativelink.com/explanations/correctness-hermeticity)
2. Current NativeLink releases use FSL-1.1 with an Apache-2.0 future licence after two years. OSF should remain protocol-compatible and backend-neutral rather than taking a NativeLink code dependency. [NativeLink repository and licence](https://github.com/TraceMachina/nativelink)

Remote execution is not required for the initial OSF architecture. REAPI should be treated as the preferred future interoperability standard, and nothing here commits to implementing it now.

## Progressive hermeticity

Moon's declared inputs and hashes improve repeatability but do not make a task hermetic. A native `dotnet test` or `cargo test` may still read ambient SDKs, package caches, environment variables, certificate stores, temporary files or network resources.

OSF should treat execution strength as a progression rather than a binary switch:

| Level | Semantics | Current owner |
| --- | --- | --- |
| Native | Run the ecosystem command normally | Language tool |
| Tracked | Declared inputs and outputs participate in task identity | moon |
| Controlled | Pin toolchains and constrain relevant environment | moon plus repository policy |
| Isolated | Execute in a restricted filesystem and process environment | Future execution environment |
| Hermetic | Only declared inputs are visible and network is unavailable | Future execution environment |
| Remoteable | Fully represent the action and inputs through REAPI | Future moon or adapter capability |
| Remote | Execute through a compatible remote service | REAPI backend |

This policy should eventually be selectable per task. Development servers and formatting may remain native; release builds or high-risk verification may warrant stronger guarantees.

## Options considered

| Option | Advantages | Costs and risks | Decision |
| --- | --- | --- | --- |
| Build an OSF task and cache engine | Total control | Rebuilds mature graph, cache and scheduling infrastructure; creates a new build system by drift | Reject |
| Adopt Buck2 as the build system | Strong hermeticity and remote execution | High migration and rule ownership; weak fit with native-tool philosophy | Reject |
| Embed moon Rust crates | In-process control | Private internal APIs and tight release coupling | Reject |
| Fork moon | Maximum customisation | Permanent maintenance burden and ecosystem divergence | Reject |
| Force all agents to use moon commands | Every invocation gains moon semantics | Degrades native-tool ergonomics and agent priors | Reject as default |
| Transparently intercept native commands | Preserves visible command UX while applying policy | Complex, fragile and unnecessary for current requirements | Defer |
| Use moon for canonical checkpoints | Minimal integration, native agent UX, strong verification semantics | Exploratory commands do not benefit from moon cache | Adopt |

## Licensing

OSF is dual-licensed under Apache-2.0 and MIT. Moon is MIT-licensed, and Buck2 is Apache-2.0/MIT dual-licensed. Invoking moon as an external process presents no licence conflict with OSF's chosen licences. If OSF later distributes moon binaries, the distribution must retain moon's applicable copyright and licence notices.

NativeLink's current FSL-1.1-Apache-2.0 licence is another reason to keep remote infrastructure behind the open REAPI boundary. This is an architectural observation, and no part of it is legal advice.

## Risks and mitigations

| Risk | Consequence | Mitigation |
| --- | --- | --- |
| Moon task declarations are incomplete | False cache hits or missed verification | Validate inputs conservatively; periodically force clean runs and compare results |
| Direct agent runs differ from canonical tasks | Agent sees a pass that the gate later rejects | Treat native runs as feedback only; make moon results authoritative |
| Moon changes unstable features | Integration breakage | Pin moon versions and depend on stable CLI/report surfaces first |
| OSF duplicates moon configuration | Conflicting sources of truth | Keep task topology and task identity in moon |
| Remote cache is mistaken for hermeticity | Incorrect cached results | Introduce controlled and isolated execution before trusting remote execution |
| NativeLink becomes strategically unsuitable | Backend lock-in | Depend on REAPI and on no NativeLink-specific behaviour |

## Open questions

These do not block the architectural direction:

- How should OSF provision and pin moon across Windows, macOS and Linux?
- Which moon command best represents each OSF gate: `run`, `check`, `exec --plan` or `ci`?
- Which parts of moon's run report and action graph should OSF persist as durable evidence?
- How should repositories define the mapping from OSF gate names to moon targets without duplicating task definitions?
- At what scale does remote caching become economically useful?
- Which sandboxing mechanism can enforce stronger task policies consistently across supported operating systems?
- Will moon add a stable remote-execution or pluggable-runner boundary before OSF needs one?

## Recommended next validation

Before making this a hard dependency, run one thin vertical experiment in the OSF repository:

1. define representative build, test, lint and security tasks in moon;
2. invoke those tasks from an OSF verification checkpoint;
3. capture exit status, run report and action graph;
4. verify local cache hits and affected-only behaviour;
5. compare Windows, macOS and Linux behaviour;
6. document any mismatch between native agent commands and canonical moon tasks.

The experiment should validate the process boundary. It should not introduce command interception, `osf exec`, remote execution or a moon fork.

## Related documents

- Agent Native Operating Model
- Product Lifecycle
- Automated SPDLC and SDLC
- OSF Execution and Verification Architecture
