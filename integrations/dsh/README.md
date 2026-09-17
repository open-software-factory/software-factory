# The writing check inside dsh

`dsh`, the DeepSeek Harness, is a coding agent. This plugin runs `osf` over
the agent's reply at the end of every turn, and asks for a short follow-up
when the reply has a problem a reader would trip on.

## Why this exists rather than a hook file

dsh ships a bridge, `@deepseek-ai/dsh-hooks-claude-code`, that runs a hook
file written for another agent. It works, and it is the right way to drive
most hook events.

It cannot drive this one. Its end-of-turn payload carries a session id, an
empty transcript path, and nothing else. There is no assistant text in it
and no path to a file holding any. A check reading that payload has nothing
to read, so it passes every turn without ever looking at a message.

That is the worst way for a check to fail. It does not report an error. It
reports success.

A plugin runs in the same process as the harness, so the reply is already
in memory. This one reads it from the session and sends it to `osf`.

## It does not replace the bridge

dsh delivers the end-of-turn event to every listener in turn, and the
bridge's own listener never stops the chain. Register both and both run.

Register this alone to check the writing. Register the bridge as well to
drive the other hook events from a hook file.

## Building it

The source is TypeScript under `src/`, compiled to `dist/` before use. `dist/`
is not committed; the consumer builds it.

```
npm ci
npm run build
```

`main`, `exports`, and the patch this package ships all point at `dist/`, so
a checkout with no build has nothing to load.

## Installing it

Two steps. Installing the package is not one of them on its own: dsh runs a
plugin only when a configuration row names it.

```
dsh plugin --profile <name> add @open-software-factory/osf-dsh-plugin
```

That adds the package and writes the row, because this package ships its
own `cordis.patch.yml` and points at it from `package.json`.

To write the row by hand instead, put this in the profile's
`cordis.patch.yml`:

```yaml
- insert:
    - id: osf-writing-check
      name: '@open-software-factory/osf-dsh-plugin'
      config:
        command: osf
```

The shape matters. A patch file is a list of entries, and a plugin is added
with an `insert` entry carrying an `id`, the package `name`, and its
`config`. A bare `- name:` row is not read, and dsh does not say so.

To see what loaded, run `dsh --profile <name> --dump-config` and look for
the plugin's name in the composed tree. If it is absent, the row is wrong.

## Settings

| Setting | Default | What it does |
|---|---|---|
| `command` | `osf` | The binary. A bare name is resolved on the path. |
| `args` | none | Passed before `hook stop`, for a config file or a name list. |
| `timeoutMs` | 10000 | How long the check may take before this plugin gives up. |
| `adviseOnly` | `false` | Report what the check says, but never hold the turn. |

`adviseOnly` is there to try the check out without it interrupting anyone.

## What happens at the end of a turn

| The check | What this plugin does |
|---|---|
| passes | nothing |
| refuses | asks the agent for a short follow-up, naming only the points to answer |
| could not run | logs a warning saying nothing was checked |

That last row is the one to keep. A check that could not run looks exactly
like a check that ran and found nothing, so this plugin never lets the two
look alike. A missing binary, a binary that will not start, a timeout and an
unexpected exit code all say plainly that nothing was checked.

The single exception is a turn that said nothing at all: no text, so nothing
to check, and no warning.

## Checking it

```
npm run check
npm run test
```

`check` type-checks the source without emitting, lints it, and checks its
formatting. `test` builds, then runs the compiled tests in `dist/test/`.

Both scripts run with `dist` as the working directory for the actual test
invocation, `node --test` with no path argument. Node finds every
`*.test.js` under the current directory on its own; a directory path handed
to `node --test` (`dist/test`, or even the bare word `test`) is read as a
module name on Windows and fails to load, so no path argument is passed.

The two decisions this plugin makes are plain functions in `src/check.ts`,
which imports nothing from the harness. The tests run anywhere, including
where dsh is not installed. What talks to the harness is wiring and carries
no decision of its own.

## The toolchain

Type-checking and emit use `tsgo`, the native-code preview of the TypeScript
compiler (package `@typescript/native-preview`), pinned to an exact version.
A `typescript`-based fallback script is kept for the day `tsgo` cannot emit
something this package needs; as of this writing `tsgo` builds and
type-checks this package without the fallback.

Linting uses `oxlint` with the `correctness`, `suspicious`, and `pedantic`
rule categories plus its TypeScript plugin, all as errors. Formatting uses
`oxfmt`, run with `--check` so a formatting drift fails the same way a lint
finding does. Both are pinned to exact versions in `package.json`.

`tsconfig.json` turns on the strict compiler settings this project expects:
`strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`,
`noImplicitOverride`, `noPropertyAccessFromIndexSignature`,
`noFallthroughCasesInSwitch`, `useUnknownInCatchVariables`,
`verbatimModuleSyntax`, and `isolatedModules`, targeting `ES2022` with
`NodeNext` modules and resolution.
