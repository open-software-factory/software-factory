# UX Benchmark Pack Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and freeze the reproducible `ux-operator-slice-v1` benchmark pack, synthetic factory fixture, neutral React harness, deterministic verification, and sanitized research-record tooling required before calibration runs begin.

**Architecture:** Keep durable methodology and publishable run evidence under `docs/research/ux/benchmark/`. Keep executable fixture, scenario, starter and verification code under `experiments/ux/command-attention/`. A typed fixture and event reducer feed a neutral React provider; candidate UI code consumes that provider without owning scenario semantics. Unit tests validate data and transitions, while Playwright and axe-core validate the browser contract independently of a candidate's visual choices.

**Tech Stack:** Node.js 24.6.0, npm 11.5.1, React 19.2.8, TypeScript 7.0.2, Vite 8.2.1, Vitest 4.1.10, Zod 4.4.3, Playwright 1.62.1, axe-core Playwright 4.13.0.

## Global Constraints

- Implement benchmark version `ux-operator-slice-v1` exactly as specified in `docs/research/ux/benchmark/benchmark-design.md`.
- The first controlled harness is Codex; do not install or run any candidate design workflow in this plan.
- Use a synthetic six-repository, 45-work-item, 14-session, 9-PR fixture. Do not copy files, identifiers, operational values or product details from another workspace.
- The Factory Floor is the persistent spatial backbone. Do not supply candidate-facing components, layouts, graph libraries or visual styling that bias a direction.
- Primary viewport is `1536 × 960`; secondary viewport is `1280 × 800`.
- All evaluated values must resolve to fixture data. Stable entity IDs and explicit trace links are mandatory.
- Deterministic verification is authoritative for schema validity, state transitions, interaction outcomes, accessibility checks and viewport behavior.
- Publishable records contain only synthetic data and sanitized observable evidence. Raw transcripts and recordings stay under ignored `docs/temp-context/ux-benchmark/`.
- Normalize text to LF through the repository `.gitattributes`.
- Pin direct npm dependencies to the exact versions in this plan and commit `package-lock.json`.
- Do not add Storybook, React Flow, a component library, Tauri, a backend, authentication or production Factory Engine types.
- Do not tag the benchmark, push experiment branches or start calibration until the user reviews the frozen pack.

## Plan boundary

This plan implements the reusable inputs and deterministic gates for Stage 1. It stops before installing a candidate workflow, generating a design direction or running the Codex calibration pair. After the frozen pack passes user review, a separate plan will pin the reviewed commit and execute the baseline and Trystan-SA runs. Stage 2 promotion and design-system acceptance remain outside both benchmark-construction and calibration execution.

## Planned file structure

```text
docs/research/ux/benchmark/
  README.md
  benchmark-design.md
  benchmark-pack-implementation-plan.md
  benchmark-brief.md
  domain-glossary.md
  context-manifest.json
  visualization-intent.md
  constraint-sheet.md
  interaction-script.md
  scorecard.md
  capture-format.md
  journal/README.md
  runs/README.md
  runs/run-record.schema.json

experiments/ux/command-attention/
  .gitignore
  README.md
  package.json
  package-lock.json
  eslint.config.js
  index.html
  playwright.config.ts
  tsconfig.app.json
  tsconfig.json
  tsconfig.node.json
  vite.config.ts
  fixture/factory.json
  fixture/scenario.json
  scripts/create-run.ts
  scripts/freeze-context.ts
  scripts/validate-run.ts
  scripts/verify-benchmark.ts
  src/App.tsx
  src/main.tsx
  src/styles/reset.css
  src/benchmark/BenchmarkProvider.tsx
  src/benchmark/ContractTestSurface.tsx
  src/benchmark/actions.ts
  src/benchmark/loadFixture.ts
  src/benchmark/reducer.ts
  src/benchmark/runRecord.ts
  src/benchmark/schema.ts
  src/candidate/CandidateSurface.tsx
  src/test/setup.ts
  src/benchmark/__tests__/fixture.test.ts
  src/benchmark/__tests__/reducer.test.ts
  src/benchmark/__tests__/runRecord.test.ts
  tests/e2e/starter.spec.ts
  tests/e2e/candidate-contract.spec.ts
  tests/e2e/helpers/accessibility.ts
  tests/e2e/helpers/provenance.ts

.github/workflows/ux-benchmark.yml
```

---

### Task 1: Complete the durable benchmark method pack

**Files:**
- Create: `docs/research/ux/benchmark/README.md`
- Create: `docs/research/ux/benchmark/benchmark-brief.md`
- Create: `docs/research/ux/benchmark/domain-glossary.md`
- Create: `docs/research/ux/benchmark/visualization-intent.md`
- Create: `docs/research/ux/benchmark/constraint-sheet.md`
- Create: `docs/research/ux/benchmark/interaction-script.md`
- Create: `docs/research/ux/benchmark/scorecard.md`
- Create: `docs/research/ux/benchmark/capture-format.md`
- Create: `docs/research/ux/benchmark/journal/README.md`
- Create: `docs/research/ux/benchmark/runs/README.md`
- Modify: `docs/research/ux/benchmark/benchmark-design.md`

**Interfaces:**
- Consumes: the approved requirements in `benchmark-design.md`.
- Produces: the human-readable inputs and evaluation contract referenced by the context manifest, run-record tooling and calibration plan.

- [ ] **Step 1: Write a method-pack completeness check**

Create a temporary PowerShell check at `docs/temp-context/ux-benchmark/check-method-pack.ps1` that asserts the ten durable files above exist and verifies these literal requirements occur in the relevant files:

```powershell
$required = @{
  'benchmark-brief.md' = @('Factory Floor', '45 work items', '14 agent sessions', '9 open PRs')
  'domain-glossary.md' = @('work item', 'agent session', 'evidence', 'runway')
  'visualization-intent.md' = @('persistent spatial backbone', 'semantic zoom', 'reduced motion')
  'constraint-sheet.md' = @('1536 × 960', '1280 × 800', 'no supplied component library')
  'interaction-script.md' = @('three clarification questions', 'Direction A', 'natural-language revision', 'annotation')
  'scorecard.md' = @('Product outcome: 74', 'Workflow quality: 19', 'Engineering fitness: 7')
  'capture-format.md' = @('as of August 2026', 'observable tool calls', 'privacy review')
}

foreach ($entry in $required.GetEnumerator()) {
  $path = Join-Path 'docs/research/ux/benchmark' $entry.Key
  if (-not (Test-Path -LiteralPath $path)) { throw "Missing $path" }
  $content = Get-Content -Raw -LiteralPath $path
  foreach ($text in $entry.Value) {
    if (-not $content.Contains($text)) { throw "$path is missing: $text" }
  }
}
```

- [ ] **Step 2: Run the check to verify it fails**

Run:

```powershell
pwsh -File docs/temp-context/ux-benchmark/check-method-pack.ps1
```

Expected: failure on `docs/research/ux/benchmark/README.md` because the method pack does not yet exist.

- [ ] **Step 3: Write the method-pack documents**

Use the approved spec as the source of truth and keep each file single-purpose:

- `README.md`: version, status, file map, freeze/run lifecycle, and the rule that calibration has not started.
- `benchmark-brief.md`: candidate-facing product goal, connected operational slice, synthetic incident, required states and outputs; no score weights or implementation instructions.
- `domain-glossary.md`: concise candidate-facing definitions for work items, sessions, attempts, PRs, checks, deployments, evidence, traceability, attention and runway.
- `visualization-intent.md`: surface/operator-question table, Factory Floor requirements, meaningful-motion rules, visualization exclusions and conceptual references.
- `constraint-sheet.md`: common substrate, viewports, required states, allowed dependency choices, export requirements, accessibility expectations and forbidden implementation assumptions.
- `interaction-script.md`: preflight, maximum three questions, A/B/C directions, selection, first revision, annotation revision and neutral stall nudge.
- `scorecard.md`: all eleven weighted criteria, three evidence columns, three hard gates and instructions for preserving disagreements.
- `capture-format.md`: publishable run directory, required captures, transcript rules, usage fields, private raw evidence location and privacy checklist.
- `journal/README.md`: dated methodology-change and cross-run observation format.
- `runs/README.md`: one directory per sanitized run, immutable run IDs and links to experiment commit SHAs.

Add a short “Derived files” section to `benchmark-design.md` linking to these documents without duplicating their content.

- [ ] **Step 4: Run the method-pack check**

Run:

```powershell
pwsh -File docs/temp-context/ux-benchmark/check-method-pack.ps1
```

Expected: exit 0 with no output.

- [ ] **Step 5: Check links and Markdown whitespace**

Run:

```powershell
rg -n "\]\([^)]*\.md[^)]*\)" docs/research/ux/benchmark
git diff --check
```

Expected: every relative Markdown link resolves when checked from its containing file; `git diff --check` reports nothing.

- [ ] **Step 6: Commit the durable method pack**

```bash
git add docs/research/ux/benchmark
git commit -m "Document UX benchmark method pack"
```

---

### Task 2: Scaffold the neutral, pinned React benchmark harness

**Files:**
- Create: `experiments/ux/command-attention/.gitignore`
- Create: `experiments/ux/command-attention/README.md`
- Create: `experiments/ux/command-attention/package.json`
- Create: `experiments/ux/command-attention/package-lock.json`
- Create: `experiments/ux/command-attention/eslint.config.js`
- Create: `experiments/ux/command-attention/index.html`
- Create: `experiments/ux/command-attention/tsconfig.app.json`
- Create: `experiments/ux/command-attention/tsconfig.json`
- Create: `experiments/ux/command-attention/tsconfig.node.json`
- Create: `experiments/ux/command-attention/vite.config.ts`
- Create: `experiments/ux/command-attention/src/main.tsx`
- Create: `experiments/ux/command-attention/src/App.tsx`
- Create: `experiments/ux/command-attention/src/styles/reset.css`
- Create: `experiments/ux/command-attention/src/candidate/CandidateSurface.tsx`
- Create: `experiments/ux/command-attention/src/test/setup.ts`

**Interfaces:**
- Consumes: Node.js 24.6.0 and npm 11.5.1.
- Produces: `npm run lint`, `npm run test`, `npm run build`, `npm run verify:starter`, and a deliberately unstyled `CandidateSurface` boundary.

- [ ] **Step 1: Create the package manifest with exact versions**

Use this package boundary and scripts:

```json
{
  "name": "@software-factory/ux-command-attention-benchmark",
  "private": true,
  "version": "0.0.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "lint": "eslint .",
    "test": "vitest run",
    "test:watch": "vitest",
    "test:e2e:starter": "playwright test --grep @starter",
    "test:e2e:candidate": "playwright test --grep @candidate",
    "check": "npm run lint && npm run test && npm run build",
    "verify:starter": "npm run check && npm run test:e2e:starter",
    "benchmark:freeze": "tsx scripts/freeze-context.ts",
    "benchmark:verify": "tsx scripts/verify-benchmark.ts",
    "run:create": "tsx scripts/create-run.ts",
    "run:validate": "tsx scripts/validate-run.ts"
  },
  "dependencies": {
    "react": "19.2.8",
    "react-dom": "19.2.8",
    "zod": "4.4.3"
  },
  "devDependencies": {
    "@axe-core/playwright": "4.13.0",
    "@playwright/test": "1.62.1",
    "@testing-library/jest-dom": "7.0.1",
    "@testing-library/react": "16.3.2",
    "@testing-library/user-event": "14.6.4",
    "@types/node": "26.2.0",
    "@types/react": "19.2.18",
    "@types/react-dom": "19.2.4",
    "@vitejs/plugin-react": "6.0.5",
    "eslint": "10.8.1",
    "eslint-plugin-react-hooks": "7.1.1",
    "eslint-plugin-react-refresh": "0.5.4",
    "globals": "17.11.0",
    "jsdom": "30.0.1",
    "tsx": "4.23.12",
    "typescript": "7.0.2",
    "typescript-eslint": "8.67.0",
    "vite": "8.2.1",
    "vitest": "4.1.10"
  }
}
```

- [ ] **Step 2: Install and lock dependencies**

Run from `experiments/ux/command-attention`:

```bash
npm install
npx playwright install chromium
```

Expected: `package-lock.json` is created and Chromium installs successfully.

- [ ] **Step 3: Write the failing starter smoke test**

Add a test next to `App.tsx` that expects the candidate boundary to render the neutral instruction:

```tsx
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { App } from './App';

describe('App', () => {
  it('exposes the candidate surface without a supplied visual system', () => {
    render(<App />);
    expect(screen.getByRole('main')).toHaveTextContent('Candidate surface not implemented');
  });
});
```

- [ ] **Step 4: Run the test to verify it fails**

Run:

```bash
npm test -- src/App.test.tsx
```

Expected: failure because `App` and `CandidateSurface` do not exist.

- [ ] **Step 5: Implement the neutral harness shell**

`CandidateSurface.tsx` must contain no cards, dashboard layout, palette, graph or component abstraction:

```tsx
export function CandidateSurface() {
  return (
    <main>
      <h1>Candidate surface not implemented</h1>
      <p>Use the benchmark provider and fixture; replace this file during a candidate run.</p>
    </main>
  );
}
```

`App.tsx` renders only `CandidateSurface`. `reset.css` applies `box-sizing: border-box`, removes default body margin, and does not set colors, typefaces, spacing scales or component styles.

- [ ] **Step 6: Configure TypeScript, Vite, Vitest and ESLint**

Configure:

- strict TypeScript with `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes` and `noFallthroughCasesInSwitch`;
- Vitest `jsdom` environment and `src/test/setup.ts` importing `@testing-library/jest-dom/vitest`;
- the React Vite plugin;
- ESLint recommended JavaScript, TypeScript, hooks and refresh rules;
- output under ignored `dist/`, `coverage/`, `playwright-report/` and `test-results/`.

- [ ] **Step 7: Run the harness checks**

Run:

```bash
npm run lint
npm test -- src/App.test.tsx
npm run build
```

Expected: all three commands exit 0.

- [ ] **Step 8: Commit the neutral harness**

```bash
git add experiments/ux/command-attention
git commit -m "Add neutral UX benchmark harness"
```

---

### Task 3: Define and author the typed synthetic factory fixture

**Files:**
- Create: `experiments/ux/command-attention/src/benchmark/schema.ts`
- Create: `experiments/ux/command-attention/src/benchmark/loadFixture.ts`
- Create: `experiments/ux/command-attention/src/benchmark/__tests__/fixture.test.ts`
- Create: `experiments/ux/command-attention/fixture/factory.json`
- Create: `experiments/ux/command-attention/fixture/scenario.json`
- Modify: `experiments/ux/command-attention/README.md`

**Interfaces:**
- Consumes: Zod 4.4.3.
- Produces: `FactoryFixture`, `ScenarioDefinition`, `FixtureMode`, `loadFactoryFixture(): FactoryFixture`, `loadScenario(): ScenarioDefinition`, `collectEntityIds()` and `resolveBrokenReferences()`.

- [ ] **Step 1: Write fixture invariants as failing tests**

The tests must assert exact counts and referential integrity:

```ts
const allEntityIds = collectEntityIds(fixture);
expect(fixture.repositories).toHaveLength(6);
expect(fixture.workItems).toHaveLength(45);
expect(fixture.agentSessions).toHaveLength(14);
expect(fixture.pullRequests).toHaveLength(9);
expect(new Set(allEntityIds).size).toBe(allEntityIds.length);
expect(resolveBrokenReferences(fixture)).toEqual([]);
expect(fixture.attentionItems.filter(item => item.severity === 'critical')).toHaveLength(1);
expect(fixture.traceLinks.some(link => link.fromId === 'intent-sync-integrity' && link.toId === 'work-sync-epic')).toBe(true);
```

Also assert that no string in either fixture matches an email address, Windows/Unix home path, public IP address, or the names scanned by the repository privacy check.

- [ ] **Step 2: Run the fixture test to verify it fails**

Run:

```bash
npm test -- src/benchmark/__tests__/fixture.test.ts
```

Expected: failure because the schemas and fixture files do not exist.

- [ ] **Step 3: Define the schemas and inferred types**

Use Zod as the single runtime/type source. Export schemas and inferred types for:

```ts
export type Repository = z.infer<typeof RepositorySchema>;
export type WorkItem = z.infer<typeof WorkItemSchema>;
export type AgentSession = z.infer<typeof AgentSessionSchema>;
export type PullRequest = z.infer<typeof PullRequestSchema>;
export type VerificationCheck = z.infer<typeof VerificationCheckSchema>;
export type Runner = z.infer<typeof RunnerSchema>;
export type Deployment = z.infer<typeof DeploymentSchema>;
export type AttentionItem = z.infer<typeof AttentionItemSchema>;
export type Evidence = z.infer<typeof EvidenceSchema>;
export type TraceLink = z.infer<typeof TraceLinkSchema>;
export type MetricSeries = z.infer<typeof MetricSeriesSchema>;
export type FactoryEvent = z.infer<typeof FactoryEventSchema>;
export type FactoryFixture = z.infer<typeof FactoryFixtureSchema>;
export type ScenarioDefinition = z.infer<typeof ScenarioDefinitionSchema>;
export type FixtureMode = z.infer<typeof FixtureModeSchema>;
```

Every entity schema contains a stable `id`. `FactoryEventSchema` is a discriminated union over:

```ts
type FactoryEventType =
  | 'work.stage.changed'
  | 'agent.operation'
  | 'artifact.emitted'
  | 'gate.changed'
  | 'deployment.changed'
  | 'attention.raised'
  | 'runner.stale'
  | 'decision.requested'
  | 'decision.approved'
  | 'recovery.completed';
```

- [ ] **Step 4: Author the six-repository fixture**

Use the exact repository IDs from the design:

```text
repo-investor-app
repo-portfolio-api
repo-wallet-importer
repo-exchange-importer
repo-market-data-sync
repo-platform-infra
```

Distribute the 45 work items as 3 epics, 7 features, 11 stories, 14 tasks, 5 bugs and 5 chores. The scenario-critical chain must include:

```text
intent-sync-integrity
  -> work-sync-epic
  -> work-single-flight-feature
  -> work-cross-service-ordering-story
  -> pr-portfolio-api-418
  -> check-replay-suite
  -> deployment-portfolio-api-218
  -> evidence-reconciliation-breach
```

The remaining work covers app freshness, wallet providers, exchange imports, market-data reliability, CI runner hardening, observability, documentation, dependency maintenance and ready backlog. Give every active session one work item and one repository; do not invent human-like agent names.

- [ ] **Step 5: Author the scenario definition**

Use these ordered step IDs:

```text
calm
reconciliation-breach
contained
investigating
decision-required
forward-fix-approved
recovering
recovered
```

Include alternate fixture modes `healthy`, `attention-required`, `loading`, `streaming`, `stale`, `disconnected` and `empty`. Timestamps are fixed ISO-8601 UTC values beginning `2026-08-17T09:00:00Z`.

- [ ] **Step 6: Implement fixture loading and reference validation**

`loadFactoryFixture()` and `loadScenario()` parse imported JSON and throw a Zod error before React renders. `resolveBrokenReferences()` returns deterministic strings of the form:

```text
work-sync-epic.dependsOn -> missing-work-id
```

- [ ] **Step 7: Run fixture verification**

Run:

```bash
npm test -- src/benchmark/__tests__/fixture.test.ts
npm run lint
```

Expected: all tests pass and lint exits 0.

- [ ] **Step 8: Commit the fixture contract**

```bash
git add experiments/ux/command-attention/fixture experiments/ux/command-attention/src/benchmark experiments/ux/command-attention/README.md
git commit -m "Add synthetic factory benchmark fixture"
```

---

### Task 4: Implement deterministic scenario state and operator actions

**Files:**
- Create: `experiments/ux/command-attention/src/benchmark/actions.ts`
- Create: `experiments/ux/command-attention/src/benchmark/reducer.ts`
- Create: `experiments/ux/command-attention/src/benchmark/__tests__/reducer.test.ts`

**Interfaces:**
- Consumes: `FactoryFixture`, `ScenarioDefinition`, `FactoryEvent`.
- Produces: `BenchmarkState`, `BenchmarkAction`, `createInitialState()`, `reduceFactoryEvent()`, `reduceBenchmarkAction()` and `snapshotAtStep()`.

- [ ] **Step 1: Write reducer behavior as failing tests**

Cover at least:

```ts
expect(snapshotAtStep(fixture, scenario, 'calm').attentionItems).toEqual([]);
expect(snapshotAtStep(fixture, scenario, 'reconciliation-breach').deployments[0]?.state).toBe('halted');
expect(snapshotAtStep(fixture, scenario, 'contained').attentionItems[0]?.containmentStatus).toBe('complete');
expect(snapshotAtStep(fixture, scenario, 'decision-required').pendingDecision?.options).toHaveLength(2);
expect(snapshotAtStep(fixture, scenario, 'recovered').attentionItems[0]?.status).toBe('resolved');
```

Test that the same base fixture and step always produce deeply equal snapshots and that event application never mutates its inputs.

- [ ] **Step 2: Run the reducer tests to verify they fail**

Run:

```bash
npm test -- src/benchmark/__tests__/reducer.test.ts
```

Expected: failure because reducer exports do not exist.

- [ ] **Step 3: Define the operator action contract**

Use this discriminated union:

```ts
export type BenchmarkAction =
  | { type: 'attention.select'; attentionId: string }
  | { type: 'workstream.select'; workItemId: string }
  | { type: 'overlay.set'; overlay: 'work' | 'quality' | 'bottlenecks' | 'risk' }
  | { type: 'evidence.open'; evidenceId: string }
  | { type: 'recovery.choose'; optionId: 'rollback' | 'forward-fix' }
  | { type: 'recovery.confirm'; optionId: 'rollback' | 'forward-fix' }
  | { type: 'scenario.advance' }
  | { type: 'fixture-mode.set'; mode: FixtureMode };
```

State records the selected attention item, workstream, overlay, evidence and recovery option separately from the fixture snapshot.

- [ ] **Step 4: Implement pure event and action reducers**

Reject unknown entity IDs with messages that include the action type and missing ID. `recovery.confirm` advances only when its option matches the previously chosen option. Alternate fixture modes do not mutate scenario progress.

- [ ] **Step 5: Run the reducer and fixture tests**

Run:

```bash
npm test -- src/benchmark/__tests__/fixture.test.ts src/benchmark/__tests__/reducer.test.ts
```

Expected: all tests pass.

- [ ] **Step 6: Commit deterministic scenario behavior**

```bash
git add experiments/ux/command-attention/src/benchmark/actions.ts experiments/ux/command-attention/src/benchmark/reducer.ts experiments/ux/command-attention/src/benchmark/__tests__/reducer.test.ts
git commit -m "Add deterministic benchmark scenario"
```

---

### Task 5: Expose the candidate-neutral React provider and contract surface

**Files:**
- Create: `experiments/ux/command-attention/src/benchmark/BenchmarkProvider.tsx`
- Create: `experiments/ux/command-attention/src/benchmark/ContractTestSurface.tsx`
- Create: `experiments/ux/command-attention/src/benchmark/BenchmarkProvider.test.tsx`
- Modify: `experiments/ux/command-attention/src/App.tsx`
- Modify: `experiments/ux/command-attention/src/candidate/CandidateSurface.tsx`
- Modify: `experiments/ux/command-attention/README.md`

**Interfaces:**
- Consumes: the fixture loaders and reducers.
- Produces: `BenchmarkProvider`, `useBenchmark(): BenchmarkContextValue`, and stable `data-benchmark-action` / `data-fixture-id` contracts.

- [ ] **Step 1: Write provider interaction tests that fail**

Render a test consumer and assert:

```tsx
expect(screen.getByTestId('scenario-step')).toHaveTextContent('calm');
await user.click(screen.getByRole('button', { name: 'Advance scenario' }));
expect(screen.getByTestId('scenario-step')).toHaveTextContent('reconciliation-breach');
await user.click(screen.getByRole('button', { name: 'Use quality overlay' }));
expect(screen.getByTestId('overlay')).toHaveTextContent('quality');
```

Also test URL initialization from `?step=contained&mode=stale`.

- [ ] **Step 2: Run the provider test to verify it fails**

Run:

```bash
npm test -- src/benchmark/BenchmarkProvider.test.tsx
```

Expected: failure because `BenchmarkProvider` does not exist.

- [ ] **Step 3: Implement the provider API**

Export:

```ts
export interface BenchmarkContextValue {
  state: BenchmarkState;
  dispatch(action: BenchmarkAction): void;
  fixtureIds: ReadonlySet<string>;
}
```

Parse `step` and `mode` URL parameters once on initialization. Invalid values render an explicit benchmark configuration error instead of silently falling back.

- [ ] **Step 4: Implement the contract-only test surface**

`ContractTestSurface` is available only with `?surface=contract`. It renders unstyled semantic controls for every action and fixture ID so harness tests can verify behavior without supplying a visual direction. It must not be imported by `CandidateSurface`.

The contract surface exposes `data-testid="scenario-step"`, `data-testid="overlay"` and `data-testid="advance-scenario"` exactly as used by provider and Playwright tests.

All candidate implementations use:

```html
data-benchmark-action="open-incident"
data-benchmark-action="set-quality-overlay"
data-benchmark-action="open-evidence"
data-benchmark-action="choose-forward-fix"
data-benchmark-action="confirm-forward-fix"
data-fixture-id="<stable fixture entity ID>"
```

- [ ] **Step 5: Wire App without adding design bias**

`App` renders `BenchmarkProvider` and chooses `ContractTestSurface` only when requested; otherwise it renders `CandidateSurface`. The candidate placeholder may show current step and mode as plain text but must not introduce layout primitives.

- [ ] **Step 6: Run provider and full unit checks**

Run:

```bash
npm test -- src/benchmark/BenchmarkProvider.test.tsx
npm run check
```

Expected: all checks pass.

- [ ] **Step 7: Commit the provider contract**

```bash
git add experiments/ux/command-attention/src
git commit -m "Expose UX benchmark provider contract"
```

---

### Task 6: Add independent browser, accessibility and provenance verification

**Files:**
- Create: `experiments/ux/command-attention/playwright.config.ts`
- Create: `experiments/ux/command-attention/tests/e2e/helpers/accessibility.ts`
- Create: `experiments/ux/command-attention/tests/e2e/helpers/provenance.ts`
- Create: `experiments/ux/command-attention/tests/e2e/starter.spec.ts`
- Create: `experiments/ux/command-attention/tests/e2e/candidate-contract.spec.ts`
- Modify: `experiments/ux/command-attention/README.md`

**Interfaces:**
- Consumes: the provider action/data attributes and fixed fixture IDs.
- Produces: passing `verify:starter` checks and a candidate contract suite that every run must satisfy.

- [ ] **Step 1: Write a failing starter scenario test**

Use the contract surface and require:

```ts
test('@starter completes the controlled recovery path', async ({ page }) => {
  await page.goto('/?surface=contract&step=calm');
  await page.getByTestId('advance-scenario').click();
  await expect(page.getByTestId('scenario-step')).toHaveText('reconciliation-breach');
  await page.locator('[data-benchmark-action="open-incident"]').click();
  await page.locator('[data-benchmark-action="set-quality-overlay"]').click();
  await page.locator('[data-benchmark-action="open-evidence"]').click();
  await page.locator('[data-benchmark-action="choose-forward-fix"]').click();
  await page.locator('[data-benchmark-action="confirm-forward-fix"]').click();
  await expect(page.getByTestId('scenario-step')).toHaveText('recovering');
});
```

- [ ] **Step 2: Run the starter E2E test to verify it fails**

Run:

```bash
npm run test:e2e:starter
```

Expected: failure because Playwright configuration and test helpers are incomplete.

- [ ] **Step 3: Configure deterministic browser projects**

Use Chromium only, fixed locale `en-AU`, timezone `Australia/Sydney`, color scheme `dark`, device scale factor `1`, animations allowed by default, and the two exact viewports. Start Vite on `127.0.0.1:4173` with `npm run dev -- --host 127.0.0.1 --port 4173`.

- [ ] **Step 4: Add starter verification**

The `@starter` suite verifies:

- every scenario step and alternate mode loads;
- the contract surface completes the action path;
- unknown step/mode values show configuration errors;
- every `data-fixture-id` resolves to the fixture;
- no horizontal document overflow at either viewport;
- axe reports no critical or serious violations on the contract surface;
- the action path is keyboard-operable;
- reduced-motion media emulation leaves every state legible.

- [ ] **Step 5: Add candidate contract verification**

The `@candidate` suite targets the default surface and checks the same action attributes, fixture provenance, required states, viewports, axe results, keyboard path, overflow and reduced motion. It is not part of `verify:starter` because the neutral placeholder intentionally does not implement a design.

- [ ] **Step 6: Run starter verification**

Run:

```bash
npm run verify:starter
```

Expected: lint, unit tests, build and all `@starter` browser checks pass.

- [ ] **Step 7: Confirm the candidate suite detects the placeholder**

Run:

```bash
npm run test:e2e:candidate
```

Expected: failure on the missing `open-incident` action. Record this expected red result in `README.md`; do not weaken the test to make the placeholder pass.

- [ ] **Step 8: Commit browser verification**

```bash
git add experiments/ux/command-attention
git commit -m "Add UX benchmark browser verification"
```

---

### Task 7: Add sanitized run-record and capture tooling

**Files:**
- Create: `experiments/ux/command-attention/src/benchmark/runRecord.ts`
- Create: `experiments/ux/command-attention/src/benchmark/__tests__/runRecord.test.ts`
- Create: `experiments/ux/command-attention/scripts/create-run.ts`
- Create: `experiments/ux/command-attention/scripts/validate-run.ts`
- Create: `docs/research/ux/benchmark/runs/run-record.schema.json`
- Modify: `docs/research/ux/benchmark/capture-format.md`
- Modify: `experiments/ux/command-attention/README.md`

**Interfaces:**
- Consumes: benchmark version, context manifest hash, experiment commit, run metadata and capture paths.
- Produces: `RunRecord`, `createRunRecord()`, `validatePublishableRecord()`, `npm run run:create`, and `npm run run:validate`.

- [ ] **Step 1: Write failing run-record and privacy tests**

Test a valid minimal record and reject:

```ts
expect(() => validatePublishableRecord(recordWithWindowsHomePath)).toThrow(/local path/i);
expect(() => validatePublishableRecord(recordWithEmail)).toThrow(/email/i);
expect(() => validatePublishableRecord(recordWithPrivateKey)).toThrow(/credential/i);
expect(() => validatePublishableRecord(recordWithUnknownFixtureId)).toThrow(/fixture ID/i);
```

The validator must allow public package URLs and synthetic repository names.

- [ ] **Step 2: Run the run-record tests to verify they fail**

Run:

```bash
npm test -- src/benchmark/__tests__/runRecord.test.ts
```

Expected: failure because the run-record contract does not exist.

- [ ] **Step 3: Define the run-record schema**

The record requires:

```ts
interface RunRecord {
  runId: string;
  benchmarkVersion: 'ux-operator-slice-v1';
  asOf: '2026-08';
  candidate: { lane: string; name: string; version: string };
  harness: { name: 'Codex'; model: string; reasoningLevel: string; version: string };
  source: { benchmarkCommit: string; experimentCommit: string; contextManifestSha256: string };
  timing: { startedAt: string; finishedAt?: string; elapsedSeconds?: number };
  interaction: { clarificationCount: number; revisions: 2; neutralNudges: number; manualInterventions: string[] };
  usage: { tokens?: number; costUsd?: number; unavailableReason?: string };
  artifacts: { prompts: string[]; captures: string[]; dependencyManifest: string; licences: string };
  evidence: { operatorScore?: string; blindReviewScore?: string; deterministicReport?: string };
  privacyReview: { status: 'pending' | 'passed'; reviewedAt?: string; notes: string[] };
}
```

Export a JSON Schema representation to `runs/run-record.schema.json` from the same field definitions. Keep private raw-evidence paths outside `RunRecord`.

- [ ] **Step 4: Implement run creation and validation CLIs**

`npm run run:create -- --candidate baseline --lane control` creates a directory named `YYYYMMDD-HHMM-baseline` beneath `docs/research/ux/benchmark/runs/`, writes `run.json`, `notes.md`, `prompts.md`, `dependencies.md`, `licences.md`, and creates empty `captures/` and `scores/` directories.

`npm run run:validate -- <run-directory>` validates schema, relative artifact existence, fixture IDs and privacy patterns. It exits non-zero while privacy status is `pending` when invoked with `--publishable`.

- [ ] **Step 5: Run run-record tests and a disposable CLI smoke test**

Run:

```bash
npm test -- src/benchmark/__tests__/runRecord.test.ts
npm run run:create -- --candidate smoke --lane test --output docs/temp-context/ux-benchmark/smoke-run
npm run run:validate -- docs/temp-context/ux-benchmark/smoke-run
```

Expected: all commands exit 0; no smoke files appear in `git status`.

- [ ] **Step 6: Commit run-record tooling**

```bash
git add experiments/ux/command-attention docs/research/ux/benchmark/capture-format.md docs/research/ux/benchmark/runs/run-record.schema.json
git commit -m "Add UX benchmark run records"
```

---

### Task 8: Freeze and verify the supplied context

**Files:**
- Create: `experiments/ux/command-attention/scripts/freeze-context.ts`
- Create: `experiments/ux/command-attention/scripts/verify-benchmark.ts`
- Create: `experiments/ux/command-attention/scripts/freeze-context.test.ts`
- Create: `docs/research/ux/benchmark/context-manifest.json`
- Modify: `docs/research/ux/benchmark/README.md`

**Interfaces:**
- Consumes: exact context file paths, Git source commit and SHA-256.
- Produces: `ContextManifest`, `buildManifest()`, `verifyManifest()`, deterministic `context-manifest.json`, `npm run benchmark:freeze` and `npm run benchmark:verify`.

- [ ] **Step 1: Write failing manifest determinism tests**

Test that:

```ts
expect(buildManifest(filesInReverseOrder)).toEqual(buildManifest(filesInForwardOrder));
expect(manifest.files.map(file => file.path)).toEqual([...manifest.files.map(file => file.path)].sort());
expect(manifest.files.every(file => /^[a-f0-9]{64}$/.test(file.sha256))).toBe(true);
expect(verifyManifest(manifest)).toEqual([]);
```

Then change one temporary source byte and expect `verifyManifest()` to report its path and expected/actual hashes.

- [ ] **Step 2: Run the manifest test to verify it fails**

Run:

```bash
npm test -- scripts/freeze-context.test.ts
```

Expected: failure because manifest functions do not exist.

- [ ] **Step 3: Implement deterministic freezing**

The manifest contains:

```ts
interface ContextManifest {
  benchmarkVersion: 'ux-operator-slice-v1';
  asOf: '2026-08';
  sourceCommit: string;
  generatedAt: string;
  files: Array<{ path: string; sha256: string; bytes: number }>;
}
```

Hash this exact context list:

```text
docs/product/ux/factory-ux-vision.md
docs/product/ux/design-principles.md
docs/product/ux/open-questions.md
docs/product/decisions/0001-factory-ui-is-an-operator-console.md
docs/research/ux/ux-references.md
docs/research/ux/benchmark/benchmark-brief.md
docs/research/ux/benchmark/domain-glossary.md
docs/research/ux/benchmark/visualization-intent.md
docs/research/ux/benchmark/constraint-sheet.md
experiments/ux/command-attention/fixture/factory.json
experiments/ux/command-attention/fixture/scenario.json
```

Do not hash mutable indexes, run records or the benchmark README. Normalize manifest paths to forward slashes, but hash source bytes without rewriting them. Sort paths lexicographically before serialization and write a trailing LF.

- [ ] **Step 4: Implement benchmark verification**

`npm run benchmark:verify` checks:

- context hashes;
- fixture parse and reference integrity;
- required document existence;
- benchmark version consistency across design, brief, manifest and package README;
- no absolute local paths, emails, private keys or credential assignments in publishable benchmark files;
- no candidate run directories present before calibration;
- clean generated schema and manifest output.

- [ ] **Step 5: Run the unit and benchmark verification tests**

Run:

```bash
npm test -- scripts/freeze-context.test.ts
npm run benchmark:verify
```

Expected: unit tests pass. Benchmark verification fails only because `context-manifest.json` has not been generated.

- [ ] **Step 6: Commit the freeze tooling before generating the manifest**

```bash
git add experiments/ux/command-attention/scripts docs/research/ux/benchmark/README.md
git commit -m "Add UX benchmark context freezing"
```

- [ ] **Step 7: Generate the manifest from the committed source snapshot**

Run:

```bash
npm run benchmark:freeze -- --source-commit "$(git rev-parse HEAD)"
npm run benchmark:verify
```

Expected: `context-manifest.json` is created and verification exits 0.

- [ ] **Step 8: Commit the frozen manifest**

```bash
git add docs/research/ux/benchmark/context-manifest.json
git commit -m "Freeze UX benchmark context"
```

---

### Task 9: Add CI and prove clean-checkout reproducibility

**Files:**
- Create: `.github/workflows/ux-benchmark.yml`
- Modify: `docs/research/ux/benchmark/README.md`
- Modify: `todo.md`

**Interfaces:**
- Consumes: pinned npm lockfile and all starter verification commands.
- Produces: a path-scoped GitHub Actions gate and a reviewed frozen benchmark commit ready for a separate calibration execution plan.

- [ ] **Step 1: Add the path-scoped CI workflow**

Run CI for changes under `experiments/ux/command-attention/**`, `docs/research/ux/benchmark/**`, the workflow itself, or `todo.md`.

Pin actions by immutable commit with release comments:

```yaml
- uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
- uses: actions/setup-node@820762786026740c76f36085b0efc47a31fe5020 # v7.0.0
  with:
    node-version: 24.6.0
    cache: npm
    cache-dependency-path: experiments/ux/command-attention/package-lock.json
```

Then run:

```yaml
- working-directory: experiments/ux/command-attention
  run: npm ci
- working-directory: experiments/ux/command-attention
  run: npx playwright install --with-deps chromium
- working-directory: experiments/ux/command-attention
  run: npm run verify:starter
- working-directory: experiments/ux/command-attention
  run: npm run benchmark:verify
```

Upload `playwright-report/` and `test-results/` on failure with `actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1`.

- [ ] **Step 2: Run the complete local gate**

Run from the experiment directory:

```bash
npm ci
npx playwright install chromium
npm run verify:starter
npm run benchmark:verify
```

Expected: every command exits 0.

- [ ] **Step 3: Verify the candidate contract remains red against the placeholder**

Run:

```bash
npm run test:e2e:candidate
```

Expected: failure on the missing `open-incident` action. This proves the candidate gate is not accidentally satisfied by the neutral starter.

- [ ] **Step 4: Perform a clean-checkout rehearsal**

Create an isolated worktree from the current commit using `superpowers:using-git-worktrees`, run `npm ci`, install Chromium, run `verify:starter` and `benchmark:verify`, then remove the rehearsal worktree. Do not edit the primary checkout during this rehearsal.

Expected: the isolated checkout passes without access to untracked files or another workspace.

- [ ] **Step 5: Mark the benchmark-pack item complete**

Change only this line in `todo.md`:

```markdown
- [x] Prepare the frozen UX benchmark brief, realistic fixture, interaction script, capture format and scorecard.
```

Update the benchmark README with the source commit recorded by the context manifest, local verification commands, the expected-red candidate check and the statement that calibration has not started. The final readiness commit is reported at handoff rather than embedded into a file that would change its own commit ID.

- [ ] **Step 6: Run repository-level verification**

Run:

```bash
git diff --check
git status --short
```

Run the repository relative-link checker used for the initial documentation commit. Scan publishable files for absolute local paths, emails, credentials and private workspace identifiers. Expected: no failures; only Task 9 files are modified.

- [ ] **Step 7: Commit CI and benchmark readiness**

```bash
git add .github/workflows/ux-benchmark.yml docs/research/ux/benchmark/README.md todo.md
git commit -m "Verify frozen UX benchmark pack"
```

- [ ] **Step 8: Stop at the calibration review gate**

Present the frozen benchmark commit, manifest hash, local/CI verification, expected-red candidate contract, method pack and fixture summary to the user. Do not create the benchmark tag, install Trystan's workflow, generate directions or start Codex calibration sessions.

After user approval, write a separate `calibration-pair-execution-plan.md` pinned to the reviewed frozen commit.
