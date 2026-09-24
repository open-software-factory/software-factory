---
title: Native tools per verification slot
date: 2026-09-25
kind: research
status: draft
---

# Native tools per verification slot

Research note, compiled 2026-09-25, listing one fast, native or Rust-based tool per verification slot, per ecosystem. A native tool is preferred over a tool that only wraps something else, because it needs one fewer moving part and one fewer version to track.

How to read each table:

- **Recommended default**: the tool to pick first for that slot.
- **Alternatives**: other real options, in the order they were considered.
- **qlty plugin**: whether qlty, the check runner covered in docs/research/2026-09-25-qlty-as-a-check-runner.md, ships a ready-made plugin for it.
- **Notes**: language, maturity, licence, machine-readable output, and whether the tool covers the whole slot or only part of it.

A claim marked unconfirmed could not be checked against a source, and is not stated as fact.

qlty's plugin list, read 2026-09-24: https://github.com/qltysh/qlty/tree/main/qlty-plugins/plugins/linters. Confirmed entries used below: bandit, checkstyle, clippy, eslint, gitleaks, gofmt, golangci-lint, google-java-format, mypy, osv-scanner, oxc, pmd, prettier, ruff, rustfmt, semgrep, trivy, trufflehog, tsc. There is no .NET plugin of any kind in that list.

## Rust

| Slot | Recommended default | Alternatives | qlty plugin | Notes |
|---|---|---|---|---|
| Lint | clippy | | yes | Ships with rustup. MIT or Apache-2.0. JSON diagnostics, no native SARIF. Covers the whole slot. Source: https://github.com/rust-lang/rust-clippy |
| Format | rustfmt | | yes | Ships with rustup. MIT or Apache-2.0. Covers the whole slot. Source: https://github.com/rust-lang/rustfmt |
| Type check | cargo check | cargo build | no | Part of Cargo. Covers the whole slot. Source: https://doc.rust-lang.org/cargo/commands/cargo-check.html |
| Unit tests | cargo nextest | cargo test | no | Apache-2.0 or MIT. Runs each test as its own process. Claims up to 3x faster than cargo test on multi-binary workspaces. Source: https://nexte.st/docs/benchmarks/ |
| Secret scanning | gitleaks | trufflehog, ripsecrets, betterleaks | yes | Written in Go rather than Rust. No mature Rust-native scanner covers the whole slot. MIT, SARIF output. Source: https://www.jit.io/resources/appsec-tools/trufflehog-vs-gitleaks-a-detailed-comparison-of-secret-scanning-tools |
| Dependency audit | cargo-audit | cargo-deny, osv-scanner | no | Apache-2.0 or MIT. Checks Cargo.lock against the RustSec advisory database. Source: https://github.com/rustsec/rustsec |
| Static security analysis | none confirmed as a good fit | semgrep (partial), cargo-geiger (partial) | semgrep: yes | Semgrep is OCaml, not Rust, and its Rust ruleset is thin. cargo-geiger only flags unsafe blocks. Source: https://github.com/semgrep/semgrep |
| Licence audit | cargo-deny | cargo-license | no | Checks each dependency's licence against an allow or deny list. Covers the whole slot. Source: https://github.com/EmbarkStudios/cargo-deny |
| Architecture rules | none confirmed as a good fit | cargo-modules, visualisation only | no | No widely used Rust tool enforces layer rules as a CI gate. See "Rust architecture and dependency-rule tools" below for a deeper look. |
| Mutation testing | cargo-mutants | | no | MIT or Apache-2.0. Covers the whole slot. Source: https://github.com/sourcefrog/cargo-mutants |

## .NET (C#)

| Slot | Recommended default | Alternatives | qlty plugin | Notes |
|---|---|---|---|---|
| Lint | built-in Roslyn analyzers, via `dotnet build` | Meziantou.Analyzer, StyleCop.Analyzers | no | Part of the .NET SDK since .NET 5. MIT. Source: https://learn.microsoft.com/en-us/visualstudio/code-quality/roslyn-analyzers-overview?view=visualstudio |
| Format | dotnet format | | no | Ships with the .NET SDK. MIT. Covers the whole slot. |
| Type check | dotnet build | | no | Uses the Roslyn C# compiler. Covers the whole slot. |
| Unit tests | dotnet test | | no | Native SDK command, runs xUnit, NUnit, or MSTest. |
| Secret scanning | gitleaks | trufflehog | yes | No .NET-native scanner found. |
| Dependency audit | dotnet list package --vulnerable | osv-scanner | no | Built into the SDK since 5.0.200. Uses Microsoft's NuGet advisory data. Source: https://github.com/NuGet/Home/wiki/dotnet-list-package---vulnerable |
| Static security analysis | Security Code Scan | SonarAnalyzer.CSharp, Microsoft DevSkim | no | Roslyn-analyzer-based, MIT. Detects SQL injection, XSS and similar patterns during a normal build. |
| Licence audit | see "NuGet licence-audit tools" below | | no | No dominant, actively maintained native tool was confirmed in the first pass. A deeper look follows. |
| Architecture rules | NetArchTest | ArchUnitNET | no | MIT, a fluent API modelled on ArchUnit, used as ordinary unit-test assertions. Source: https://github.com/BenMorris/NetArchTest |
| Mutation testing | Stryker.NET | | no | Part of the Stryker Mutator family, active as of July 2026. Source: https://github.com/stryker-mutator/stryker-net |

Confirmed gap: qlty has no .NET or C# plugin at all, checked by reading its plugin directory listing. Any .NET default check bundle needs its own wiring, outside qlty.

## Java

| Slot | Recommended default | Alternatives | qlty plugin | Notes |
|---|---|---|---|---|
| Lint | PMD plus Checkstyle, run together | | yes, both | PMD is BSD-style licensed, with SARIF support since v6.31.0. Checkstyle is LGPL-2.1, with SARIF support since 10.3.3. Together they cover the whole slot. Source: https://docs.pmd-code.org/latest/pmd_userdocs_report_formats.html |
| Format | google-java-format | Spotless | yes | Apache-2.0, maintained by Google. No configuration options. |
| Type check | javac, via Maven or Gradle | | no | Native JDK compiler. |
| Unit tests | JUnit 5, via Surefire or Gradle test | | no | Standard JVM test stack. |
| Secret scanning | gitleaks | trufflehog | yes | No Java-native scanner found. |
| Dependency audit | OWASP Dependency-Check | osv-scanner | no | Apache-2.0, an OWASP Flagship project. Checks Maven or Gradle dependencies against NVD-derived data. Source: https://github.com/dependency-check/DependencyCheck |
| Static security analysis | SpotBugs plus find-sec-bugs | Semgrep | partial | SpotBugs is LGPL-2.1, bytecode-level, over 400 bug patterns. find-sec-bugs adds 144 vulnerability types. Source: https://spotbugs.github.io/ |
| Licence audit | none confirmed as dominant | Maven license-maven-plugin, Gradle gradle-license-report | no | Coverage is split by build tool. Verify current maintenance before adopting either. |
| Architecture rules | ArchUnit | | no | Apache-2.0. Runs as ordinary unit tests, asserts package and layer rules. Source: https://github.com/TNG/ArchUnit |
| Mutation testing | PIT (Pitest) | | no | Apache-2.0. Reports are XML or HTML. Whether a SARIF exporter exists is unconfirmed. Source: https://github.com/hcoles/pitest |

## TypeScript / JavaScript

| Slot | Recommended default | Alternatives | qlty plugin | Notes |
|---|---|---|---|---|
| Lint | oxlint | ESLint, Biome lint | yes, as `oxc` | Rust, MIT. Emits SARIF 2.1.0 natively. Type-aware linting went stable in mid-2026, but how complete it is compared with ESLint's typescript-eslint plugin is unconfirmed. Source: https://github.com/oxc-project/oxc |
| Format | Biome formatter | Prettier | yes | Rust, MIT, stable. oxc's own formatter, oxfmt, is not yet ready to ship as a default, see the gap note below. |
| Type check | tsgo, also called TypeScript Native Preview | tsc | no | Written in Go rather than Rust. Claims roughly 10x faster type checking. Still a preview, its general-availability status is unconfirmed. Source: https://devblogs.microsoft.com/typescript/typescript-native-port/ |
| Unit tests | Vitest | node --test, Jest | no | Its transform pipeline uses the Rust-based esbuild, but Vitest itself is not Rust. |
| Secret scanning | gitleaks | trufflehog, ripsecrets, betterleaks | yes | No competitive JavaScript-native scanner. |
| Dependency audit | npm audit, or pnpm or yarn audit | osv-scanner | no | Native to the package manager. |
| Static security analysis | Semgrep | ESLint security plugins, partial | yes | The deepest JavaScript and TypeScript ruleset of any generalist tool, but not itself native. |
| Licence audit | license-checker-rseidelsohn | | no | A maintained fork of the original license-checker, which is unmaintained. The fork runs at reduced pace, re-check before relying on it long term. Source: https://www.npmjs.com/package/license-checker-rseidelsohn |
| Architecture rules | dependency-cruiser | | no | Actively released. Validates and visualises module dependency rules. Source: https://github.com/sverweij/dependency-cruiser |
| Mutation testing | StrykerJS | | no | Apache-2.0, part of the Stryker Mutator family. |

## Python

| Slot | Recommended default | Alternatives | qlty plugin | Notes |
|---|---|---|---|---|
| Lint | ruff | | yes | Rust, MIT. Replaces flake8, pylint, and isort for most rules. Not a full pylint replacement. Source: https://github.com/astral-sh/ruff |
| Format | ruff format | | yes | Part of the same ruff binary. Black-compatible output. |
| Type check | ty, from Astral | pyrefly, from Meta; mypy | no | Rust, beta, claims 10 to 60x faster than mypy or Pyright without caching. pyrefly is also Rust, and reached stable 1.0.0 in May 2026. Both are newer than mypy and may have gaps in unusual typing features. Source: https://astral.sh/blog/ty |
| Unit tests | pytest | | no | Standard and mature, not Rust, but no faster native alternative has comparable ecosystem support. |
| Secret scanning | gitleaks | detect-secrets | yes | detect-secrets is Python-native but has a slow release cadence, treat that as a risk. Source: https://github.com/Yelp/detect-secrets |
| Dependency audit | pip-audit | osv-scanner | no | Maintained by PyPA. Uses the Python Packaging Advisory Database. Source: https://github.com/pypa/pip-audit |
| Static security analysis | bandit | semgrep | yes | Apache-2.0, 47 built-in checks. Native SARIF support. Source: https://github.com/pycqa/bandit |
| Licence audit | pip-licenses | | no | Small but actively released. Source: https://pypi.org/project/pip-licenses/ |
| Architecture rules | import-linter | | no | Enforces import-direction contracts between modules. Source: https://github.com/seddonym/import-linter |
| Mutation testing | mutmut | | no | Actively released. Source: https://github.com/boxed/mutmut |

## Go

| Slot | Recommended default | Alternatives | qlty plugin | Notes |
|---|---|---|---|---|
| Lint | golangci-lint | | yes | A meta-linter, runs many linters in one pass with a shared cache, including staticcheck and gosec. Source: https://github.com/golangci/golangci-lint |
| Format | gofmt | gofumpt | yes, gofmt only | gofmt ships with the Go toolchain. gofumpt is a stricter superset, code passing gofumpt also passes gofmt. Source: https://github.com/mvdan/gofumpt |
| Type check | go build, go vet | | no | Native Go toolchain. |
| Unit tests | go test | | no | Native, built into the Go toolchain. |
| Secret scanning | gitleaks | trufflehog, betterleaks | yes | Gitleaks is itself written in Go. |
| Dependency audit | govulncheck | osv-scanner | no | The official Go team tool. Supports SARIF output since v1.1.1. Does call-graph analysis, so it checks whether a vulnerable code path is reachable, rather than merely present. Source: https://pkg.go.dev/golang.org/x/vuln/internal/sarif |
| Static security analysis | gosec | golangci-lint, which bundles gosec | no, standalone | Supports JSON, SARIF, JUnit-XML, SonarQube and CSV output. Source: https://github.com/securego/gosec |
| Licence audit | go-licenses | | no | Maintained by Google, stated as not an official Google product. Source: https://github.com/google/go-licenses |
| Architecture rules | go-arch-lint | depguard, import-restriction only, partial | no | Enforces declared layer boundaries from a YAML config. Its popularity and health are unconfirmed beyond one release date in July 2026. |
| Mutation testing | gremlins | go-mutesting, inactive since late 2025, do not use | no | Described by its own users as well-maintained. Source: https://github.com/go-gremlins/gremlins |

## Rust architecture and dependency-rule tools

No single, mature, widely adopted tool plays the role ArchUnit plays for Java, or NetArchTest plays for .NET, in Rust. The closest real option today is a combination of three smaller tools.

**cargo-modules** checks a crate's module tree for cycles, with the `--acyclic` flag. 1.3k stars, MPL-2.0, latest release 0.27.0 (2026-08-03). It fails CI on a cycle, but does not check licence or dependency policy. Source: https://github.com/regexident/cargo-modules

**cargo-deny** is the closest thing to a general dependency-policy gate: licence allow and deny lists, banned crates, duplicate versions, and security advisories. It does not check module-level layering. 2.4k stars, MIT or Apache-2.0, latest release 0.20.2 (2026-07-09). Source: https://github.com/EmbarkStudios/cargo-deny

Two small, unrelated projects add explicit layer rules. Both are new and low-adoption.

| Tool | What it checks | Maturity | Licence | Fails CI |
|---|---|---|---|---|
| cargo-archtest-cli | named-layer access rules, cycles, external-crate allow and deny lists | about 663 downloads a month, latest release 0.2.5 (2026-09-17), actively maintained | AGPL-3.0, a strong copyleft licence, check with legal before use in a closed-source pipeline | yes |
| ArchUnitRust, crate name `archunit` | layers, forbidden imports, cycles, public-API boundaries | very new, version 0.0.1 (2026-09-20), about 4 GitHub stars, treat as experimental | MIT | yes, as ordinary `cargo test` rules |

Sources: https://lib.rs/crates/cargo-archtest-cli, https://github.com/LukasNiessen/ArchUnitRust

Two more tools cover a narrower question, a crate's own public API surface, rather than module layering:

- **cargo-public-api** diffs a crate's public API between two commits, to catch an unreviewed API change. 576 stars, MIT, needs a nightly Rust toolchain to build the data it reads. Source: https://github.com/cargo-public-api/cargo-public-api
- **cargo-check-external-types**, from AWS Labs, checks whether a crate's public API leaks types from other crates it depends on. 71 stars, Apache-2.0. Source: https://github.com/awslabs/cargo-check-external-types

One more layer of control needs no extra crate. Clippy's `disallowed_methods`, `disallowed_types`, and `disallowed_macros` ban a specific function, type, or macro by name, each with a reason shown in the warning. They ban a symbol, rather than a direction between modules. Cargo's stable `[workspace.lints]` table lets a workspace set one lint policy that every member crate inherits. Source: https://rust-lang.github.io/rfcs/3389-manifest-lint.html

Bottom line: pair `cargo-deny` for dependency and licence policy with `cargo-modules --acyclic` for cycles. Add `cargo-archtest-cli` or `archunit` only if explicit layer rules are needed, and treat either as early-stage.

## .NET NuGet licence-audit tools

No single, dominant, actively maintained native tool was confirmed in the first pass. A closer look found two tools that read NuGet data directly and are both actively maintained and already widely used for this purpose.

| Tool | Reads NuGet directly | Maturity | Licence | Fail on violation |
|---|---|---|---|---|
| nuget-license, sensslen fork | yes, parses `project.assets.json` | 173 stars, active | Apache-2.0 | has an allow-list option, exact exit code on a violation is unconfirmed |
| dotnet-project-licenses, tomchavakis | yes | 289 stars, marked abandoned by its own README, which points to the sensslen fork above | Apache-2.0 | superseded |
| Trivy | yes, parses `*.deps.json` and `packages.lock.json` | very mature, widely used | Apache-2.0 | yes, with an explicit `--exit-code` flag, its default exit code is 0 even on findings |
| OSV-Scanner | yes, parses `packages.lock.json` | 11.1k stars, the most widely adopted tool in this list | Apache-2.0 | has a `--licenses` allow-list flag, exact exit code on a licence violation is unconfirmed |
| Anchore Grant | no, needs a Syft or SPDX or CycloneDX bill of materials as input | 185 stars | Apache-2.0 | policy-file driven, default exit code unconfirmed |
| cyclonedx-dotnet | generates a bill of materials, it does not check a policy itself | 295 stars | Apache-2.0 | none, pair it with Grant or a similar checker |
| ScanCode Toolkit | unconfirmed whether it has NuGet-specific extraction | 2.6k stars | Apache-2.0 core | a general-purpose licence scanner |

Sources: https://github.com/sensslen/nuget-license, https://github.com/tomchavakis/nuget-license, https://trivy.dev/docs/latest/coverage/language/dotnet/, https://google.github.io/osv-scanner/usage/license-scanning/, https://github.com/anchore/grant, https://github.com/CycloneDX/cyclonedx-dotnet, https://github.com/nexB/scancode-toolkit

Bottom line: Trivy and OSV-Scanner both read NuGet data directly, are actively maintained, and need no separate bill-of-materials step. nuget-license is the right choice if a .NET-only, licence-only tool is preferred over a general vulnerability scanner.

## Gaps: no good native tool exists

1. Rust static security analysis. No Rust-native tool has broad rule coverage. Semgrep is the closest generalist option, but its Rust ruleset is thinner than its Java, Python, or JavaScript coverage.
2. Rust architecture and dependency-rule testing. Covered in depth above. No mature, widely adopted tool exists yet.
3. .NET licence audit. Covered in depth above. Trivy and OSV-Scanner are the strongest confirmed options.
4. Java licence audit. Split across Maven's and Gradle's own plugins. No single cross-build-tool native tool exists, and neither plugin is qlty-integrated.
5. qlty has zero .NET or C# plugin coverage, confirmed by reading its plugin directory.
6. oxc's own formatter, oxfmt, is not yet production-ready. As of mid-September 2026 it still depends on Prettier for Markdown, and its own Rust Markdown formatter is not wired into its command line yet. Do not default to it ahead of Biome or Prettier today.
7. tsgo, also called TypeScript 7, has an unconfirmed general-availability status. It is a genuinely fast native port, written in Go, but whether it has left preview is not confirmed.
8. PIT, the Java mutation-testing tool, has an unconfirmed SARIF output. Its native reports are XML or HTML only.
9. go-arch-lint's popularity and health are unconfirmed beyond one release date in July 2026.
