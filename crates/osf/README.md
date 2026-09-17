# osf

The Open Software Factory command line. One static binary with deterministic
checks that run before an agent's work is accepted. Agents cannot read or edit
the rules, because the rules are compiled in.

## Commands

| Command | What it does |
|---|---|
| `osf lint writing [files]` | Checks prose for references without a repository or a label, phrases that only make sense inside one conversation, names used with no description, sentences over 25 words, dashes, arrows, filler, and headings in short texts. Reads standard input when no file is given. Exit code 1 when an error is found. `--json` prints one finding per line. `--strict` treats warnings as errors. `--message` marks the text as a reply to a person, where a heading in a short text is an error. |
| `osf hook stop` | Reads a coding agent's Stop event from standard input and lints the final message. Stop is the event an agent sends when it wants to end its turn. The command refuses the stop when the message has errors, and also when it could not check the message at all (bad input, no message in the event, or no known-names list to check against). The agent gets the findings, or the reason it could not be checked, and rewrites. After two refusals in one turn the message goes through. |
| `osf status render` / `apply` / `refresh` | Builds, applies, or refreshes the status block at the top of a pull request description. See "The status block" below. |

## The status block

`osf status` manages the block at the top of a pull request description.
The block says whether a change is ready to merge. It sits between two
HTML comment markers and has six rows.

| Row | Meaning |
|---|---|
| Ready | `yes` when every gate passed, the verdict is APPROVE, and no human review is still required. Otherwise, the one blocking reason. |
| Risk | The blast radius tier from `osf risk`, with its reasons. |
| Verified | The gate results. `all N passed` when every check passed, `no checks reported yet` when there are none, or `P of N passed, failed: name (reason), ...` naming only the failing checks. |
| Review | The review verdict, whether it is advisory, and the latest review round's counts. |
| Problem | One sentence describing the problem the change fixes. |
| Approach | One sentence describing the approach taken. |

`osf status render` builds the block from named inputs. `osf status
apply` puts a rendered block into a description. It goes between the
markers if they are there, or at the top if they are not.

`osf status refresh --repo <owner/name> --pr <number>` recomputes the
block from the pull request's live state. It reads the pull request's own
checks, its review state, and a fresh risk assessment against its base
branch. It reads `Problem` and `Approach` back out of the block already
in the description. A description with no block is left alone, and the
command says so and exits 0. To start a block on such a pull request, pass
`--problem` and `--approach` once, or run `osf status apply`. Refresh compares the new block against the one already
there, byte for byte. It writes nothing when they match, and prints `osf
status refresh: unchanged`. When they differ, it writes the new block and
prints `osf status refresh: updated`.

## The status block workflow

`.github/workflows/status-block.yml` runs `osf status refresh` on a pull
request. It watches three events:

- a pull request opens, updates, or reopens
- a review is submitted or dismissed
- a check run completes

The check-run trigger only starts working once this file reaches the
default branch. GitHub only sends `check_run` events from a workflow file
already there. The other two triggers work right away, from the pull
request branch itself.

## Wiring the stop check into an agent

The same command works for every agent that has a stop hook. Put the binary on
the path, then:

Agents are listed in the order this project supports them. Each is a coding
agent with a command line of its own.

| Agent | Where the hook goes | Can it be refused? |
|---|---|---|
| dsh | the `@deepseek-ai/dsh-hooks-claude-code` bridge, pointed at the same hooks file | no: the bridge's stop event carries a session id and an empty transcript path, and no message text at all |
| pi | an extension on `agent_end`, which receives the turn's messages, answering with `pi.sendUserMessage` | yes, by sending the findings as the next message |
| omp | a hook from `integrations/omp`, installed at `~/.omp/agent/hooks/`, on `session_stop` | yes: that hook returns `{"decision":"block","reason":...}`, so pass `--answer decision-json` |
| opencode2 | a plugin from `integrations/opencode`, added with `opencode2 plugin add`, on the `event` hook | no: that hook returns nothing, so the plugin reports the findings only |
| Codex | `~/.codex/hooks.json`, same shape as Claude Code's `hooks` object. Hooks need trust before they run. | yes, by exit code |
| Claude Code | `hooks.Stop` in `~/.claude/settings.json` or `.claude/settings.json` | yes, by exit code |
| GitHub Copilot CLI | `~/.copilot/hooks/*.json` with an `agentStop` entry | yes, by a JSON decision on standard output |

### The agents do not agree on key names

There is no shared schema for a stop event, and an event names neither its
agent nor its format. Claude Code, Codex and the dsh bridge write
`session_id`. Copilot CLI writes `sessionId`. The plugin interface of
opencode2 writes `sessionID`. The command reads every spelling, so no wiring
needs to translate.

Two refusals are also on offer and they are not interchangeable. An agent
reading the exit code ignores standard output, and an agent reading standard
output treats exit code 2 as the check crashing. The command guesses from the
key spelling, which is right for every agent above. An adapter that builds the
event itself should not rely on the guess: pass `--answer exit-code` or
`--answer decision-json` and the guess is skipped.

A hooks file for Claude Code, Codex and the dsh bridge:

```json
{
  "hooks": {
    "Stop": [
      { "hooks": [ { "type": "command", "command": "osf hook stop --known-names ~/.osf/known-names.txt" } ] }
    ]
  }
}
```

`--known-names` points at a file, one name per line, of project names that need
no description on first use. Everyday names such as GitHub or Rust are built in.

## Configuration

The writing lint's limits and word lists come from four layers, a later one
overriding an earlier one field by field:

1. compiled defaults
2. one TOML file
3. named environment variables
4. command-line flags the user actually passed

`osf config show` prints the values in force, and which layer set each one.

The file: `--config <file>`, else `OSF_CONFIG`, else `osf.toml` at the
current git repository's top level (found with `git rev-parse
--show-toplevel`; skipped, not an error, outside a repository or without
`git`), else `~/.osf/config.toml`, else the compiled defaults apply. An
unknown key is refused; the error names the bad key and lists the valid
ones.

```toml
[writing]
max_sentence_words = 30     # error above this
warn_sentence_words = 20    # warning above this
max_numerals = 2
short_text_words = 500
known_names = ["Vale", "Tauri"]
must_explain_names = ["Linear", "Canny"]  # errors, even under --gate; see below
filler = ["delve", "leverage"]            # replaces the built-in list
chat_local_phrases = ["as discussed"]     # replaces the built-in list
chat_local_labels = ["phase", "item"]     # "phase 2" and the like

[writing.levels]            # error, warning, or off, keyed by rule id
semicolon = "off"
reference-without-link = "error"
```

Today, `known_names` and `writing.levels` change what `osf lint writing` and
`osf hook stop` report. The other fields are resolved and shown by
`osf config show`; wiring them into each rule's own check is later work.

`must_explain_names` is read from the file even under `--gate`, unlike
every other setting here: it can only add `undefined-name` errors, never
remove one, so a change cannot use it to loosen its own gate.

Every environment variable maps to one field:

| Variable | Field |
|---|---|
| `OSF_WRITING_MAX_SENTENCE_WORDS` | `writing.max_sentence_words` |
| `OSF_WRITING_WARN_SENTENCE_WORDS` | `writing.warn_sentence_words` |
| `OSF_WRITING_MAX_NUMERALS` | `writing.max_numerals` |
| `OSF_WRITING_SHORT_TEXT_WORDS` | `writing.short_text_words` |
| `OSF_WRITING_FILLER` | `writing.filler` (comma-separated, replaces the list) |
| `OSF_WRITING_CHAT_LOCAL_PHRASES` | `writing.chat_local_phrases` (comma-separated) |
| `OSF_WRITING_CHAT_LOCAL_LABELS` | `writing.chat_local_labels` (comma-separated) |
| `OSF_WRITING_KNOWN_NAMES` | `writing.known_names` (comma-separated) |
| `OSF_WRITING_MUST_EXPLAIN_NAMES` | `writing.must_explain_names` (comma-separated) |

`osf lint writing` also takes `--max-sentence-words`, `--warn-sentence-words`,
`--max-numerals`, and `--short-text-words`, each overriding the file and the
environment for that one run.

In the container the file is root-owned next to the hooks file, and the hook
command names it with `--config`.

## Building

```sh
cargo build --release
cargo clippy --all-targets
cargo test
```

A static Linux binary:

```sh
cargo build --release --target x86_64-unknown-linux-musl
```
