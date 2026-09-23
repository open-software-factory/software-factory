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
osf verify --stage pre-commit
osf verify --stage pre-push
osf lint writing path/to/file.md
```

Add `--format human` for readable output in a script or a non-interactive
shell. Run `osf explain <rule-id>` for the full text of one rule, using
the rule id shown in a finding, for example `osf explain long-sentence`.

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
4. In your clone of this repository, run:

   ```sh
   git config core.hooksPath .osf/hooks
   ```

That folder does not exist yet. A later change in this repository adds
it.

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
