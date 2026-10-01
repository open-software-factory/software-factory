# `.agents/`

This folder is the one home for agent configuration every coding agent shares.

## Skills

[`.agents/skills/`](skills/) holds the skills a contributor's agent uses while working on this repository itself. Most supported agents read this folder directly at the project level.

Claude Code reads only `.claude/skills`, and is the one exception. So `.claude/skills` is a git symlink to `../.agents/skills`, and CI fails the build if it is not a real symlink. On Windows, run `git config core.symlinks true` and turn on Developer Mode before you clone, or the symlink checks out as a plain text file instead.

## Why there is no `CLAUDE.md`

Claude Code reads the root [`AGENTS.md`](../AGENTS.md) file directly, from version 2.1.277 onward. This repository keeps no separate `CLAUDE.md` file for that reason: one set of agent rules, read by every agent, including Claude Code.
