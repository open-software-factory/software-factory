# The writing check inside omp

omp is a coding agent. It began as a fork of pi, an earlier coding
agent. `osf` is a command line tool that lints prose and coding agent
replies. This hook runs `osf` over the reply at the end of every turn.
It asks for a short follow-up when the reply has a problem a reader
would trip on.

## Why a global hook

omp loads a TypeScript file in its global hooks directory as an
extension. That extension can subscribe to `session_stop`. This is an
event fired once per turn, right before the session settles. A handler
for that event can ask omp for one continuation turn. It does this by
returning a block decision. omp caps that at 8 continuations per turn.

The event carries the last assistant message directly, so the reply is
already in hand. No file to find, no transcript to parse.

## Installing it

Copy the hook file into the global hooks directory:

```
mkdir -p ~/.omp/agent/hooks
cp osf-stop.ts ~/.omp/agent/hooks/
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
node --test
```

The two decisions this hook makes are plain functions in `src/check.js`,
which imports nothing from omp. The tests run anywhere, including where
omp is not installed. `osf-stop.ts` is wiring and carries no decision of
its own.
