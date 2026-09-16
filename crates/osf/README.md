# osf

The Open Software Factory command line. One static binary with deterministic
checks that run before an agent's work is accepted. Agents cannot read or edit
the rules, because the rules are compiled in.

## Commands

| Command | What it does |
|---|---|
| `osf lint writing [files]` | Checks prose for references without a repository or a label, phrases that only make sense inside one conversation, names used with no description, sentences over 25 words, dashes, arrows, filler, and headings in short texts. Reads standard input when no file is given. Exit code 1 when an error is found. `--json` prints one finding per line. `--strict` treats warnings as errors. `--message` marks the text as a reply to a person, where a heading in a short text is an error. |
| `osf hook stop` | Reads a coding agent's Stop event from standard input and lints the final message. Stop is the event an agent sends when it wants to end its turn. The command refuses the stop when the message has errors, and also when it could not check the message at all (bad input, no message in the event, or no known-names list to check against). The agent gets the findings, or the reason it could not be checked, and rewrites. After two refusals in one turn the message goes through. |

## Wiring the stop check into an agent

The same command works for every agent that has a stop hook. Put the binary on
the path, then:

Agents are listed in the order this project supports them. Each is a coding
agent with a command line of its own.

| Agent | Where the hook goes | Can it be refused? |
|---|---|---|
| dsh | the `@deepseek-ai/dsh-hooks-claude-code` bridge, pointed at the same hooks file | no: the bridge's stop event carries a session id and an empty transcript path, and no message text at all |
| pi | an extension on `agent_end`, which receives the turn's messages, answering with `pi.sendUserMessage` | yes, by sending the findings as the next message |
| omp | an extension under `.omp/hooks/pre/` on `session_stop` | yes: that hook returns `{"decision":"block","reason":...}`, so pass `--answer decision-json` |
| opencode2 | a plugin added with `opencode2 plugin add`, on the `event` hook | no: that hook returns nothing, so the plugin reports the findings only |
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

The file: `--config <file>`, else `OSF_CONFIG`, else `~/.osf/config.toml`,
else the compiled defaults apply. An unknown key is refused; the error
names the bad key and lists the valid ones.

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
