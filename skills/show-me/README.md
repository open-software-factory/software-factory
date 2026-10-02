# Provenance

`SKILL.md` in this folder is adapted from the `show-me` skill published by
HumanLayer.

- Source repository: `github.com/humanlayer/skills`
- Source path: `plugins/show-me/skills/show-me/SKILL.md`
- Source commit: `bba9d13ab34f0a87f1cc33df4dd196372393ddfc`
- Licence: MIT, copyright HumanLayer, 2026. See `LICENSE` in this folder for
  the full text, copied unchanged from the same source repository.

What osf changed from that commit:

- Reworded the description so it names when to use the skill, instead of
  telling the model what the skill helps with.
- Dropped `disable-model-invocation: true`, since this skill runs the way
  every other one in this repository does.
- Shortened the opening instruction.
- Cut several longer worked examples: a component-tree diff, a file-tree
  diff, a second call-tree diff, a state-diff example, and a standalone
  code-snippet example. One example remains per visual kind.
- Removed the step that opens a saved HTML file with `Bash(open ...)`,
  which assumes a desktop session this project's agents do not have.
- Folded the closing "guidance" heading into plain closing paragraphs.
- Added this provenance note, the SPDX headers in `SKILL.md`, and the
  closing credit line in `SKILL.md`.
