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

GitHub always reads a `pull_request_target` workflow from the base
branch. That is why `.github/workflows/review.yml` is the real gate, and
a pull request cannot change what this job does.

The workflow has one job for each step. A `build` job builds the `osf`
tool from the base branch only, with no secrets. Each reviewer then runs
in its own job: `review codex`, `review claude` and `review opencode`.
A reviewer job gets only its own provider's key. Its network list holds
only its own provider host and the hosts it needs to run. It saves what
the reviewer did as an artifact, with `osf review run --reviewer <name>
--out <file>`. The last job, named `review`, downloads every saved
file and runs `osf review reduce <files>`. It checks the findings,
decides, and posts. It alone gets the code host's token. A reviewer whose
job left no file counts as could-not-run, and could-not-run never passes.

Each reviewer asks for two rounds for each lens. A reviewer that answered
also runs one more round, the critical round, and saves it marked as
critical. A reviewer job cannot know whether another family answered. So
`osf review reduce` counts a family's critical round only when exactly one
family answered. This is the interim policy of decision 0016. In every
other case, `osf review reduce` ignores the critical rounds. The cost is one
more round for each reviewer for each lens.

`osf review reduce` treats every saved file as input to check. It checks
each answer again against the schema and the lens, the same way the
reviewer job does. The lens name must match. Every criterion needs a
score, and each score must be between 0 and 1. A file counts only for
the reviewer its file name gives, and no two files may carry one name. A
file that fails a check is could-not-run for that reviewer. The reducer
works out the round and the critical flag from the order of the attempts
in the file.

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

The job also reads the lens catalogue from the base branch. It reads the
`[agents]` and `[review]` settings from the base branch too: the reviewers,
the threshold, the timeout, the cost ceiling, and the prompt file. A pull
request cannot turn off a lens, lower the threshold, or rewrite its own
reviewer's instructions to pass its own review.

Findings that survive verification are posted on the pull request as one
review, with a comment on each finding's own line. A must-fix finding
fails the job. A review that could not run fails the job too, and every
other outcome passes.

A same-repository pull request's jobs read their secrets and variables
through a GitHub environment named `review`. A repository admin must
create it. Set up this short list, then the check is live.

### Required setup

- The verifier app. Set `vars.VERIFIER_APP_ID` and
  `secrets.VERIFIER_APP_PRIVATE_KEY` in the `review` environment. The
  job mints a short-lived token from this app, to post the review.
- The `review` environment itself, holding at least two of these keys:
  `secrets.OPENAI_API_KEY`, `secrets.CLAUDE_CODE_OAUTH_TOKEN` (or
  `secrets.ANTHROPIC_API_KEY`), `secrets.OPENROUTER_API_KEY`. Each key
  belongs to one reviewer job. The codex job gives `secrets.OPENAI_API_KEY`
  to codex as `CODEX_API_KEY`. `osf` itself reads a key only to remove its
  value from answers.
- Branch protection that requires the `review` job. Require every
  conversation resolved too, so a person still looks at each finding.

A fork's pull request runs a separate job, also named `review`. It
fails on purpose, with one line explaining why, unless the repository
variable `OSF_REVIEW_FORKS` is `true` and the `fork-review` environment
is also set up. When both are set, the same jobs run, and the reviewer
jobs and the last job use the `fork-review` environment. This is why: GitHub creates a missing environment on
demand, with no protection at all. Without this off switch, a fork's
pull request could run with the model keys before anyone set up
`fork-review` at all. Turning it on is one of the recommendations
below.

### Recommended

- Limit the `review` environment's deployment branches to `main` only.
  This stops a pull request branch from widening its own review.
- Use a dedicated key for each reviewer, separate from any other use.
  Give each one a low monthly spending cap on the provider's own site.
  A leaked or misbehaving key then costs little to replace, and its
  cap limits what a runaway review run can spend before anyone
  notices.
- Remove the builder app's `actions_variables: write` permission. The
  review job only ever reads variables.
- Turn on fork review. Set the repository variable `OSF_REVIEW_FORKS`
  to `true`. Create a `fork-review` environment, with its own copy of
  the required settings above, its deployment branches also limited to
  `main`, and required reviewers. A fork's pull request then waits
  there until a maintainer approves the run.

### The container image

Each job runs directly on the runner. It logs in to `ghcr.io`
with its own token first. That login works whether the image behind
it stays public or turns private later. Every job pulls the same image,
`ghcr.io/open-software-factory/devcontainer:main`. This is the same
image the development container in this repository builds from. It
already carries a pinned Rust toolchain, moon, and every reviewer
tool the agent list can enable.

Each step that runs `osf` goes through `docker run` against that image.
Each run gets its own fresh, disposable container. The `build` job mounts
`base` read-write, and `cargo build` writes its own output there. It
uploads the `osf` binary as an artifact, and every later job downloads
that one binary. A reviewer job and the last job mount `base` and `pr`
read-only. The pull request's tree is only ever data. A third mount,
`out`, holds the saved reviewer file, the SARIF file, and the review's
own journal and state, written through `OSF_STATE_DIR`. A reviewer job
passes only its own provider's key into its container. The last job
passes only the verifier's token.

### Outbound network access

Every job runs `step-security/harden-runner` as its first step. Its
policy sets `egress-policy` to `block`, with an explicit list of the
hosts that job needs. The `build` job names GitHub, the crates.io
registry and the container registry. A reviewer job names GitHub and the
container registry. It names one provider host: `api.openai.com` for
codex, `api.anthropic.com` for claude, and `openrouter.ai` for
opencode. opencode also reads its model catalogue from
`models.opencode.ai`, so that host is on its list. The last job names
GitHub and the container registry.

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
installed and logged in inside the development container, the same as it
would on a person's own machine. Run `osf agents list` to see every
agent, with what `osf.toml` selects.

Decision 0016 needs a reviewer's family to differ from the builder's own
family.

| Reviewer | Family | Model | Reads its key from | Read-only mode, and where it comes from |
|---|---|---|---|---|
| `codex` | openai | its own default | `CODEX_API_KEY`, the variable `codex exec` reads | `--sandbox read-only`, from `codex exec --help` |
| `dsh` | deepseek | its own default | `DEEPSEEK_API_KEY` | none: `dsh --help` documents no read-only mode, so it cannot review |
| `claude` | anthropic | `claude-sonnet-5` | `CLAUDE_CODE_OAUTH_TOKEN` (or `ANTHROPIC_API_KEY`) | `--restricted`, `--tools Read,Grep,Glob`, `--add-dir` for the diff folder and `--permission-prompts none`, from `claude --help` |
| `opencode` | qwen, from its model | `openrouter/qwen/qwen3-coder-next` | `OPENROUTER_API_KEY` | the `OPENCODE_PERMISSION` setting, with bash denied, from the opencode CLI docs |
| `omp` | from its model | none: it takes no model flag | its own login | `--tools read,grep,glob`, from `omp --help` |

Each reviewer runs read-only. `agents.rs` records the exact flags or
settings of each agent as data, and `osf` adds them to the agent's
command. An agent with no documented read-only mode has none recorded.
A reviewer list that names it reports could-not-run with the reason "no
read-only mode", and the agent never starts. The `claude`, `opencode`
and `omp` modes give file tools only, with no shell. The `codex` sandbox
stops file writes and changes. Its shell can still read any file the
reviewer's user can read. So a reviewer's home and environment hold no
secret beyond that reviewer's own provider key. A mode that allowed `git diff` through a shell
would also allow `git diff --output=<file>`, which writes a file. So no
mode allows a shell for git.

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
against the owner's own Claude subscription. Claude Code reads it the
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
trailer to a family, through a small table this tool ships. A
repository's own `osf.toml` can add more names to that table, under
`[review] builder_family_aliases`. The `--builder-family` flag skips
this reading and names the family directly; pass it more than once for
more than one family.

A reviewer whose family matches does not run for this change. The
review's decision still names every family it found, so a person
reading the result can see why a reviewer sat out. When no trailer
names a known family, `osf` records `unknown` and runs the roster the
way it always did: nothing is left out.

Too few other families can then mean the review could-not-run.
`decide_lens` still needs answers from two families to score a lens.
When leaving out the builder's family drops it below two, the lens
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

`osf` ships the prompt frame as a file, `crates/osf/defaults/review-prompt.md`,
built into the binary. A repository replaces it with a file its own
`osf.toml` names, relative to the trusted config root:

```toml
[review]
prompt_file = ".osf/review-prompt.md"
```

`osf` reads the file from the base branch only. It reads the lenses the
same way. A pull request cannot rewrite its own reviewer's instructions.
The file holds these placeholders, which `osf` fills in once, left to
right: `{lens_name}`, `{lens_summary}`, `{lens_questions}`,
`{severity_guide}` and `{metadata}`. Only the answer format that `osf`
parses stays in code. `osf` appends it after the file's text, so no file
can break parsing.

### Run one reviewer, then decide

`osf review run` with no `--reviewer` runs every reviewer in turn and
decides, in one process. The pull request job splits the same work in
two. `osf review run --reviewer <name> --out <file>` runs one reviewer
and saves what it did. `osf review reduce <files...>` takes the saved
files, checks the findings against the files, journals, and decides. Both
reach the same decision. Pass `--pull-request <file>`, a JSON file with
the `number`, `title` and `body`, to put the pull request in the prompt.

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
