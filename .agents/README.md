# `.agents/`

This folder is the one home for agent configuration every coding agent shares.

## Skills

[`.agents/skills/`](skills/) holds the skills a contributor's agent uses while working on this repository itself. Claude Code is the one agent known today to read a different folder: `.claude/skills`.

- Keep `.claude/skills` a git symlink to `../.agents/skills`. After you touch it, run `git ls-files -s .claude/skills`. The mode must be `120000`.
- On Windows, run `git config core.symlinks true` and turn on Developer Mode before you clone. Without both, the symlink checks out as a plain text file.
