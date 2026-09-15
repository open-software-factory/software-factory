# osf

The Open Software Factory command line. One static binary with deterministic
checks that run before an agent's work is accepted. Agents cannot read or edit
the rules, because the rules are compiled in.

## Commands

| Command | What it does |
|---|---|
| `osf lint writing [files]` | Checks prose for references without a repository or a label, phrases that only make sense inside one conversation, names used with no description, sentences over 25 words, dashes, arrows, filler, and headings in short texts. Reads standard input when no file is given. Exit code 1 when an error is found. `--json` prints one finding per line. `--strict` treats warnings as errors. |
| `osf hook stop` | Reads a coding agent's Stop event from standard input, lints the final message, and refuses the stop when the message has errors. The agent gets the findings and rewrites. After two refusals in one turn the message goes through. |

## Wiring the stop check into an agent

The same command works for every agent that has a stop hook. Put the binary on
the path, then:

| Agent | Where the hook goes |
|---|---|
| Claude Code | `hooks.Stop` in `~/.claude/settings.json` or `.claude/settings.json` |
| Codex | `~/.codex/hooks.json`, same shape as Claude Code's `hooks` object. Hooks need trust before they run. |
| GitHub Copilot CLI | `~/.copilot/hooks/*.json` with an `agentStop` entry |
| dsh | the `@deepseek-ai/dsh-hooks-claude-code` bridge, pointed at the same hooks file. Its Stop event carries no message text yet, so the check cannot refuse there. |
| omp | a hook under `.omp/hooks/post/` that calls `osf lint writing` on the last message. omp cannot refuse a turn from a hook, so this only reports. |

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
