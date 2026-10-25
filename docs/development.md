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

## One container per worktree

The paragraph above mounts the main clone's `.git` read-only. A commit in
the container writes objects, refs, and this worktree's own files there,
so the launcher below mounts it read-write and protects only the config
file.

Run each worktree's build, test, and git work in its own container, so
two worktrees never share a process, a scratch folder, or a leaked
setting. `.devcontainer/worktree-container` starts and drives one. It
is a POSIX `sh` script that calls `docker` (or the engine named in
`OSF_CONTAINER_ENGINE`) and `git`. It is not an osf command: osf starts
no container in this release, and the person or orchestrator supplies it.

What a container holds:

| Mount | Where | Access |
|---|---|---|
| The worktree | `/workspace` | read-write |
| The main clone's `.git` directory | its host path | read-write |
| The main clone's `.git/config` file | its host path | read-only, over the line above |

Nothing else comes from the host. The real home folder and every login
folder stay out. `HOME` is the image's own home for its `dev` user, and
goes away with the container. The name is `osf-wt-<folder>-<checksum of
the full path>`, so two worktrees never share a container, even when
their folders have the same name.

Build the image once, then start a container and run commands in it:

```sh
docker build -t osf-devcontainer -f .devcontainer/Dockerfile .
# or: export OSF_IMAGE=<the pinned image you pulled>

W=/path/to/worktree
.devcontainer/worktree-container start "$W"
.devcontainer/worktree-container exec "$W" -- git status
.devcontainer/worktree-container exec "$W" -- cargo fmt --all -- --check
.devcontainer/worktree-container exec "$W" -- cargo clippy --workspace --all-targets -- -D warnings
.devcontainer/worktree-container exec "$W" -- cargo test --workspace
.devcontainer/worktree-container stop "$W"
```

Leave `"$W"` out to use the current directory's worktree. Start as many
worktrees as you like. Each gets its own container, and they run side by
side.

Set the commit identity once per worktree, in the worktree's own scope.
The shared config is read-only, so `--worktree` is the only scope that
works, and the repository already enables it:

```sh
.devcontainer/worktree-container exec "$W" -- git config --worktree user.name "<builder app name>"
.devcontainer/worktree-container exec "$W" -- git config --worktree user.email "<bot user id>+<slug>[bot]@users.noreply.github.com"
```

To push or call `gh`, set the builder's token in the same shell call.
The script passes `GH_TOKEN` by name, so the value is in no argument and
no file. A token from an app lasts about an hour, so pass a fresh one
on each call rather than only at `start`:

```sh
GH_TOKEN=<token from your token tool> .devcontainer/worktree-container exec "$W" -- \
  sh -c 'gh auth setup-git && git push https://github.com/<owner>/<repo>.git HEAD:refs/heads/<branch>'
```

Push with a full URL and a `HEAD:refs/heads/<branch>` refspec. A push
with `-u` or a `git town` command writes branch settings to the shared
config, and fails (see below).

What is covered:

- A POSIX shell with `git` and a Docker-compatible engine on Linux, macOS,
  and Windows through WSL. The image is Linux, so the container is too.
- The git wrapper and the forced hooks path apply, because the worktree
  is mounted at `/workspace`.

What is not covered:

- A native Windows shell, and Git Bash on Windows: it rewrites the mount
  paths. Use WSL.
- A host user whose id is not 1000. The image's `dev` user is 1000, so
  files in the worktree would be owned by another id.
- Podman: the script uses only options both engines share, but no test
  has run it there.
- A change to the shared config. Anything that writes a branch's tracking
  or `git town` parent, such as `git push -u`, `git branch --set-upstream-to`,
  or `git town hack`, fails with `Device or resource busy`. Do those on
  the host, outside the container, or avoid them. This is the point of
  the read-only mount.
- A hook or a script in the shared `.git/hooks` directory: it is
  read-write. The wrapper forces the hooks path, so git does not run it,
  but a process can still write there.
- Network limits: the container has the engine's default network.
- An image tag: the script defaults to `osf-devcontainer:latest` and does
  not build or pull it.

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
reviewer. `crates/osf/src/agents.rs` is the one list of them, selected
by the `[agents]` table of `osf.toml`. It also names the default builder
agent, read from the `OSF_DEFAULT_BUILDER`
environment variable, set to `dsh` today.

Each agent's own hook settings call `osf hook`, root-owned and read-only
so the agent itself cannot edit its own wiring:

| Agent | Hook file |
|---|---|
| Claude Code | `~/.claude/settings.json` |
| Codex | `/etc/codex/` (managed hooks directory, forced by a requirements file; it stays hand-written because the requirements file refuses any other hook source) |
| dsh | `~/.dsh/profiles/factory` |
| opencode | `~/.config/opencode/opencode.json` |
| omp | `~/.omp/agent/hooks/osf-stop/index.js` |

dsh loads `~/.dsh/cordis.patch.yml` in every profile, so the image deletes
the file the installer writes there and keeps the plugin in the factory
profile only.

`osf hooks install --agents` writes each file at image build from the
`[agents]` table of osf.toml. An agent that osf.toml does not select gets
no file there. Each written file is root-owned and read-only inside a
sticky directory, so dev cannot delete or replace it.

`osf hook` only answers two events: `stop`, at the end of a turn, and
`prompt`, when a new one starts. Not every agent's own hook system has an
event for both. Codex has no turn-end event at all, so only `prompt` is
wired for it.

dsh and omp have no system-wide configuration path of their own. Run
`dsh --profile factory` to boot dsh with its hooks wired. omp and
opencode read their hooks and plugin automatically.

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

`osf verify` reads two variables of its own, and sets the rest for
each task moon runs. Moon cannot tell a task which tag selected it.
Which checkpoint is running instead reaches each task through its own
`--checkpoint <name>` flag. `OSF_INDEX_HASH` and `OSF_COMMITS_HASH` are
only set when the checkpoint running has a use for them, decided the
same way `OSF_BASE` is:

| Variable | Read or set | Holds |
|---|---|---|
| `OSF_STATE_DIR` | Read | Where the journal lives |
| `OSF_MOON` | Read | A moon binary other than the one on `PATH` |
| `OSF_FILES_FROM` | Set | A file with one path to check per line |
| `OSF_BASE` | Set | The commit the checkpoint compares against |
| `OSF_FILES_HASH` | Set | A fingerprint of the file list, a moon cache key every task declares |
| `OSF_INDEX_HASH` | Set at pre-commit | A fingerprint of the staged blobs, `scan-staged`'s own cache key |
| `OSF_COMMITS_HASH` | Set at pre-push and pull-request | A fingerprint of the commit range, `scan-commits`'s own cache key |

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

## Check your setup

Run `osf doctor` in the repository. It is read-only. It prints one line per check:

```sh
osf doctor
osf doctor --json
osf doctor --check hooks
```

It checks the osf version, that the settings load, the enabled agents and their credentials, the git hooks, and whether you are inside the development container. It exits non-zero when a check fails.

Agent binaries and pinned versions are reported as unknown until the launch templates land. That is deliberate, not a pass.

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

A moon task named `review` runs `osf review run` at pre-push. It prints
the verdict, and the push always continues. Pre-push runs inside your
own workspace, so you could change its result there. The real gate is a
separate job.

GitHub always reads a `pull_request_target` workflow from the default
branch. That is why `.github/workflows/review.yml` is the real gate, and
a pull request cannot change what this job does. The workflow runs again
when the pull request text is edited. `review.yml` holds no step. It calls
the reusable workflow `.github/workflows/review-check.yml`, which holds
every job below. See [Calling the review check from another
workflow](#calling-the-review-check-from-another-workflow).

The workflow has two jobs. A `build` job builds the `osf` tool from the
trusted commit of the workflow itself, with no secrets. The pull
request's base commit is used only as what the reviewers diff against.
The one `review` job runs `osf review run` over every configured
reviewer, inside the container. It holds the repository-level
`OSF_REVIEW_*` credentials, and passes into each reviewer container only
that reviewer's provider variable. Its network list holds every
configured provider's host. It checks the findings, decides, and posts.
It alone mints the verifier app's token. The `build` job holds a
read-only token, which it uses to find the work item. A reviewer that
left no answer counts as could-not-run, and could-not-run never passes.

### Why a reviewer could not run

The review posts one verdict per run. A pass posts an approval. A blocking
finding, or a failed lens, requests changes. A run that could not run, with
no quorum or with no reviewer answering, posts a comment and never an
approval. The posted event and the exit code of `osf review run` and
`osf review reduce` come from the same verdict.

When a reviewer's command exits with an error, `osf` reads what the command
printed to standard output and standard error. It puts only a fixed
category in the reason, with the exit code and the seconds it ran, such as
`reviewer 'example' exited with code 1 after 2.1 s: the provider rejected
the credential (HTTP 401 or 403)`. `crates/osf/src/failure.rs` holds the
categories. A harness-independent reader checks JSON fields first, such as
`statusCode`, `status`, `api_error_status`, `error.type` and `error.code`,
and plain text patterns last. A status number counts only with HTTP-status
context, such as `status 401`, `HTTP 429` or `401 Unauthorized`, never as a
number inside a path. A quota or network word counts only in a failure phrase, and an explicit
permission refusal outranks it. A generic not-found error counts as an
unavailable model only when it names the whole word "model". The categories are:

- The provider rejected the credential (HTTP 401 or 403).
- The model is unavailable to this credential.
- The provider reports a quota or rate limit.
- The harness rejected the answer schema.
- The provider could not be reached.
- The harness rejected its command line.
- A home or file permission was refused.
- No known failure pattern in its output.

What this covers: a reviewer whose command exits with a non-zero code, and
a reviewer that timed out, where its partial output points to a category.
What it does not cover: a reviewer that exits with code 0 and prints an
invalid answer, which keeps its own validation reason, and any failure
whose text matches none of the patterns. The last category means that no
pattern matched. It does not mean that nothing failed. In a failure
reason, `osf` never keeps or prints the reviewer's own text, because it can
hold a credential. That covers the console, the step log, the journal, SARIF
and the posted review. Each saved reason also goes through the secret removal
below. The text of a valid finding is different: it is the reviewer's own
words, and it reaches the review output after that same secret removal.

The category is osf's reading of the output, and it can be wrong. So the run
file that `osf review run --out` writes, which CI uploads as the
`review-run-<agent>` artifact, keeps the evidence for each failed attempt in
that attempt's `cause` field, and nowhere else:

- `tail` holds the last 40 lines of the command's standard output followed by
  its standard error. The two streams are read separately, so this is not
  their true interleaving. A line longer than 2000 characters is cut and
  ends with `[line cut]`. The limits are `TAIL_LINES` and `TAIL_LINE_CHARS`
  in `crates/osf/src/failure.rs`.
- `basis` names what made the classifier choose its category. It comes from
  a fixed list and never holds the reviewer's text: a phrase from the
  classifier's own pattern list such as `quota exceeded`, `http status 429`,
  `json status 429`, `json label rate_limit_error` or `json errno eacces`. It
  is empty when the category is the last one.

The tail passes through the same secret removal as every other saved text
before the run file is written: the exact values of the secrets the job
holds, in plain, base64 and hex form, then the pattern redaction, which
replaces a whole line that matches a secret pattern. If that
removal cannot run, the tail becomes a one-line notice, never the raw text.
This covers a command that exited with an error and a command that timed out,
where the tail is its partial output. It does not cover a command that
exited with code 0 and printed an invalid answer, or a command that never
started. The journal, the summary and the posted review do not carry the
tail, and the step log never prints it. Download the artifact to read it.

The one review job turns red when it made attempts and none answered. A
reviewer that is off under `--if-enabled`, or left out for the builder's
family, does not fail. The job decides by quorum.

A reviewer runs in a fresh home that `osf` removes whole when the run ends,
whether the reviewer answered, failed, timed out or never started. If the
removal fails, `osf` prints one line naming its own home folder. CI uploads
only each reviewer's saved answers, never a home.

Each reviewer asks for two rounds for each lens. A reviewer that answered
also runs one more round, the critical round, and saves it marked as
critical. The one review job knows which families answered. So it counts a
family's critical round only when exactly one family answered. This is the
interim policy of decision 0016. In every other case, it ignores the
critical rounds. The cost is one more round for each reviewer for each
lens.

A reviewer runs its selected lenses on a fixed pool of threads. It starts
a lens when a thread is free, up to the configured number at once. Each
lens runs in its own read-only copy of the change. The `[review]` table
sets the pool size and the time limits. `concurrency` is how many lenses
run at once, and its default is 8. `timeout_seconds` is the most the
attempts for one lens may take, a retry included, and its default is 1800
seconds. `osf` gives a retry only the time left under both time limits,
the lens limit and the whole-run limit. `osf` records a retry with no
time left as could-not-run, and the reason names the limit. `osf` stops a
lens that goes past that limit and records it as could-not-run, and the
reason names `timeout_seconds`. `total_timeout_seconds` is the most the
whole reviewer run may take, counted from the first lens start, and its
default is 2700 seconds. When it runs out, `osf` stops every running lens
and records every waiting lens as could-not-run, and the reason names
`total_timeout_seconds`. `osf` refuses a `timeout_seconds` or
`total_timeout_seconds` above 86400 seconds (24 hours) and names the
setting. `osf` checks the cost ceiling before it starts each lens. No
reviewer reports spend yet, so only a ceiling of 0 stops lenses today. A
failure on one lens, including a provider rate limit, does not stop the
other lenses. A panic in one lens is recorded as could-not-run for that
lens, and the other lenses finish. The journal holds a complete event for
each lens and attempt, and each event names the lens. The order of events
does not matter.

```toml
[review]
concurrency = 8
timeout_seconds = 1800
total_timeout_seconds = 2700
```

`osf review reduce` treats every saved file as input to check. It checks
each answer again against the schema and the lens, the same way the
one review job does. The lens name must match. Every criterion needs a
score, and each score must be between 0 and 1. A file counts only for
the reviewer its file name gives, and no two files may carry one name. A
file that fails a check is could-not-run for that reviewer. The reducer
works out the round and the critical flag from the order of the attempts
in the file.

Each saved run records the repository, the pull request number, the base
and head commits, and the CI run id. `osf review run --reviewer` and
`osf review reduce` both take these from `--repository`,
`--pull-request-number`, `--head` and `--ci-run-id`. The repository and
the run id fall back to `GITHUB_REPOSITORY` and `GITHUB_RUN_ID`. The head
must be the commit that is checked out. `osf review reduce` refuses a run
whose record differs from the values it was given in any field, and a run
with no record. The run notes name a saved file for a reviewer outside the
roster, and a lens entry that the change did not select.

Before `osf` saves an answer, and again in `osf review reduce`, it removes
the exact value of every secret the job holds from every field. These are
each reviewer's provider key and the code host's token. `osf` removes
each one in plain, base64 and hex form. The pattern redaction runs as
well. Where an API key in the environment is enough to sign in, `osf`
copies no login file into the reviewer's home.

The reviewer starts in a clean copy of the change. The copy is a temporary
folder. It holds the change's files and no coding agent's settings,
plugins or instruction files. Examples are `.opencode`, `opencode.json`,
`.omp`, `.codex`, `.claude`, `.mcp.json`, `.cursor`, `.dsh`, `AGENTS.md`
and `CLAUDE.md`. `agents.rs` holds the full list. The copy leaves out every
symbolic link. Where an agent documents a switch that ignores project
settings, `osf` passes it too. The quote check still reads the real
checkout at the head commit. Claude Code, opencode and omp have file tools
only: read, grep and glob. They have no
shell. Before the reviewer starts, `osf` writes the commit log and the
output of `git diff --no-color <base>...<head>` to one file in a new
read-only folder, and the prompt gives the path of that file. `osf` pastes
no diff and no file text into the prompt. The prompt holds these items:
the pull request's number, title and body, the base and head commits, the
changed files with their line counts, the work item text, the paths of
the linked decision records, and the lens questions. `osf` redacts a
secret in any of that text. A finding's quote must still exist at the cited file and line in
the checkout, or `osf` drops the finding.

The work item comes from the pull request's own link. The `build` job runs
`osf review work-item` with the job's read-only token. The command reads the
pull request's body from the event. It takes the issue that the `Issue:`
line names, or else the first issue the body closes with a closing keyword.
A pull request whose visible text names several issues in this repository
takes the first: the `Issue:` line wins, and otherwise the first closing
keyword does.
It reads that issue from this repository through the code host's API. It
saves the issue's text, or the reason there is none, as the `work-item`
artifact, and the one review job receives that file read-only. The saved work
item records the issue number and the pull request's head commit. A reviewer
refuses a saved work item that does not match its own pull request text and
commit, and reports the reason. The issue's text is untrusted, like the pull
request's body. The prompt tells the reviewer to treat it as data, and `osf`
redacts a secret in it. A pull request with no readable linked issue makes
the spec and acceptance lens could-not-run, with a reason that says to link
one. A work item whose acceptance section is empty makes the spec and
acceptance lens could-not-run too, with a reason that says the section is
empty.

The job also reads the lens catalogue from the base branch. It reads the
`[agents]` and `[review]` settings from the base branch too. Those settings
are the reviewers, the threshold, the concurrency, the timeout, the total
timeout, the cost ceiling, and the prompt file. A pull request cannot turn
off a lens, lower the threshold, or rewrite its own reviewer's instructions
to pass its own review.

`osf` checks these numbers when it loads them. The threshold must be a
number from 0 to 1. A lens weight must be a finite number that is not
negative. The cost ceiling must be a finite number that is not negative. The
`concurrency`, `timeout_seconds` and `total_timeout_seconds` settings must
each be a whole number greater than zero. A zero or a wrong type for one of
them stops the review as could-not-configure, and the error names the
setting. A value that is not a number, is infinite or is out of range stops
the review as could-not-configure, and the error names the file.

One test starts a real opencode and checks that a plugin file in the change
does not run. It builds only with the `real-agents` feature. Run it inside
the development container, where opencode is installed:

```sh
cargo test -p osf --features real-agents --test review_run
```

Findings that survive verification are posted on the pull request as one
review, with a comment on each finding's own line. A must-fix finding
fails the job. A review that could not run fails the job too, and every
other outcome passes.

A same-repository pull request's one review job reads the
repository-level `OSF_REVIEW_*` secrets and variables. Set up this short
list, then the check is live.

### The review check is advisory

The review check posts its result as a check run and as a review under
the verifier app. It becomes a required check once the rebuilt workflow
is verified: one review job, one check. Until then, no branch protection
or ruleset requires it. A pull request cannot merge while a must-fix
finding is open, or while a reviewer answered could-not-run, once it is
required.

### Required setup

- The verifier app at repository level: `OSF_REVIEW_VERIFIER_APP_ID` and
  `OSF_REVIEW_VERIFIER_APP_PRIVATE_KEY`. The one review job mints a
  short-lived token from this app, to post the review.
- The repository-level reviewer credentials: `OSF_REVIEW_OPENAI_API_KEY`,
  `OSF_REVIEW_CLAUDE_CODE_OAUTH_TOKEN`, `OSF_REVIEW_OPENROUTER_API_KEY`,
  `OSF_REVIEW_DEEPSEEK_API_KEY` and `OSF_REVIEW_CODEX_AUTH_JSON`. One
  review job runs `osf review run` over every configured reviewer. Each
  reviewer container receives only the one variable its agent's
  `credential_env` entry names. `osf` itself reads a key only to remove
  its value from answers.
- Make the one review job a required check once the rebuilt workflow is
  verified. Require every conversation resolved, so a person still looks
  at each finding.

A fork's pull request is reviewed only when the repository variable
`OSF_REVIEW_FORKS` is `true`. The one review job then uses the same
repository-level credentials. The default is off, and the
recommendations below keep it off. A fork's pull request otherwise
fails with one line explaining why.

### Recommended

- Limit the repository-level reviewer credentials to the `main` branch
  ruleset. This stops a pull request branch from widening its own review.
- Use a dedicated key for each reviewer, separate from any other use.
  Give each one a low monthly spending cap on the provider's own site.
  A leaked or misbehaving key then costs little to replace, and its
  cap limits what a runaway review run can spend before anyone
  notices.
- Remove the builder app's `actions_variables: write` permission. The
  review job only ever reads variables.
- Keep fork review off. It is off by default, and only the repository
  variable `OSF_REVIEW_FORKS` set to `true` turns it on. Leave that
  variable unset until the review check is a required check.

### The container image

Most jobs run directly on the runner and log in to `ghcr.io` with their
own token first. That login works whether the image behind it stays
public or turns private later. Every job that uses a container pulls
the same image, named once by digest in the `REVIEW_IMAGE` variable at
the top of `.github/workflows/review-check.yml`. No job names a moving tag.
This is the image that the development container workflow builds from the
`.devcontainer` folder in this repository. It already carries a pinned
Rust toolchain, moon, and every reviewer tool the agent list can enable.
To use a newer image, open a pull request that changes the digest in
`REVIEW_IMAGE`. Take the digest of the manifest list from the build log of
the `devcontainer image` workflow run for the commit you want. The change
is reviewed like any other workflow change.

Each step that runs `osf` in a container goes through `docker run` against
that image. Each docker run gets its own fresh, disposable container. The
`build` job mounts the pinned osf checkout, `osf-src`, read-only, and
`cargo build` writes its own output to a separate `build-out` folder. It uploads the `osf` binary as an artifact, and every later
job downloads that one binary. The one review job mounts
`base` and `pr` read-only. The pull request's tree is
only ever data. A third mount,
`out`, holds the saved reviewer file, the SARIF file, and the review's
own journal and state, written through `OSF_STATE_DIR`. The one review job passes into each
reviewer container only that reviewer's provider variable, and holds the
verifier's token for the result.

### Outbound network access

Every job runs `step-security/harden-runner` as its first step. Its
policy sets `egress-policy` to `block`, with an explicit list of the
hosts that job needs. The `build` job names four GitHub hosts:
`github.com`, `api.github.com`, `codeload.github.com` and
`objects.githubusercontent.com`. It names `ghcr.io` and
`pkg-containers.githubusercontent.com` for the container registry. It
names `index.crates.io`, `static.crates.io` and `crates.io`
for the crates.io registry. The one review job names GitHub and the container registry, and every
configured provider's host. The current list holds `api.openai.com`,
`api.anthropic.com`, `openrouter.ai` and `models.opencode.ai`; an enabled
agent adds its provider's host.

This step-security/harden-runner action needs sudo access on the
runner's own virtual machine to enforce that policy. A job-level
container would leave none of its own steps that access. This is one
reason each job runs directly on the runner instead, using
`docker run` only to run `osf`. Every step then falls
under that one policy. So does the traffic each docker container makes.

### Enable a reviewer

A reviewer is a coding-agent tool, run headless, such as Codex or
Claude Code. `crates/osf/src/agents.rs` is the one list of agents osf can
drive. Each entry holds the agent's command, model family, credential
variables, and login files, so a repository never repeats them. This
repository's `osf.toml` selects from that list under `[agents]`:

```toml
[agents]
enabled = ["dsh", "omp", "opencode", "codex", "claude"]
builder = "dsh"
reviewers = ["codex", "claude", "opencode"]

[agents.models]
claude = "claude-sonnet-5"
opencode = "openrouter/qwen/qwen3-coder-next"
```

`enabled` defaults to every agent in the list, `builder` to dsh, and
`reviewers` to none. A name the list does not hold is an error. So is a
builder or reviewer that is not enabled. A reviewer needs its own tool
installed inside the development container, and the credential its
`credential_env` entry names, the same as it would on a person's own
machine. Run `osf agents list` to see every
agent, with what `osf.toml` selects.

Decision 0016 prefers a reviewer's family to differ from the builder's own
family.

| Reviewer | Family | Model | Reads its key from | Read-only mode, and where it comes from |
|---|---|---|---|---|
| `codex` | openai | its own default | `CODEX_API_KEY`, the variable `codex exec` reads | `--sandbox read-only` outside the container, and `--dangerously-bypass-approvals-and-sandbox` inside it; both from `codex exec --help` |
| `dsh` | deepseek | its own default | `DEEPSEEK_API_KEY` | none: `dsh --help` documents no read-only mode, so it cannot review |
| `claude` | anthropic | `claude-sonnet-5` | `CLAUDE_CODE_OAUTH_TOKEN` | `--restricted`, `--tools Read,Grep,Glob`, `--add-dir` for the diff folder and `--permission-prompts none`, from `claude --help` |
| `opencode` | qwen, from its model | `openrouter/qwen/qwen3-coder-next` | `OPENROUTER_API_KEY` | the `OPENCODE_PERMISSION` setting, with bash denied, from the opencode CLI docs |
| `omp` | from its model | none: it takes no model flag | its own login | `--tools read,grep,glob`, from `omp --help` |

A reviewer runs in a read-only mode, and codex inside the review
container is the one exception: its own sandbox is off there and
the container limits it, as described below.
`agents.rs` records the exact flags or
settings of each agent as data, and `osf` adds them to the agent's
command. An agent with no documented read-only mode has none recorded.
A reviewer list that names it reports could-not-run with the reason "no
read-only mode", and the agent never starts. The `claude`, `opencode`
and `omp` modes give file tools only, with no shell. osf runs codex with
its own sandbox off only on Linux or macOS, and only when
`/opt/factory/bin/osf` is a regular file owned by root that no one else
can write, and `/.dockerenv` exists. The review container is then the
wall and limits codex with: only the codex key in its environment, a
network list of the six GitHub hosts and `api.openai.com`, read-only
mounts of the checkout and the work item, and no container runtime
socket. Anywhere else, including Windows, codex runs with
`--sandbox read-only`. A development container passes the same check,
and its wall can be weaker than the review container's because it can
mount a writable workspace and personal credentials. So a reviewer's
home and environment hold no secret beyond that reviewer's own provider
key. A mode that allowed `git diff` through a shell would also allow
`git diff --output=<file>`, which writes a file. So no mode allows a
shell for git.

The workflow's `env` names the codex version the review image carries,
and the codex job fails when the image's `codex --version` prints
another version.

`agents.rs` holds the check for each
agent as `sandbox_check`. Codex carries one, and `osf` runs it before
codex reviews outside the container. Inside the container the check is
empty.
`osf` also gives each reviewer a temporary folder inside its own home.

Codex, Claude and DeepSeek Harness each run one family. opencode and omp
run models from any family. The model they run decides their family.
`MODEL_FAMILIES` in `agents.rs` maps a model id prefix to a family. For
example, `openrouter/qwen/` maps to qwen, and `claude-` maps to anthropic.
A model that no prefix matches leaves the family unknown. So does no model
at all. Such a reviewer does not run. It is could-not-run, and the journal
gives the reason.

`osf` passes each key to the one reviewer it belongs to, and holds a key
only to remove its value from the reviewer's answer. Each tool reads
its own key, the same way it would outside `osf`. A reviewer whose key
is missing exits on its own. `osf` then counts that reviewer as
could-not-run, and tries the next one. The review as a whole never
stalls on one missing key.

`CLAUDE_CODE_OAUTH_TOKEN` comes from `claude setup-token`, run once
against the adopter's own Claude subscription. Claude Code reads it the
same way it would outside `osf`, and talks to Anthropic directly.

DeepSeek Harness needs one adjustment the other agents do not need. Its
headless profile takes the task as a command-line argument. It never
reads one from standard input. Its entry in the list wraps the call in a
small shell script instead: `sh -c 'exec dsh --profile headless
"$(cat "$1")"' sh {prompt_file}`. That script reads the prompt file
`osf` already writes. It then passes that file's text as the argument
DeepSeek Harness expects.

To write each enabled agent's stop and prompt hook settings, run
`osf hooks install --agents --root <dir>`. It writes one settings file
per enabled agent under `<dir>`, from the same list, and replaces a file
already there.
### The builder's own family is left out

A reviewer from the same family as the change's own builder is not an
independent second opinion. `osf review run` finds the builder's
family before it runs any reviewer.

It reads every commit in the reviewed range. It looks for each
commit's own `Code-Generator:` trailer. It maps the model name in that
trailer to a family, through the model-family table this tool ships. Any
word of the model name can decide the family: a vendor name or a model
id, such as `Acme Coder acme-code-2`, which a repository maps with an
`[[review.builder_family_aliases]]` entry of prefix `acme` and family
`acme-labs`. The shipped table covers each family it lists, and every
form works the same way for each. A name whose words point to two
families is unknown. A commit may
also carry an `Orchestrator:` trailer naming the model that supervised
the work; that line is not read here, because this rule concerns the
model that wrote the change. A
repository's own `osf.toml` can add more names to that table, under
`[review] builder_family_aliases`. The `--builder-family` flag skips
this reading and names the family directly; pass it more than once for
more than one family.

A reviewer whose family matches does not run first for this change. The
review's decision still names every family it found, so a person reading
the result can see why a reviewer sat out. A different family for the
reviewer is a preference, not a requirement, and a reviewer of the
builder's family is a last resort. It counts only when no reviewer of
another family answered the lens, and only with evidence: the summary
lists every configured reviewer of another family and the error text of
each attempt that failed. A reviewer that was never tried is no evidence.
Without the evidence the reviewer is not counted, and the lens is
could-not-run. When no reviewer of another family is configured, the
summary says so, and the reviewer counts. `osf review run` tries the
reviewers of other families first, in the configured order, and starts the
builder's family only afterwards. A single reviewer run (`--reviewer`)
starts a reviewer of the builder's family only when the roster holds no
reviewer of another family; otherwise `osf review reduce` finds that
reviewer's run empty and does not count it.

When no trailer names a known family, or any one commit has none, `osf`
records `unknown`. No reviewer is left out, every configured reviewer is
tried, and the summary says `builder family unknown, independence not
achieved` and names each unknown commit by its short hash and a fixed
cause. It never repeats any text from a trailer, so an author's text
cannot reach a saved run, the journal, the console or a posted review. The
verdict counts like any other. A reviewer counted as the last resort adds
`reviewer family same as builder, independence not achieved` and its
evidence to the summary. A reviewer left out for its family records
`skipped: same model family as the builder (<family>)` in its saved run
and in the posted summary. Findings that point outside the pull request's
diff are posted in the review body, not as inline comments; the body
stays within its limit by listing what fits and counting the rest.

One commit is exempt without a trailer: a merge commit that introduces
nothing of its own. It must have exactly two parents, and
`git merge-tree --write-tree <parent1> <parent2>` must merge them with no
conflict into the same tree as the merge commit. A conflict resolved
either way, a dropped change and an extra edit all fail that test. A merge
with a trailer counts like any commit. Any other merge without a trailer
makes the family unknown, and the reason names the commit. A git older
than 2.38 has no `merge-tree --write-tree`: every merge without a trailer
then counts as unknown, so the review runs without independence. The
note lists up to three commits and counts the rest.

Too few other families can still mean a lens is could-not-run. `decide_lens`
needs answers from two families to score a lens, unless the interim policy
of decision 0016 applies. A reviewer that never answered leaves the lens
could-not-run, and its reason names the family that was left out.

### Pin a reviewer's model

Name the model an agent should use under `[agents.models]`, keyed by the
agent's name:

```toml
[agents.models]
codex = "o4-mini"
```

`osf` passes the agent's own model flag and the model on its command
line. Leaving an agent out lets it use its own default model. An agent
with no model flag, such as dsh, cannot be given one, and naming it here
is an error. Each reviewer's own answer, on the journal, records the
model it actually ran with.

### Change the reviewer's prompt

`osf` ships the prompt frame as a file, `crates/osf/defaults/review-prompt.md`.
The build embeds that file in the `osf` binary, as it does the shipped lens
files, so the one static binary carries its defaults. `osf` loads the prompt
from the first of these that exists:

1. The file that `[review] prompt_file` names, relative to the trusted
   config root.
2. The file `.osf/review-prompt.md` under the trusted config root.
3. The default that ships with `osf`.

To name a different file, set it in `osf.toml`:

```toml
[review]
prompt_file = "docs/review-prompt.md"
```

`osf` reads the file from the base branch only. It reads the lenses the
same way. A pull request cannot rewrite its own reviewer's instructions.
The file holds these placeholders, which `osf` fills in once, left to
right: `{lens_name}`, `{lens_summary}`, `{lens_questions}`,
`{severity_guide}` and `{metadata}`. Only the answer format that `osf`
parses stays in code. `osf` appends it after the file's text, so no file
can break parsing.

### Run one reviewer, then decide

`osf review run` runs every configured reviewer in turn and decides, in
one process. The one review job runs that command. `osf review run
--reviewer <name> --out <file>` runs one reviewer and saves what it did,
for debugging. `osf review reduce <files...>` takes the saved files,
checks the findings against the files, journals, and decides. Pass `--pull-request <file>`, a JSON file with
the `number`, `title` and `body`, to put the pull request in the prompt.

## Calling the review check from another workflow

The review check is a reusable workflow, `.github/workflows/review-check.yml`
in this repository. osf's own `review.yml` calls it, and so does a caller in
any other repository. Both run the same one review job, so osf tests what an
adopter runs.

### The caller

A caller is one job that has no step of its own. It must sit in a workflow
triggered by `pull_request_target`, because GitHub reads that workflow from the
base branch and a pull request cannot change it. Any other event fails the
`build` job with a message.

```yaml
on:
  pull_request_target:
    types: [opened, synchronize, reopened, ready_for_review, edited]
permissions:
  contents: read
jobs:
  review:
    permissions:
      contents: read
      packages: read
      issues: read
      pull-requests: read
    uses: <owner>/<repo>/.github/workflows/review-check.yml@<40-character SHA>
    with:
      osf-ref: <the same 40-character SHA>
      verifier-app-id: ${{ vars.VERIFIER_APP_ID }}
    secrets:
      VERIFIER_APP_PRIVATE_KEY: ${{ secrets.VERIFIER_APP_PRIVATE_KEY }}
      OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}
      CLAUDE_CODE_OAUTH_TOKEN: ${{ secrets.CLAUDE_CODE_OAUTH_TOKEN }}
      OPENROUTER_API_KEY: ${{ secrets.OPENROUTER_API_KEY }}
```

Map each secret by name, as above, and leave out the key of a reviewer that
`[agents]` does not select. `secrets: inherit` passes every secret of the
caller, but GitHub allows it only within one organization or enterprise. A
caller in another organization than the workflow cannot use it. It suits a
caller in the same organization, such as an adopter's own factory repo that
calls a shared workflow for its watched repositories. osf's own caller uses it.

Two references pin the check, and both name the same commit. The `@<SHA>` after
the file name pins the jobs. The `osf-ref` input pins the `osf` that the `build`
job compiles and that the one review job later runs next to its keys. A moving
reference, such as a branch name, is refused for `osf-ref`. The `build` job also
compares `osf-ref` and `osf-repository` with the workflow's own commit and
repository, which GitHub reports as `job.workflow_sha` and
`job.workflow_repository`, and fails when they differ. Change the two references
together. GitHub Enterprise Server does not report these two values yet, so there the check fails and a caller there needs a different pin.

osf's own caller names the file by a local path. GitHub reads a local path at
the same commit as the caller, which is the head of the default branch for
`pull_request_target`. It passes `osf-repository` as its own repository and
`osf-ref` as `github.workflow_sha`, the commit of the caller. A pull request's
base can be any branch, so the base commit is never where `osf` comes from. It
is only the commit the reviewers diff against. A pull request therefore cannot
change the caller, the called jobs, or the `osf` that runs them.

### Inputs

| Input | Required | Default | Why it is an input |
| --- | --- | --- | --- |
| `osf-ref` | yes | none | The commit of osf to build. This value decides which code runs next to the keys. It must be a full commit SHA, and the `build` job refuses any commit but the workflow's own. |
| `osf-repository` | no | `open-software-factory/software-factory` | The repository that holds that commit. It must be the repository of the workflow. Only osf's own caller, and a fork of osf, set it. |
| `runs-on` | no | `"ubuntu-latest"` | The runner label, written as JSON: a string or an array of labels. It must name a Linux runner with Docker. The adopter's own settings choose it, so the caller reads its variable and passes the value. |
| `verifier-app-id` | yes | none | The id of the app that posts the review. It is not a secret. |
| `allow-forks` | no | `false` | Review a fork's pull request too. A fork's pull request fails the check on purpose while this is `false`. |

Left out on purpose:

- The image. It is pinned by digest inside the workflow, together with the
  codex version it carries, so a caller cannot choose what runs beside the keys.
  An osf release moves it.
- The environment names. The jobs that hold a key use `review`, or
  the repository-level settings. A caller cannot name another repository's settings.
- The reviewer list. `[agents]` in the watched repository's `osf.toml` on its
  base branch chooses reviewers, and `osf review run --if-enabled` reads it.
- The repository under review. It is the repository of the calling workflow.

### Secrets

The one review job reads its keys from the repository-level `OSF_REVIEW_*`
secrets and variables. A caller passes nothing: the job reads its own
repository's settings. An empty reviewer key still lets that reviewer run;
it cannot sign in, so `osf review run` records a note that names the empty
variable, and the aggregate verdict follows the quorum policy.

| Secret | Required | Read by |
| --- | --- | --- |
| `OSF_REVIEW_VERIFIER_APP_PRIVATE_KEY` | yes | The one review job, which mints the verifier token. |
| `OSF_REVIEW_OPENAI_API_KEY` | no | The codex reviewer container. |
| `OSF_REVIEW_CLAUDE_CODE_OAUTH_TOKEN` | no | The claude reviewer container. |
| `OSF_REVIEW_OPENROUTER_API_KEY` | no | The opencode reviewer container. |
| `OSF_REVIEW_DEEPSEEK_API_KEY` | no | The DeepSeek Harness reviewer container. |
| `OSF_REVIEW_CODEX_AUTH_JSON` | no | The codex reviewer container, for subscription auth. |

A reviewer key is not required, because a repository can leave a reviewer out of
`[agents]`. A reviewer that is selected and has an empty key still runs. It
cannot sign in, so `osf review run` records a note that names the empty
variable. That reviewer's attempts and job fail, and the aggregate verdict
follows the existing quorum policy, so the other reviewers can still reach
quorum and approve.

The one review job reads the repository-level settings directly, so the
repository's ruleset and its limit to the `main` branch
still apply. [Required setup](#required-setup) lists what the `review`
environment holds.

### What is stable

The input names, their types, and their meaning are stable. A change to an
input's name or meaning is a breaking change, and osf releases it under a new
major version. Adding an optional input is not breaking. The job names and
the artifact names are not part of the contract.

### Private factory repo and its other repositories

A private repository can share its workflows with other private repositories
of the same organisation only after an organisation setting allows it. In the
private factory repo, open Settings, then Actions, then General, and under
"Access" choose "Accessible from repositories in the organization". Without
it, a watched repository's caller cannot use a reusable workflow stored there.
The setting applies to a workflow in the factory repo. The workflow in this
public repository needs no setting.

This slice makes the check callable. Who holds the keys when a factory repo
wraps the check for several watched repositories is not decided here. The
environment secrets above live in the repository that holds the calling
workflow.

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
holds a wrapper called `git`. The wrapper raises the bar on known
argument spellings. It is not a guard. The check that runs on the pull
request is the authority, per decision 0003
(docs/architecture/decisions/0003-deterministic-verification-is-authoritative.md).
Git allows its own
options before the subcommand. One example is `git -c user.email=x
commit ...`. The wrapper looks past those options to find the real
subcommand. It does not only look at the first word.

`git push -n` is short for `--dry-run`, an unrelated and harmless option,
so the wrapper leaves it alone.

The wrapper refuses these spellings of a skipped check:

- The full `--no-verify` and its shorter accepted forms, such as
  `--no-v` and `--no-verif`, on `git commit`, `git merge`, and
  `git push`.
- A short `-n` on `git commit` and `git merge`, alone or in a bundle. One
  example is `-nm`. The wrapper reads a bundle from left to right. It
  stops at a flag that takes a value, because the rest is that value. So
  `-mn` is a message value and is not refused.

The wrapper also refuses a caller-chosen hooks path or repository. It
checks every route it knows, on any git call:

- `-c core.hooksPath=...`, in any capitalisation of the key. Git treats a
  config key's letters as case-insensitive, and so does this check.
- `--config-env core.hooksPath=SOME_VAR` or
  `--config-env=core.hooksPath=SOME_VAR`, which sets a config value from
  an environment variable instead of a literal.
- `--git-dir` and `--work-tree`, with a value or with `=`. Each points
  git at a repository the caller chose.
- `--no-hooks`, `--skip-hooks`, `--hooks-path`, and `--hooks-path=...`.
  Each names a hooks path or skips the hooks.
- The `GIT_DIR`, `GIT_WORK_TREE`, and `GIT_COMMON_DIR` environment
  variables.
- The git config environment overrides: `GIT_CONFIG_COUNT`,
  `GIT_CONFIG_KEY_*`, `GIT_CONFIG_VALUE_*`, `GIT_CONFIG_PARAMETERS`,
  `GIT_CONFIG_GLOBAL`, and `GIT_CONFIG_SYSTEM`. The wrapper refuses all
  of them, before it looks at the command line. One exception applies.
  `GIT_CONFIG_COUNT` is allowed when it carries only the pair
  `core.fsmonitor=false`, which the build tool moon needs to turn the
  file-system monitor off. Any other key or value, or a count that does
  not match the pairs, is refused. Unset the others and run the command
  by hand instead.

The wrapper also refuses an alias, because an alias hides the real
command from the first lookup. A `-c alias.*` or `--config-env alias.*`
setting is refused on the command line. An alias written in a config
file is expanded before the checks run, and the same refusals apply to
its expansion. An alias that starts with `!` is refused, because it
runs a shell command.

The same keys are also refused for `-c` and `--config-env`, for the
reason they always were. Each one was tested by hand in this container,
and each ran an arbitrary command as part of an ordinary `git commit`:

- `core.hooksPath` repoints every hook, in one call, to a folder of the
  caller's choosing.
- `core.fsmonitor` runs as a command during `git commit`, even a plain
  one with no other flags.
- `core.editor` runs as a command when `git commit` opens an editor.
  That happens whenever `-m` is left off.

These settings from the same family were also tested. The wrapper leaves
them alone, because neither one applies here:

- `core.pager` was tried against both `git commit` and `git push`,
  including with `--paginate` forced on. It is not a route into either
  command. No output from either command went through it.
- `sequence.editor` was tried against `git commit`. It did not run.
  Git only runs it for an interactive rebase. This wrapper does not
  police that command.

The `GIT_DIR` variable has one exception. Git exports `GIT_DIR` to a hook
when a commit runs in a linked worktree. The wrapper allows `GIT_DIR`
only when it names the same folder git finds from the current folder. It
refuses a discovery failure and a different folder. `GIT_WORK_TREE` and
`GIT_COMMON_DIR` have no exception.

Every other `-c` value, such as `user.email`, still works. Setting one
for a single command is still a normal, allowed thing to do.

State this plainly: the wrapper raises the bar on known spellings and
seals nothing. One thing defeats it, and the wrapper cannot stop it.
Calling the real binary at its full path, `/usr/bin/git`, skips the
wrapper completely.

The pull-request check is the authority, per decision 0003
(docs/architecture/decisions/0003-deterministic-verification-is-authoritative.md).
The wrapper does not replace it. Even that check only reaches as far as the credential used to
push. An agent that holds a push credential can still push straight past
every check in this file. Taking that credential away from the agent is
separate work. This wrapper does not do it.

Known limit: a worktree outside the workspace. A commit in a git
worktree added outside the workspace root runs no hooks. This is on
purpose. The forced hooks path covers only the workspace root, so osf's
own scratch repositories keep working. The pull request check stays the
authority.

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
coding agents above, it installs `cargo`, `rustfmt`, `clippy`, `kache`,
`osf`, `moon`, Node, `pnpm`, `git-town`, `gh`, `git`, `actionlint`, `uv`,
and `bun`.

`kache` is the Rust compiler cache. Cargo uses it as its `rustc-wrapper`
in the container, so compiler outputs are stored once and restored in
every worktree instead of being rebuilt in each one. The cache lives in
the developer user's home, and every worktree in the same container
shares it. `KACHE_DISABLED=1` bypasses it for one command. The wrapper
changes nothing about how Cargo is run: `cargo build`, `cargo test` and
`cargo clippy` work as usual.

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
hook wiring itself: every enabled agent's hook file, from the `[agents]`
table of `osf.toml` selected from `crates/osf/src/agents.rs`, is
root-owned and read-only, and `osf hook` answers a sample payload for
each event it supports.

A login shell (`bash -l`) resets `PATH` before reading
`/etc/profile.d/*.sh`. That would otherwise drop `osf`, the cargo
toolchain, and every coding agent's own binary for that shell, and
`.devcontainer/profile.d/osf-path.sh` is what puts them back.

## Third-party skills

This repository does not vendor third-party skills. The image build
installs them for the container user, under `~/.agents/skills`, and
`.devcontainer/tests/skills-installed.sh` fails the build if one is
missing or empty. Third-party skills are installed at the full pinned
commit, each skill named by its path, and are not linted by osf; their
quality checks are upstream's.

| Skill folders | Source | Commit |
| --- | --- | --- |
| `archify`, `archify-review` | [tt-a1i/archify](https://github.com/tt-a1i/archify) | `d5a1333d7447c866a765adac7d4d062f2f02e4d2` |

The pins are `ARCHIFY_COMMIT` and `SKILLS_CLI_VERSION` in
`.devcontainer/Dockerfile`. A change to a pin is a reviewed change.
[open-software-factory/software-factory#224 (pinned third-party skill
list)](https://github.com/open-software-factory/software-factory/issues/224)
is the home of the full list.
