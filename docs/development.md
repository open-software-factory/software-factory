# Development container

This repository ships a development container. It has a pinned Rust
toolchain, `git-town`, and the `osf` command line tool built in.

## Open the container

Dev Containers is an open specification for an editor to build and open
a project inside a container. It uses the settings in `.devcontainer/`.
Visual Studio Code and other editors support it, through a menu command
that opens the current folder inside its container.

Open the repository folder in such an editor, then run that command. The
first build compiles `osf` from this repository's own source, so the
first open takes a few minutes.

The container user is called `dev`. It is a normal user. It has no
password-less root access, so it cannot install packages as root or
change files that root owns.

## What the hooks check

Git hooks live at `/opt/factory/githooks` inside the container, owned by
root. Each hook calls `osf` and nothing else:

| Hook | What it runs |
|---|---|
| `pre-commit` | `osf verify --stage pre-commit`, which scans staged files for text that must never reach a public repository. |
| `commit-msg` | `osf lint writing` over the commit message file, checking prose style. |
| `pre-push` | `osf verify --stage pre-push`, which scans changed files, changed prose, changed skill folders, and pushed commit messages. |

Run `git config --show-origin core.hooksPath` to see where this setting
comes from. It comes from the system git configuration, not from this
repository or from the `dev` user's own settings. The `dev` user does not
own that file, so it cannot point the hooks somewhere else.

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

## Run the same checks by hand

Every hook is a thin call to `osf`, so the same commands work outside a
hook:

```sh
osf verify --checkpoint pre-commit
osf verify --checkpoint pre-push
osf lint writing path/to/file.md
```

`osf verify` prints its own plain-text summary. It has no `--format`
flag. `osf lint writing` accepts `--format human` for readable output
in a script or a non-interactive shell. Run `osf explain <rule-id>` for
the full text of one rule, using the rule id shown in a finding, for
example `osf explain long-sentence`.

`osf` removes every `MOON_*` variable it inherits. It does this before
it starts its own moon process. A nested checkpoint run then never
reads an outer one's workspace by mistake.

`osf verify` reads two variables of its own, and sets two more for
each task moon runs. Moon cannot tell a task which tag selected it.
Which checkpoint is running instead reaches each task through its own
`--checkpoint <name>` flag:

| Variable | Read or set | Holds |
|---|---|---|
| `OSF_STATE_DIR` | Read | Where the journal lives |
| `OSF_MOON` | Read | A moon binary other than the one on `PATH` |
| `OSF_FILES_FROM` | Set | A file with one path to check per line |
| `OSF_BASE` | Set | The commit the checkpoint compares against |

## Install moon on the host

The container image already has moon. Outside the container, on a host
machine running Windows, macOS, or Linux, install it by hand:

1. Open the release page for the pinned version:
   `https://github.com/moonrepo/moon/releases/tag/v2.5.5`.
2. Download the archive for your platform. For example, use
   `moon_cli-x86_64-pc-windows-msvc.zip` on Windows, or
   `moon_cli-aarch64-apple-darwin.tar.xz` on an Arm Mac. Extract the
   `moon` binary (`moon.exe` on Windows) from it onto a folder on your
   `PATH`.
3. Check the version: `moon --version` must print `2.5.5`.
4. Run `osf hooks install` (below) once in your clone.

## Install the local git hooks

Outside the development container, run this once per clone:

```sh
osf hooks install
```

It writes the hook scripts osf owns to a folder next to its own state,
outside this repository, and points this repository's own git config at
that folder. It is safe to run again; nothing changes the second time.
Check the setting at any time with `osf hooks install --check`, which
exits non-zero when this repository's hooks do not point at that folder.

Inside the development container, skip this: the container forces its own
hooks path on every git call, so a repository's own `core.hooksPath` has no
effect there.

## Lint a file right after your agent writes it

`osf hook post-tool` runs the hook checkpoint on one file, right after a
tool writes it and before anyone commits it. It reads the tool event from
standard input and finds the written path there. It reports an error on
the agent's next turn.

Add this entry to your coding agent's settings file, alongside `Stop` and
`UserPromptSubmit`:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Write|Edit|MultiEdit|NotebookEdit",
        "hooks": [ { "type": "command", "command": "osf hook post-tool" } ]
      }
    ]
  }
}
```

## Review at pre-push and the pull request

A moon task named `review` runs `osf review run`. It is tagged so it runs
at pre-push and at the pull request, alongside the other checks in this
file.

With no reviewer enabled, the task prints one line and exits clean. No
review runs. With at least one reviewer enabled, a real review runs. A
review that cannot finish fails the checkpoint, the same as any other
task here.

### Enable a reviewer

A reviewer is a coding-agent tool, run headless, such as Claude Code or
Codex. Every shipped reviewer starts disabled. Turn one on in this
repository's `osf.toml`:

```toml
[[review.roster]]
name = "claude-code"
enabled = true
```

The name must match a reviewer this tool already ships, or a new entry
this file adds in full. A reviewer needs its own tool installed and
logged in on whatever machine runs it.

### The pull-request review runs on its own runner

The pull-request review runs as its own job in continuous integration, on
a self-hosted runner inside the development container. That runner
already has the coding-agent tools installed and logged in, the way a
person's own machine does.

The job runs only when the repository variable `OSF_REVIEW_RUNNER` is
`true`. Set it once the runner is registered with the `osf-devcontainer`
label. Until then, pull requests do not wait for a runner that does not
exist.

A repository secret can hold an API key for a cheaper reviewer, such as
one of the `opencode` models. The job passes each key through as an
environment variable. The reviewer's own tool reads it the way it already
does outside `osf`. `osf` never reads or holds a key itself. A missing
secret leaves that one reviewer unable to answer. It does not fail the
job by itself.

### Fork pull requests skip the self-hosted review

This job never runs a fork's code. It runs only when the pull request's
own source repository is this repository. A fork's contributor cannot
reach the self-hosted runner or a reviewer's login through a pull
request. Every other check on a fork's pull request still runs, on a
hosted runner, the same as before.

## The git wrapper, and its limit

`/opt/factory/bin` comes before the real git on the container's path, and
holds a wrapper called `git`. It refuses `git commit --no-verify`,
`git commit -n`, and `git push --no-verify`, and prints why. Git allows
its own options before the subcommand. One example is `git -c
user.email=x commit ...`. The wrapper looks past those options to find
the real subcommand. It does not only look at the first word.

`git push -n` is short for `--dry-run`, an unrelated and harmless option,
so the wrapper leaves it alone.

The wrapper also refuses a `-c` that sets `core.hooksPath`,
`core.fsmonitor`, or `core.editor`, on any git call. This block applies
whatever capitalisation the key is given in. Git treats a config key's
letters as case-insensitive, and so does this check. Each of these three
keys was tested by hand in this container. Each one ran an arbitrary
command as part of an ordinary `git commit`:

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
different folder still left the container's system-wide hooks path in
force for that folder. That path comes from `/etc/gitconfig`. It
applies to every repository, unless something with a stronger claim
overrides it. The wrapper now stops `-c` from being that override. So
on their own, `--git-dir` and `--work-tree` do not open a way past the
hooks. A shell in the container can already do what it likes to a
folder it owns. It does not need those two options to do that.

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
