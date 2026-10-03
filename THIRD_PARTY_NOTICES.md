# Third-party notices

This file lists work from other projects that this repository copies, and
tools it depends on but does not copy. It follows the intent of the
[REUSE specification](https://reuse.software/) for third-party provenance.

## Copied into this repository

### HumanLayer show-me skill

- Name: `show-me` skill
- Source: `github.com/humanlayer/skills`, path
  `plugins/show-me/skills/show-me/SKILL.md`
- Commit: `bba9d13ab34f0a87f1cc33df4dd196372393ddfc`
- Licence: MIT
- Copyright: HumanLayer, 2026
- Path in this repository: `skills/show-me/SKILL.md`, adapted from the
  upstream source listed here. Its licence text sits beside it at
  `skills/show-me/LICENSE`. The full provenance note and the list of
  changes are at `skills/show-me/README.md`.

## Build-time dependencies, kept out of this repository

- `@coldtea/pr-lens-cli` (`github.com/coldteadotai/pr-lens`), by Ohans
  Emmanuel, MIT licence. `.github/workflows/pr-lens.yml` runs it from npm
  at build time. No source from it is copied into this repository. Thanks
  to its author for the tool.
