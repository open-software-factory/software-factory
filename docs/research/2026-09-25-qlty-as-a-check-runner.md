---
title: Qlty as a check runner across ecosystems
date: 2026-09-25
kind: research
status: draft
---

# Qlty as a check runner across ecosystems

Research note, compiled 2026-09-25, on qlty, a single command-line tool that runs many linters, formatters and scanners through one interface. Qlty CLI version read: v0.645.0, released 2026-09-23. Commit read at the tip of its main branch: `70886ba8b297e7c6e64b10aef5a90280884af885`, 2026-09-23. Source: https://github.com/qltysh/qlty.

This note covers what qlty is, what it checks by ecosystem, its licence terms, and how its own default picks compare with a per-ecosystem tool list in docs/research/2026-09-25-native-tools-per-verification-slot.md.

## 1. What qlty is and how it runs

Qlty is a single Rust binary. It does not write most of its own lint rules. It wraps existing tools, such as clippy or ruff, and reads their output through a plugin system. Source: https://github.com/qltysh/qlty, https://docs.qlty.sh/what-is-qlty.md.

Install is a shell script, or a container image on GHCR. Docker is not needed to run the linters themselves. Source: docs.qlty.sh, install section.

Main commands:

| Command | Purpose | Source |
|---|---|---|
| `qlty init` | Writes `.qlty/qlty.toml` by detecting languages and existing config in the repository. | https://docs.qlty.sh/cli/commands/init.md |
| `qlty check` | Runs the enabled linters and formatters-as-checks, and reports findings. | https://docs.qlty.sh/cli/commands/check.md |
| `qlty fmt` | Runs formatters and rewrites files. | https://docs.qlty.sh/cli/commands/fmt.md |
| `qlty smells` | Reports maintainability smells: duplication, complexity, structure. | https://docs.qlty.sh/cli/commands/smells.md |
| `qlty metrics` | Reports size and complexity metrics per file or component. | https://docs.qlty.sh/cli/commands/metrics.md |
| `qlty plugins enable/disable/list/upgrade` | Manages which plugins are active, and their pinned versions. | https://docs.qlty.sh/cli/commands/plugins-enable.md |
| `qlty sources fetch` | Pre-fetches plugin definitions, for offline or air-gapped use. | https://docs.qlty.sh/changelog.md, entry dated 2025-02-05 |
| `qlty cache status/dir/clean` | Inspects and clears the local cache. | https://docs.qlty.sh/cli/commands/cache-status.md |
| `qlty githooks install` | Installs a pre-commit hook that formats, and a pre-push hook that checks and auto-fixes. | https://docs.qlty.sh/cli/git-hooks.md |
| `qlty coverage publish` | Uploads coverage data to Qlty Cloud, a separate paid service. | https://docs.qlty.sh/coverage/ci.md |

The config file is `.qlty/qlty.toml`, in TOML format. It names a source repository for plugin definitions, exclude and test patterns, one block per enabled plugin with an optional pinned version, and exclude or triage rules for specific issues. Source: https://docs.qlty.sh/cli/qlty-toml.md.

Each plugin pins its own tool version in a separate, git-hosted source repository, by default `qltysh/qlty-source`. As read on 2026-09-24, clippy was pinned to 1.82.0 and ruff to 0.14.6. A repository using qlty can override this pin in its own `qlty.toml`. Source: https://github.com/qltysh/qlty/tree/main/qlty-plugins/plugins/linters.

Offline use is partial. `qlty sources fetch` plus `--skip-source-fetch` let a run use only what is already on disk for plugin definitions. Most plugin tools are still installed by qlty on first use, through a language runtime it manages, so the first install needs network access. Two tools, clippy and rustfmt, instead shell out to whatever Rust toolchain is already on the machine. Qlty also sends crash reports and usage telemetry by default, unless `QLTY_TELEMETRY=off` is set. Source: https://docs.qlty.sh/cli/concepts/runtimes.md, https://docs.qlty.sh/cli/telemetry.md.

## 2. Default coverage per ecosystem

Qlty's plugin catalogue has 89 linter and formatter plugin directories, as read on 2026-09-24. The table below lists tools qlty supports for each ecosystem. It does not show which tools `qlty init` turns on by default. That question is covered in section 7.

| Ecosystem | Lint | Format | Type check | Security | Gap found |
|---|---|---|---|---|---|
| Rust | clippy | rustfmt | clippy covers this | Semgrep, Trivy, OSV-Scanner, Gitleaks, TruffleHog | none for lint or format |
| .NET (C#) | none | none | none | Trivy, Semgrep, Gitleaks, TruffleHog | no C#-specific linter, formatter, or analyser plugin exists at all |
| Java | Checkstyle, PMD, radarlint-java | google-java-format | PMD and Checkstyle cover this partly | Semgrep, Gitleaks, OSV-Scanner, Trivy, TruffleHog | no SpotBugs plugin |
| TypeScript / JavaScript | ESLint, Biome, Oxc, Knip, radarlint | Biome, Prettier | ESLint plugins only | Semgrep, Gitleaks, OSV-Scanner, Trivy, TruffleHog | no dedicated tsc plugin confirmed |
| Python | Ruff, Flake8, radarlint-python | Black, Ruff | mypy | Bandit, Semgrep, Gitleaks, OSV-Scanner, Trivy, TruffleHog | full coverage, including mypy |
| Go | golangci-lint, radarlint-go | gofmt | golangci-lint covers vet-level checks | Semgrep, Gitleaks, OSV-Scanner, Trivy, TruffleHog | close to full coverage, golangci-lint bundles many linters |

Sources: https://docs.qlty.sh/languages/rust.md, https://docs.qlty.sh/languages/csharp.md, https://docs.qlty.sh/languages/java.md, https://docs.qlty.sh/languages/golang.md, https://docs.qlty.sh/languages/python.md, and the plugin directory listing above.

The plain statement: .NET is not a first-class ecosystem in qlty. It gets generic complexity and duplication metrics, and generic security scanners, and nothing that reads C# syntax for style or correctness. Rust, Python, and Go are the strongest ecosystems. Java and TypeScript are good but not complete.

## 3. Output, exit codes, and scoping

`qlty check --sarif` produces SARIF, the standard JSON format for lint findings, added 2025-06-02. A separate machine-readable JSON output exists and is being extended, but the exact flag name for `qlty check` was not confirmed in the docs read. Source: https://docs.qlty.sh/changelog.md, https://github.com/qltysh/qlty/issues/321.

qlty treats a linter's own "found issues" exit code as CLI success. A genuine crash or missing dependency is reported separately, as a plugin error. `--no-fail` makes a run exit 0 regardless of findings. `--no-error` makes it exit 0 regardless of plugin errors. `--fail-level` sets the minimum severity that triggers a non-zero exit, default the lowest level. Source: https://docs.qlty.sh/cli/debugging.md, https://docs.qlty.sh/cli/commands/check.md.

A plugin that could not run is recorded as a plugin error, kept separate from a plugin that ran and found nothing. This matches the rule that a could-not-read result and a read-and-found-nothing result are different facts. Whether a run with zero runnable plugins can still report success was not stated in plain words in the docs read. Treat that as unconfirmed.

By default, `qlty check` looks only at changed files. `--all` widens it to everything. `--upstream <ref>` compares against a base branch. `--filter` narrows to one plugin. Source: https://docs.qlty.sh/cli/commands/check.md.

## 4. Platforms

Windows support is x86 only, with no arm64 build. macOS needs Sonoma 14 or later. Linux needs kernel 6.1 or later, and glibc 2.38 or later, or the musl build. Amazon Linux 2023 does not meet the glibc floor and needs the container image instead. Two optional plugin languages, Ruby and PHP, need extra manual setup on Windows. Source: https://docs.qlty.sh/cli/system-requirements.md.

## 5. Licence

The qlty CLI is under the Business Source License 1.1, stated in its own licence file at https://github.com/qltysh/qlty/blob/main/LICENSE.md, read 2026-09-24. The Business Source License is not an open-source licence.

Its own terms, as read from that file:

- Licensor: Qlty Software Inc.
- Change Date: 2028-12-10. After this date the code converts to the GNU GPL v3 licence.
- Additional Use Grant: free for effectively all use, including commercial projects, except running the licensed code as a "Code Quality Service," meaning a commercial offering that gives third parties access to its code-quality functionality, or to build an "Artificial Intelligence Coding Service," meaning a commercial offering that lets third parties generate, edit, or review code using machine learning or large-language-model technology.

This is not legal advice. A hosted, commercial offering that runs qlty on behalf of third-party users, to check or drive an AI coding agent's changes, sits close to the wording of both restricted categories above. A legal read is needed before qlty is wired into anything beyond this project's own internal, non-commercial use.

No account is required to use the CLI itself. Qlty Cloud is a separate, optional paid product for hosted dashboards and coverage tracking. Source: https://docs.qlty.sh/cli-vs-cloud.md.

## 6. Maturity, read 2026-09-24

| Metric | Value |
|---|---|
| Stars | 3.2k |
| Forks | 269 |
| Watchers | 54 |
| Open issues | 46 |
| Open pull requests | 33 |
| Commits on main | about 2,599 |
| Recent release cadence | 9 releases in roughly 9 weeks, v0.636.0 to v0.645.0 |

Source: https://github.com/qltysh/qlty, https://github.com/qltysh/qlty/releases, both read 2026-09-24.

An actively maintained project with a small but real user base. Closer to a well-run single-vendor tool than a broad-committer open standard.

## 7. What `qlty init` actually enables by default

Qlty's own docs say `qlty init` picks "a reasonable set of linters and formatters," based on whether a tool's config file already exists, and whether files of that language are present. Checking three example repositories published by the qlty team shows a narrower default than expected. Source: https://docs.qlty.sh/cli/commands/init.

| Example repository | What `qlty init` enabled |
|---|---|
| qltysh/example-rust | clippy, plus universal tools: actionlint, checkov, markdownlint, osv-scanner, prettier, ripgrep, trivy, trufflehog, yamllint. rustfmt was not enabled. |
| qltysh/example-typescript | the same universal set. No TypeScript-specific linter, oxc, eslint, or biome, was enabled, and no tsc. |
| qltysh/example-python | an older config format, enabling only markdownlint, prettier, trufflehog. No Python-specific linter was enabled. |

Sources: https://raw.githubusercontent.com/qltysh/example-rust/main/.qlty/qlty.toml, https://raw.githubusercontent.com/qltysh/example-typescript/main/.qlty/qlty.toml, https://raw.githubusercontent.com/qltysh/example-python/main/.qlty/qlty.toml.

Whether these three files are stale snapshots, or an accurate picture of current behaviour, is unconfirmed. It needs a live `qlty init` run in a fresh repository of each language to settle.

What is solid, regardless of language: qlty's catalogue always offers the same cross-cutting tools. Both `gitleaks` and `trufflehog` exist for secrets, and the example repositories chose `trufflehog`. Both `osv-scanner` and `trivy` exist for dependency scanning, and both were enabled in the Rust and TypeScript examples. `semgrep` and per-language `radarlint-*` plugins exist for static security analysis, but were not enabled by default in any of the three examples.

## 8. qlty's picks compared with a per-ecosystem tool list

A separate note has picked one native, fast tool per ecosystem and per check (docs/research/2026-09-25-native-tools-per-verification-slot.md). This section compares those picks against what qlty's own plugin catalogue offers.

**Rust**

| Pick | In qlty's catalogue? | Note |
|---|---|---|
| clippy | yes | confirmed enabled by `qlty init` |
| rustfmt | yes, but not enabled by default in the example repository | catalogue has it, the default choice does not turn it on |
| cargo-audit | not in the catalogue | qlty offers osv-scanner and trivy instead, which do not check licence policy |
| cargo-deny | not in the catalogue | same as above |

**TypeScript / JavaScript**

| Pick | In qlty's catalogue? | Note |
|---|---|---|
| oxlint | yes, as the `oxc` plugin | not enabled by default in the example repository |
| Biome | yes | covers both lint and format, default status unobserved |
| tsc | yes, listed in the catalogue | not seen enabled in the example config, exact default role unconfirmed |

**Python**

| Pick | In qlty's catalogue? | Note |
|---|---|---|
| ruff | yes | not enabled by default in the example repository |
| ty or pyrefly | neither is in the catalogue | qlty's Python type checker is mypy, an older, slower tool |
| pip-audit | not in the catalogue | qlty uses osv-scanner and trivy instead |

**Go**

| Pick | In qlty's catalogue? | Note |
|---|---|---|
| golangci-lint | yes | matches |
| gofumpt | not in the catalogue | qlty only has plain gofmt |
| govulncheck | not in the catalogue | qlty uses osv-scanner and trivy instead |

The consistent pattern: qlty standardises dependency and vulnerability scanning on two general tools, osv-scanner and trivy, for every ecosystem. A per-ecosystem pick, such as cargo-deny or pip-audit, understands that ecosystem's own advisory database and lockfile format. qlty's general tools trade that depth for one consistent interface across every language. Source: https://github.com/qltysh/qlty/tree/main/qlty-plugins/plugins/linters, and the per-language docs pages cited above.

## 9. Assessment: where qlty fits as a moon task

Decision 0012 already designs a check recogniser for the case qlty cannot cover. MSBuild is the build system .NET projects use, and StyleCop is a common .NET style-rule package. That recogniser looks for MSBuild properties and a StyleCop package reference to recognise a .NET lint check, separate from any qlty plugin. Source: docs/architecture/decisions/0012-slots-check-recognisers-and-slot-attestations.md.

Two things make qlty a partial fit rather than a full one, for lint and format:

- For .NET, qlty has nothing to offer lint or format. It only helps the security slot there, through Trivy and generic secret scanners.
- For Rust, qlty wraps clippy and rustfmt through its own version pin, a second version source on top of whatever toolchain the repository already has. A task that calls `cargo clippy` and `cargo fmt` directly avoids that extra pin.

The strongest case for qlty is as a single task that fills the security slot, secrets plus dependency scanning, across every ecosystem with one command and one config file, rather than as the lint or format slot filler.

Example task definition, Rust, security slot only:

```yaml
tasks:
  qlty-security:
    command: qlty check --all --filter=trivy,gitleaks,osv-scanner,trufflehog --sarif --no-progress
    inputs: ['**/*']
    outputs: ['.qlty/out/**']
    tags: [osf-pre-push, osf-pull-request, osf-slot-security]
```

Risks worth carrying forward: the licence restriction in section 5, version-pin drift between qlty's pinned tool version and the repository's own toolchain, partial offline support, and telemetry that is on by default unless turned off.

## 10. Open items to verify later

- The exact non-zero exit-code behaviour of `qlty check` with zero runnable plugins.
- Whether the three example repositories reflect qlty's current default behaviour, or a stale snapshot.
- The `tsc` plugin's exact default status: present in the catalogue, absent from the one example checked.
