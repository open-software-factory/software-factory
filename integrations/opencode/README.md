# The writing check inside OpenCode

OpenCode is a coding agent. This plugin runs `osf`, a command line tool
that lints prose and coding agent replies, after every turn. It shows a
toast when the reply has a problem a reader would trip on.

## Why this plugin cannot block a turn

A plugin here registers an `event` hook. That hook fires on many bus
events, including `session.idle`, sent once a session is out of queued
work. The hook returns nothing. OpenCode cannot act on any answer from
it. So this plugin cannot hold a turn open the way the dsh and omp
integrations in this project hold theirs.

This plugin reports instead. It reads the session's messages back from
the client, and sends the last assistant reply to `osf`. It shows the
result in a toast. `adviseOnly`, on by default, does not choose between
blocking and advising: blocking is not available from this hook at all.
It only decides how loud the report is.

## Building it

This plugin is written in TypeScript and ships as a compiled package.
Build it once before registering it with OpenCode:

```
npm ci
npm run build
```

`npm run build` writes `dist/src/index.js`, the file OpenCode loads, and
`dist/test/`, the compiled tests. Nothing under `dist/` is checked in.

## Installing it

```
opencode2 plugin add <path to this directory>
opencode2 plugin list
```

If a local path is refused, add its built entry point to the `plugin`
array in `opencode.json` by hand:

```json
{
  "plugin": ["<path to this directory>/dist/src/index.js"]
}
```

then confirm with `opencode2 plugin list`.

## Settings

| Setting | Default | What it does |
|---|---|---|
| `command` | `osf` | The binary. A bare name is resolved on the path. |
| `args` | none | Passed before `hook stop`, for a config file or a name list. |
| `timeoutMs` | 10000 | How long the check may take before this plugin gives up. |
| `adviseOnly` | `true` | Show a toast only, instead of a toast plus a logged line. |

## What happens after a turn

| The check | What this plugin does |
|---|---|
| passes | nothing |
| refuses | shows a toast naming the reason |
| could not run | logs an error line, since a toast alone would be too easy to miss |

The last row is the one to keep. A check that could not run must never
look like a check that ran and found nothing. This plugin cannot block
a turn, so a quiet failure would leave a bad reply looking clean. A
missing binary, a timeout, and an answer this plugin cannot parse all
say plainly that nothing was checked.

The one exception is a turn with no assistant text at all. Nothing was
said, so there is nothing to check, and no report either.

## Checking it

```
npm run check
npm test
```

`npm run check` type-checks with `tsc` from the `typescript` package at
version 7.0.2, lints with `oxlint`, and checks formatting with `oxfmt`.
`npm test` builds, then runs the compiled tests with `node --test`.

The two decisions this plugin makes are plain functions in `src/check.ts`,
which imports nothing from OpenCode. The tests run anywhere, including
where OpenCode is not installed. `src/index.ts` is wiring and carries no
decision of its own.
