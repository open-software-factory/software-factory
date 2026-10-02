# Console design

This folder holds the design work for the operator console. The console is the product's user interface. The console-stack record, `docs/architecture/decisions/0007-console-stack.md`, says how it is built. The operator-console record, `docs/product/decisions/0001-factory-ui-is-an-operator-console.md`, says what it is for.

Read in this order.

1. `DESIGN.md` is the design guidance. It holds the rules the console follows, the tagged candidates, and the open questions.
2. `components.md` is the component catalogue. It lists the layers, what each component owns, and the adoption rule for libraries.
3. `design-tracker.md` is the working state. It holds the checkpoint entries, the review fixes, and the next steps.
4. `design-explorations.md` is the archive of directions that were tried and set aside.
5. `survey/` holds the library and framework surveys, one file per track, with a synthesis in `survey/00-synthesis.md`.

## The design lab

`apps/console-lab/` holds the pages the design work exercises, built with Vite. Each built page is
one self-contained file.

| Page | What it shows |
|---|---|
| `shell.html` | The operator console shell. It uses the placement model in `packages/console-model` and the component package in `packages/console-ui`. |
| `orb.html` | The floating orb on its own, with its variants. |
| `public/spikes/workspace-spike.html` | The docking spike, kept for reference. |
| `public/spikes/workspace-spike-panels.html` | The docking spike with panels, kept for reference. |

The pages read their sample data from `apps/console-lab/src/data/`. Build the lab with
`pnpm --filter @open-software-factory/console-lab build`, then open a page from
`apps/console-lab/dist/`. Use `pnpm --filter @open-software-factory/console-lab dev` for live
reload while editing.

## How a change is verified

The placement model and the component package have Node tests. Run
`pnpm --filter @open-software-factory/console-model test` and
`pnpm --filter @open-software-factory/console-ui test` at the repository root. The lab build and
both test suites run in continuous integration, in the `console` job of `.github/workflows/ci.yml`.

The shell page and the orb page carry their own browser test suites. They are switched off by
default. The design tracker records which suite covers which change.
