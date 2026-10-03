# Development container

This repository ships a development container. It has a pinned Rust
toolchain, `git-town`, `gh`, `moon`, `actionlint`, `uv`, Node, `pnpm`, the
`osf` command line tool, and five coding agents built in.

## Open the container

Dev Containers is an open specification for an editor to build and open
a project inside a container. It uses the settings in `.devcontainer/`.
Visual Studio Code and other editors support it, through a menu command
that opens the current folder inside its container.

Open the repository folder in such an editor, then run that command. The
first build compiles `osf` from this repository's own source, so the
first open takes a few minutes.

On Windows, clone the repository inside the WSL filesystem. A Windows
drive runs tests far slower, over a Windows bind mount.

The container user is called `dev`. It is a normal user. It has no
password-less root access, so it cannot install packages as root or
change files that root owns.

## Using a git worktree

A plain `git worktree add` checkout's `.git` is not a directory. It is a
file holding one line, `gitdir: <path>/.git/worktrees/<name>`, naming
the main clone's `.git` by its absolute path on the host. Mounting only
the worktree at `/workspace` breaks every git command inside the
container, because that absolute path does not exist there.

Mount the main clone's `.git` directory read-only at the same absolute
path inside the container that it has on the host. Add this alongside
the worktree mount at `/workspace`, in `devcontainer.json`'s `mounts` or
on `docker run`'s own `-v`. Git then finds the path its worktree file
already names, unchanged.

## What the hooks check

Git hooks live at `/opt/factory/githooks` inside the container, owned by
root. Each hook is a thin call to `osf` and nothing else:

| Hook | What it runs |
|---|---|
| `pre-commit` | `osf verify --stage pre-commit`, which scans staged files for text that must never reach a public repository. |
| `commit-msg` | `osf lint writing` over the commit message file, checking prose style. |
| `pre-push` | `osf verify --stage pre-push`, which scans changed files, changed prose, changed skill folders, and pushed commit messages. |

`git config --show-origin core.hooksPath` may still show a repository's
own setting, for example a self-hosting one under `.osf/hooks`. That
setting no longer matters. The git wrapper, below, forces
`/opt/factory/githooks` on the command line for every git call under the
workspace. A command-line setting always wins over one written to a
config file, local or system.

## What the hooks cannot do

A local hook is not unbypassable. `git commit --no-verify` and
`git push --no-verify` skip a hook, and no git setting can turn that off.
A person with a shell in the container can also edit their own
`~/.gitconfig`. That can point `core.hooksPath` at another folder. It does
not survive a container rebuild, and it does not affect the checks that
run on a pull request.

The real authority is the check that runs on a pull request. A
contributor does not control that check. The local hook exists so a
mistake is found in seconds, at the commit. Without it, the same mistake
is found only minutes later, after a push.

## Coding agents and their hooks

The container installs five coding agents: `dsh`, `omp`, `opencode`,
`codex`, and Claude Code. Every one of them can act as a builder or a
reviewer. `.devcontainer/agents.json` is the one list of them. It also
names the default builder agent, read from the `OSF_DEFAULT_BUILDER`
environment variable, set to `dsh` today.

Each agent's own hook settings call `osf hook`, root-owned and read-only
so the agent itself cannot edit its own wiring:

| Agent | Hook wiring | Path |
|---|---|---|
| Claude Code | managed settings | `/etc/claude-code/managed-settings.json` |
| Codex | a managed hooks directory, forced by a requirements file | `/etc/codex/` |
| dsh | a profile named `factory`, built from `integrations/dsh` | `~/.dsh/profiles/factory` |
| opencode | a plugin named in system-level config | `/etc/opencode/opencode.json` |
| omp | a global hook under the agent's own hooks directory | `~/.omp/agent/hooks/osf-stop` |

`osf hook` only answers two events: `stop`, at the end of a turn, and
`prompt`, when a new one starts. Not every agent's own hook system has an
event for both. Codex has no turn-end event at all, so only `prompt` is
wired for it. The `gap` field on an entry in `agents.json` records a
shortfall like this one.

dsh and omp have no system-wide configuration path of their own. This
image carves one directory, root-owned and read-only, out of each
agent's normal, otherwise writable home directory. It already carves the
Rust toolchain out of an otherwise writable `CARGO_HOME` the same way.
Run `dsh --profile factory` to boot dsh with its hooks wired. omp and
opencode read their hooks and plugin automatically.

## Run the same checks by hand

Every hook is a thin call to `osf`, so the same commands work outside a
hook:

```sh
osf verify --stage pre-commit
osf verify --stage pre-push
osf lint writing path/to/file.md
```

Add `--format human` for readable output in a script or a non-interactive
shell. Run `osf explain <rule-id>` for the full text of one rule, using
the rule id shown in a finding, for example `osf explain long-sentence`.

## Which osf a check uses

The git hooks always call `/opt/factory/bin/osf`, the version pinned and
built into the image at `docker build` time. A hook root-owned and
read-only, running a binary root-owned and read-only, is the whole point
of this container. The section above, "What the hooks cannot do",
explains why.

That pinned binary goes stale on a branch that changes `crates/osf`
itself. The baked-in binary predates the very change a contributor is
trying to test.

On such a branch, build the workspace copy and run it directly, rather
than relying on the hook:

```sh
cargo build --release -p osf
./target/release/osf verify --stage pre-commit
```

The git hooks are not changed to prefer a workspace build automatically.
A branch would then supply its own `osf` to its own pre-commit and
pre-push checks. That is exactly the local, agent-editable check this
project's hooks are built not to trust. The section above makes the same
point about the real authority being the check that runs on a pull
request. CI builds and runs `cargo test -p osf`, and the rest of this
repository's own gates, directly against the branch. It never goes
through the image's older, pinned binary.

## The forced hooks path, and its one boundary

The wrapper adds `-c core.hooksPath=/opt/factory/githooks` to every git
call it lets through, for a repository under the workspace mount.

A command-line setting always wins over one written to a config file,
local or system. This holds even when the repository sets its own
`core.hooksPath`, or when that setting was already in force before the
wrapper ran.

The workspace mount defaults to `/workspace`, matching
`devcontainer.json`'s `workspaceFolder`. It reads from the
`OSF_WORKSPACE_ROOT` environment variable when that is set, for a
container started with a different bind mount.

A repository outside the workspace mount is left alone: no
`-c core.hooksPath=...` is added, and whatever hooks it already has, if
any, run as normal. One example is a throwaway repository a test creates
under `/tmp`. This is what keeps `osf`'s own test suite working inside
the container. Its tests build scratch git repositories under `/tmp`,
and do not expect the container's checks to run against their fixture
content.

## The git wrapper, and its limit

`/opt/factory/bin` comes before the real git on the container's path, and
holds a wrapper called `git`. It refuses `git commit --no-verify`,
`git commit -n`, and `git push --no-verify`, and prints why. Git allows
its own options before the subcommand. One example is `git -c
user.email=x commit ...`. The wrapper looks past those options to find
the real subcommand. It does not only look at the first word.

`git push -n` is short for `--dry-run`, an unrelated and harmless option,
so the wrapper leaves it alone.

The wrapper also refuses a caller-chosen hooks path, by any of the three
routes git offers for it, on any git call:

- `-c core.hooksPath=...`, in any capitalisation of the key. Git treats a
  config key's letters as case-insensitive, and so does this check.
- `--config-env core.hooksPath=SOME_VAR` or
  `--config-env=core.hooksPath=SOME_VAR`, which sets a config value from
  an environment variable instead of a literal.
- The git config environment overrides: `GIT_CONFIG_COUNT`,
  `GIT_CONFIG_KEY_*`, `GIT_CONFIG_VALUE_*`, `GIT_CONFIG_PARAMETERS`,
  `GIT_CONFIG_GLOBAL`, and `GIT_CONFIG_SYSTEM`. The wrapper refuses the
  whole call the moment any one of these is set, even before it looks at
  the command line. A tool that legitimately needs one of these, such as
  some IDE integrations, does not work inside this container. Unset it
  and run the command by hand instead.

The same three keys are also refused for `-c` and `--config-env`, for the
reason they always were. Each one was tested by hand in this container,
and each ran an arbitrary command as part of an ordinary `git commit`:

- `core.hooksPath` repoints every hook, in one call, to a folder of the
  caller's choosing.
- `core.fsmonitor` runs as a command during `git commit`, even a plain
  one with no other flags.
- `core.editor` runs as a command when `git commit` opens an editor.
  That happens whenever `-m` is left off.

Two settings from the same family were also tested. The wrapper leaves
both alone, because neither one applies here:

- `core.pager` was tried against both `git commit` and `git push`,
  including with `--paginate` forced on. It is not a route into either
  command. No output from either command went through it.
- `sequence.editor` was tried against `git commit`. It did not run.
  Git only runs it for an interactive rebase. This wrapper does not
  police that command.

`--git-dir` and `--work-tree` were also checked. Pointing them at a
different folder does not change what the wrapper decides. It still adds
`-c core.hooksPath=/opt/factory/githooks` whenever the call's own working
directory is under the workspace, and that setting still wins regardless
of `--git-dir`/`--work-tree`. So on their own, these two options do not
open a way past the hooks. A shell in the container can already do what
it likes to a folder it owns, with no need for those two options.

Every other `-c` value, such as `user.email`, still works. Setting one
for a single command is still a normal, allowed thing to do.

State this plainly: the wrapper is a speed bump and seals nothing. One thing
defeats it, and the wrapper cannot stop it. Calling the real binary at
its full path, `/usr/bin/git`, skips the wrapper completely.

The wrapper only saves the time between a forgotten check and the same
problem being caught on the pull request. That check, not this wrapper,
is the real boundary. Even that check only reaches as far as the
credential used to push. An agent that holds a push credential can
still push straight past every check in this file. Taking that
credential away from the agent is separate work. This wrapper does not
do it.

## Testing the hooks themselves

`.devcontainer/tests/git-hooks.sh` checks that every hook file is
executable, in the image and in the repository's own copy. It checks that
every `osf` command and flag a hook calls exists in the installed `osf`.
It then runs each hook in a scratch repository. The Dockerfile runs it
during `docker build`. A hook that git stored without the executable bit
fails the build. So does a hook that names a flag `osf` no longer has.

## Testing the wrapper itself

`.devcontainer/tests/git-wrapper.sh` asserts every refusal and
pass-through this page describes. The Dockerfile runs it during
`docker build`, as one of the last steps, so a broken wrapper fails the
build instead of shipping quietly. Run it by hand inside a container
with `sh .devcontainer/tests/git-wrapper.sh`.

## GitHub access

The image holds no GitHub login and no token. `gh` reports that it is not
logged in until the caller passes a token in the `GH_TOKEN` environment
variable. `gh` reads that variable by itself, so `gh auth status` then
succeeds.

Pass the token when you start the container. With `docker run`, add
`-e GH_TOKEN`, so the value comes from your shell and not from the
command line. The `devcontainer.json` file forwards `GH_TOKEN` from the
host shell in the same way. Use a token that can read this repository.
Give a builder the write access it needs, and nothing more.

`osf scan` asks `gh` whether the repository is public. A gate run does
not read `[scan] repository_visibility` from the configuration, so it
always asks. Without `GH_TOKEN`, the scan warns that the visibility is
unknown. Every rule still applies, so the warning does not change the
result.

## Builder tools

One image serves both the builder and the reviewer role. Besides the
coding agents above, it installs `cargo`, `rustfmt`, `clippy`, `osf`,
`moon`, Node, `pnpm`, `git-town`, `gh`, `git`, `actionlint`, `uv`, and
`bun`.

[SkillSpector and SkillEvaluator](https://github.com/NVIDIA/skillspector)
are two NVIDIA tools that check an agent skill, the first for safety and
the second for quality. `osf lint skill` runs both against every skill
it checks, and `uv` is in this image only to install them. `omp`'s own
CLI has a `#!/usr/bin/env bun` shebang, so it runs on Bun rather than
plain Node, unlike every other coding agent here. `bun` is in this image
for that reason alone.

`.devcontainer/tests/coding-agent-tools.sh` and
`.devcontainer/tests/builder-tools.sh` run each tool's version check
during `docker build`. `.devcontainer/tests/agent-hooks.sh` checks the
hook wiring itself: every path in `.devcontainer/agents.json` is
root-owned and read-only, and `osf hook` answers a sample payload for
each event it supports.

A login shell (`bash -l`) resets `PATH` before reading
`/etc/profile.d/*.sh`. That would otherwise drop `osf`, the cargo
toolchain, and every coding agent's own binary for that shell, and
`.devcontainer/profile.d/osf-path.sh` is what puts them back.
