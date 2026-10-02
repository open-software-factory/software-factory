# The writing check inside omp

omp is a coding agent. It began as a fork of pi, an earlier coding
agent. `osf` is a command line tool that lints prose and coding agent
replies. This hook runs `osf` over the reply at the end of every turn.
It asks for a short follow-up when the reply has a problem a reader
would trip on.

The hook is written in strict TypeScript. `pnpm run check` type-checks
it, lints it, and checks its formatting; `pnpm test` builds it and runs
its tests against the compiled output.

## Why a global hook

omp loads a compiled JavaScript file in its global hooks directory as
an extension. That extension can subscribe to `session_stop`. This is
an event fired once per turn, right before the session settles. A
handler for that event can ask omp for one continuation turn. It does
this by returning a block decision. omp caps that at 8 continuations
per turn.

The event carries the last assistant message directly, so the reply is
already in hand. No file to find, no transcript to parse.

## Building it

This repository is a pnpm workspace, so `pnpm install` here installs
from the root lock file.

```
pnpm install
pnpm run build
```

This compiles `src/*.ts` and `test/*.ts` into `dist/`. `dist/` is not
checked in.

## Installing it

Build the package, then copy the compiled hook into its own directory
under the global hooks directory, as `index.js` alongside the decision
module it imports. omp resolves a hooks-directory entry that is itself
a directory through its `index.js`, and does not also treat that
directory's other files as separate top-level hooks:

```
mkdir -p ~/.omp/agent/hooks/osf-stop
cp dist/src/osf-stop.js ~/.omp/agent/hooks/osf-stop/index.js
cp dist/src/check.js ~/.omp/agent/hooks/osf-stop/check.js
```

Put the `osf` binary on the path. The hook reads `OSF_COMMAND` from the
environment only when the binary has a different name.

## What happens at the end of a turn

| The check | What this hook does |
|---|---|
| passes | nothing |
| refuses | returns a block decision, so omp runs one more turn with the reason |
| could not run | logs a warning through `pi.logger`, saying nothing was checked |

That last row is the one to keep. A check that could not run must never
look like a check that ran and found nothing. A missing binary, a
timeout, and an answer this hook cannot parse all say plainly that
nothing was checked.

The one exception is a turn with no assistant text at all. Nothing was
said, so there is nothing to check, and no warning either.

## Loop safety

`session_stop` fires again for each continuation it grants, up to the
cap omp enforces. The event marks `stop_hook_active` on the turns this
hook itself asked for. This hook checks that flag first and skips such a
turn. It never spends the continuation budget on a message it already
checked.

## Checking it

```
pnpm run check
```

This type-checks the source with `tsc` from the `typescript` package at
version 7.0.2, lints it with `oxlint`, and checks its formatting with
`oxfmt`.

## Testing it

```
pnpm test
```

This builds the package, then runs `node --test` against the compiled
tests in `dist/test/`.

## Smoke test

After building, this runs the compiled check module directly against a
sample assistant reply, a clean check result, and a blocked check
result:

```
node -e "import('./dist/src/check.js').then(({ lastAssistantText, readResult }) => { \
  const text = lastAssistantText({ role: 'assistant', content: [{ type: 'text', text: 'Sample assistant reply, not a bare reference.' }] }, []); \
  console.log('extracted:', JSON.stringify(text)); \
  console.log('clean check:', JSON.stringify(readResult({ code: 0, stdout: '' }))); \
  console.log('blocked check:', JSON.stringify(readResult({ code: 0, stdout: JSON.stringify({ decision: 'block', reason: 'message:1: error [unplaceable-reference]' }) }))); \
});"
```

The two decisions this hook makes are plain functions in `src/check.ts`,
which imports nothing from omp. The tests run anywhere, including where
omp is not installed. `src/osf-stop.ts` is wiring and carries no
decision of its own.
