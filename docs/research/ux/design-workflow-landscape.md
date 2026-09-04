# AI-assisted product design workflow landscape

Status: research survey and proposed evaluation plan

Last reviewed: 2026-08-15

## Why this survey exists

The Software Factory needs a design workflow that can produce a distinctive, modern, information-dense operator interface without assuming that its primary operator is a trained product designer.

The survey looks for a workflow that helps a non-designer:

- express intent in ordinary language, voice, screenshots, and rough annotations;
- see materially different directions early;
- react to working, interactive designs rather than design jargon;
- point at what is wrong and ask for changes;
- preserve the factory's information hierarchy and interaction model;
- carry a chosen direction into maintainable code;
- verify accessibility, interaction, responsiveness, and visual regressions;
- replace any weak part of the workflow without replacing the whole stack.

The evidence below establishes which candidates deserve hands-on evaluation. Marketing pages and repository popularity do not establish visual quality.

## Hard inclusion gate: usable by a non-designer

A primary workflow fails the gate if it requires the operator to be fluent in professional canvas tooling, manually tune dozens of visual properties, construct components by hand, or know the vocabulary needed to repair a weak design.

A suitable workflow should do most of the following:

1. Ask a small number of high-value questions in plain language.
2. Turn a vague preference into two or three genuinely different visual and interaction directions.
3. Make the differences legible enough that the operator can choose by reaction rather than theory.
4. Support feedback such as "this is too card-heavy," "the intervention is not prominent enough," or an annotation drawn directly on the preview.
5. Preserve decisions as durable context rather than making the operator repeat them.
6. Inspect its own rendered result and iterate.
7. Escalate only the decisions that actually require human taste or judgment.

Professional tools may still be useful as optional handoff or inspection surfaces. They should not become mandatory manual steps.

## Candidate types

The candidates are not all substitutes for one another.

| Type | Job in the workflow | Examples |
|---|---|---|
| Design-direction skill | Gives a coding agent visual judgment and a repeatable critique vocabulary | Impeccable, Anthropic frontend-design, Hallmark |
| Design workflow | Structures discovery, directions, selection, refinement, and delivery | Impeccable, huashu-design, Trystan procedures |
| Agent-operated canvas | Lets an agent generate, branch, compare, and revise visual artifacts | Superdesign, Stitch, Pencil, Open Design |
| Prompt-to-product environment | Generates and visually edits a running interface with little specialist knowledge | Magic Patterns, v0, Figma Make, Lovable |
| Design-system grounding | Extracts or supplies tokens, components, and product-specific visual constraints | extract-design-system, DESIGN.md conventions, product registries |
| Deterministic reviewer | Checks properties that should not depend on model taste | axe-core, Playwright, Storybook, visual regression, lint rules |

The likely workflow will combine several of these types.

## Broad survey

### Portable skills and code-agent workflows

| Candidate | What it contributes | Fit for this factory | Disposition |
|---|---|---|---|
| [Anthropic frontend-design](https://github.com/anthropics/skills/blob/main/skills/frontend-design/SKILL.md) | Compact official skill for distinctive, context-grounded frontend work | Useful reference and lightweight control; limited workflow and verification on its own | Keep as reference/control |
| [Impeccable](https://github.com/pbakaus/impeccable) | Product-aware setup, shape/craft/critique/polish commands, live browser iteration, comp-first or code-first paths, and deterministic detectors | Strong fit for a product dashboard and for a non-designer who needs named refinement operations rather than manual styling | Shortlist |
| [UI UX Pro Max](https://github.com/nextlevelbuilder/ui-ux-pro-max-skill) | Large searchable catalogue of styles, palettes, typography, UX guidance, and stack-specific recommendations | Easy to invoke and explicitly supports dashboards; risk of recipe selection replacing product-specific design thinking | Reserve |
| [taste-skill](https://github.com/Leonxlnx/taste-skill) | Strong aesthetic controls and explicit variance, motion, and density dials | The main skill explicitly excludes dashboards, data tables, and multi-step product UI; wrong primary scope | Exclude as primary; retain selected references |
| [Hallmark](https://github.com/nutlope/hallmark) | Macrostructures, themes, a study mode, and anti-slop gates | Strong differentiation mechanism, but current examples and structure are weighted toward sites rather than dense operational products | Reserve |
| [huashu-design](https://github.com/alchaincyf/huashu-design) | Conversational workflow, three directions, early previews, real asset protocol, Playwright inspection, broad artifact generation | Very good non-designer interaction model; broad, large, and less specifically product-application oriented | Reserve; borrow direction-selection protocol |
| [ux-skill](https://github.com/Laith0003/ux-skill) | Deterministic synthesis, local decision ledger, manifests, and a CI linter | Technically interesting and aligned with deterministic authority, but very new and most comparative claims are self-reported | Research and QA experiment for now |
| [Vercel Web Interface Guidelines](https://github.com/vercel-labs/web-interface-guidelines) | Concise interaction, accessibility, form, typography, performance, and state-review guidance | Strong implementation review layer; does not produce a design direction | Integrate as review input |
| [extract-design-system](https://github.com/arvindrk/extract-design-system) | Repeatable extraction of starter tokens from a public reference site | Useful only for deliberate reference analysis; tokens are not a product design or permission to clone | Optional supporting tool |
| [Trystan-SA Claude Design System Prompt](https://github.com/Trystan-SA/claude-design-system-prompt) | One system prompt and a useful set of separate design/review procedures | Not authoritative or packaged as an Agent Skill, but directly worth testing, and one of the original candidates | Claude Design lane |
| [jiji262 claude-design](https://github.com/jiji262/claude-design-skill) | Installable consolidation with progressive references, direction selection, starter artifacts, and browser verification | Broad adaptation with assumptions to test, but a credible attempt to reproduce the useful Claude Design workflow outside its hosted product | Claude Design lane |
| [Garden `web-design-engineer`](https://github.com/ConardLi/garden-skills/blob/main/skills/web-design-engineer/SKILL.md) | Deep recipes, calibration, direction advising, redesign, browser acceptance, and failure-pattern material | The surrounding Garden repository is unrelated and this should not be described as a direct Trystan adaptation; the individual skill is nevertheless one of the original Claude Design-adjacent candidates | Claude Design lane |
| Nothing Design skill | A coherent, recognizable single visual language | Too style-prescriptive for a product whose own language is still being discovered | Inspiration only |
| Material 3 skill | Detailed components, tokens, responsive guidance, and compliance audit | Strong when Material 3 is the chosen system; its web path is explicitly limited and the factory should not inherit a house style by default | Exclude as primary |

### Agent-operated canvases and design environments

| Candidate | What it contributes | Fit for this factory | Disposition |
|---|---|---|---|
| [Superdesign](https://github.com/superdesigndev/superdesign-skill) | Agent-driven design-system setup, reference gathering, branchable drafts, and an infinite canvas accessible from coding agents | Excellent bridge between ordinary conversation and visual comparison; designed to work over an existing codebase | Shortlist |
| [Google Stitch and Stitch skills](https://github.com/google-labs-code/stitch-skills) | Prompt/image/voice-to-UI, variants, an AI-native canvas, MCP/SDK access, code export, and portable Agent Skills | Strong non-designer exploration and programmatic integration; experimental Google product and cloud dependency need testing | Shortlist |
| [Magic Patterns](https://www.magicpatterns.com/docs/documentation/get-started/introduction) | Product-team-oriented interactive prototypes, screenshot input, design-system grounding, team canvas, and engineering handoff | Explicitly supports product managers and starts from description rather than professional design operations | Shortlist |
| [Open Design](https://github.com/nexu-io/open-design) | Open-source, local-first studio using existing coding agents, composable skills, DESIGN.md, sandboxed previews, and real-file export | Closely matches agent-agnostic and local-first preferences; broad scope and visual quality still need empirical evaluation | Shortlist |
| [Pencil](https://docs.pencil.dev/getting-started/ai-integration) | Local MCP design operations, an agent skill, repository-resident design files, desktop/IDE/CLI surfaces, and headless export | Architecturally attractive and agent-operable; product source is currently private and canvas/code round-trip quality is unproven here | Close reserve |
| [Figma Make and Figma agent](https://www.figma.com/blog/the-figma-canvas-is-now-open-to-agents/) | Prompt-to-app prototypes, point-and-prompt editing, agent writes to the canvas, skills, MCP, and collaborative review | Much more approachable than manual Figma, but still carries platform and professional-tool gravity that may be unnecessary for a single non-designer operator | Close reserve |
| [Onlook](https://github.com/onlook-dev/onlook) | Open-source visual-first editor over React code with AI, branches, tokens, direct manipulation, and code mapping | Interesting code/design round trip; presently concentrated on Next.js and Tailwind and the current hosted direction is early access | Reserve |

### Prompt-to-app and model controls

| Candidate | What it contributes | Fit for this factory | Disposition |
|---|---|---|---|
| [v0](https://v0.dev/docs) | Natural language and voice generation, running previews, element selection, design mode, repo sync, and an API | Extremely approachable benchmark control; defaults and ecosystem bias may produce the exact generic dashboard language we need to escape | Benchmark control |
| [Lovable](https://docs.lovable.dev/features/preview-toolbar) | Point-at-elements, plain-language changes, direct text edits, drawing annotations, queued changes, and comments | Excellent non-designer interaction mechanics; broader app-building environment and React/Supabase conventions are not the design workflow itself | Interaction reference/control |
| [Figma Make](https://www.figma.com/solutions/ai-prototype-generator/) | Natural-language interactive prototypes with visual and code editing | Worth testing if the Figma agent path makes canvas expertise unnecessary; overlaps the Figma entry above | Close reserve |
| [Kimi K3](https://github.com/MoonshotAI/Kimi-K3), [Kimi Code](https://github.com/MoonshotAI/kimi-code), and [Kimi Websites](https://www.kimi.com/help/websites/websites-overview) | Native multimodality, visual-in-the-loop coding, long-horizon engineering, standard Agent Skills, and a Websites harness with preview annotation, versioning, and code export | Potentially a leading frontend implementation stack. Public documentation describes K3 as released, but the operator's API/subscription access is currently paused and waitlisted. The model, coding harness, and Websites design harness must be evaluated separately | Deferred until access |
| 21st.dev | Searchable community components and AI component generation | Useful implementation ingredients; component novelty does not solve factory information architecture | Supporting source only |
| Replit Agent, Bolt, and general app builders | Low-friction prompt-to-app generation | Useful market baselines but not obviously stronger design collaborators than the selected controls | Defer |

## Proposed evaluation field

The hands-on field contains three lanes. The lanes prevent us from treating prompt packs, agent-operated canvases, and prompt-to-app products as if they were identical, while every participant still receives the same brief and is judged on the same rendered outcome and operator effort.

### Lane A: Claude Design lineage and adjacent candidates

These three original candidates advance to hands-on evaluation:

1. Trystan-SA Claude Design System Prompt and its separate procedures.
2. jiji262 `claude-design-skill`.
3. Garden `web-design-engineer`.

The reason is empirical rather than genealogical: this workflow is in active use and reported as useful. These candidates attempt to preserve or recreate important parts of that experience, so excluding them based only on packaging or uncertain provenance would discard relevant evidence.

They remain separate participants. Combining them into one synthetic super-skill before evaluation would make it impossible to learn which workflow decisions are actually useful. Garden is grouped here as an original Claude Design-adjacent candidate, not asserted to be a direct adaptation of Trystan's repository.

### Lane B: broader workflow and canvas candidates

#### 1. Impeccable

Why it advances:

- distinguishes product interfaces from brand/marketing surfaces;
- converts design work into approachable verbs such as `shape`, `critique`, `bolder`, `quieter`, `distill`, and `polish`;
- supports both full-fidelity comp-first exploration and code-first work;
- persists product and design context;
- includes live browser iteration and deterministic checks;
- installs into `.agents/skills`, so it fits the factory's agent-neutral repository convention.

Main risk: an extensive command vocabulary can still burden the operator if the agent does not proactively choose the right operation.

#### 2. Superdesign

Why it advances:

- offers branchable visual drafts rather than forcing the user to critique code or prose;
- begins by understanding the current product and design system;
- can be driven from an existing coding agent in natural language;
- provides a visual canvas without requiring the user to construct the design manually.

Main risk: the current design engine is a hosted product behind a CLI/login even though the skill is open; portability, cost, export quality, and data handling require verification.

#### 3. Google Stitch plus the official Stitch skills

Why it advances:

- is explicitly designed for prompt, image, and voice-led exploration;
- supports multiple variants and an infinite-canvas design conversation;
- exposes MCP and SDK integration, rather than requiring a completely separate manual workflow;
- can export frontend artifacts and has official portable Agent Skills.

Main risk: it is experimental, cloud-dependent, and may optimize for isolated screens or websites rather than a coherent, dense desktop product.

#### 4. Magic Patterns

Why it advances:

- is aimed at product teams and explicitly includes product managers;
- produces interactive prototypes from descriptions and screenshots;
- supports real design systems, reusable flows, sharing, and engineering handoff;
- does not assume professional canvas technique as the entry point.

Main risk: it is a proprietary hosted surface. We need to verify code export, iteration fidelity, complex-state coverage, and whether it can remain a design tool rather than become a competing source repository.

#### 5. Open Design

Why it advances:

- is open source, local first, BYOK, and coding-agent agnostic;
- treats skills, DESIGN.md, previews, and exports as composable filesystem artifacts;
- includes dashboards and interactive artifacts in its intended scope;
- is architecturally aligned with the factory's desire to reuse capabilities through process/protocol boundaries.

Main risk: its scope is very broad. A large capability surface and catalogue do not prove that it will make better design decisions for this product.

### Lane C: controls

#### Control A: current coding agent plus the factory UX context pack

This is the essential baseline. If a candidate cannot materially outperform our own context plus a strong frontier agent, it does not justify extra machinery.

#### Control B: v0

v0 is the accessibility-of-use control: natural language, voice, running preview, element selection, and design mode. It also tests whether a mainstream prompt-to-app product can meet the brief once it receives unusually strong product context.

Two products are first alternates. Pencil is an open-source prototyping tool that keeps its files beside the code, and it enters if local, repository-resident design artifacts become a higher-priority evaluation dimension. Figma Make is a prompt-to-design feature inside Figma, and it enters if its agent workflow demonstrably removes the need for conventional Figma operation.

### Deferred Kimi K3 experiments

Kimi K3 should enter as soon as the operator's access becomes available. It should not be represented by one undifferentiated result. Run three experiments:

1. `Kimi Websites native`: K3 inside the Websites design harness, including preview annotation, version comparison, and code export.
2. `Kimi Code baseline`: K3 inside Kimi Code with the factory UX context but no additional design skill.
3. `Kimi Code plus portable skill`: K3 inside Kimi Code with the best-performing portable design workflow from the other lanes.

This separates model, harness and skill capability. Kimi Code supports project-level `.agents/skills`, while its documented built-in skills do not currently include a dedicated frontend-design workflow. Kimi Websites provides an explicit visual design and revision surface. The three experiments are meaningfully different.

## What to integrate rather than rebuild

Regardless of which creative workflow wins, the factory should compose existing deterministic and browser capabilities.

| Need | Reuse | Why |
|---|---|---|
| Browser interaction and rendered inspection | [Playwright](https://playwright.dev/) | Script real states, interactions, viewports, reduced motion, screenshots, and traces |
| Component state catalogue | [Storybook](https://storybook.js.org/) | Exercise healthy, busy, blocked, failed, stale, streaming, empty, and disconnected states in isolation |
| Automated accessibility | [axe-core](https://github.com/dequelabs/axe-core) through Storybook/Playwright | Deterministic first-line checks; incomplete cases remain explicit manual review |
| Visual regression | Playwright snapshots initially; Storybook/Chromatic if its service is justified | Detect pixel changes across known states instead of asking an LLM to remember appearance |
| Web interaction review | Vercel Web Interface Guidelines | Broad implementation checklist without inventing our own generic web rules |
| Design context | A small project-owned design context and token set, likely using compatible `DESIGN.md` ideas | Preserve decisions independent of any one provider |
| Reference token extraction | extract-design-system, only when deliberately studying a public reference | Avoid reimplementing a fragile browser/token extraction tool |
| Agent skills packaging | Standard `.agents/skills/<skill>/SKILL.md` convention | Keep factory skills usable across coding-agent harnesses |

Agent-generated aesthetic judgments remain advisory. Contrast calculations, focus behavior, overflow, keyboard operation, interaction outcomes, reduced motion, and visual diffs should be checked by deterministic tools wherever feasible.

## Benchmark protocol

### Round one: Command / Attention surface

This is a stronger first test than a landing page. It exposes whether a workflow understands information hierarchy, calm autonomy, dense operational data, exceptions, and direct paths to evidence.

Each candidate receives the same frozen brief:

- the current factory UX vision and design principles;
- a concise product/domain glossary;
- a realistic desktop viewport and technology-neutral constraint sheet;
- the same fixture containing healthy operation, one urgent deployment failure, one agent ambiguity, runway pressure, active work, throughput, cost anomaly, and stale connectivity;
- a requirement to make the surface interactive rather than return a static screenshot;
- no reference screenshot that would collapse the exploration into imitation.

Each candidate must produce:

1. Three materially different directions. A theme swap does not count.
2. A short explanation in ordinary language of what each direction optimizes.
3. One chosen direction as an interactive prototype.
4. Healthy, attention-required, loading/streaming, stale/disconnected, and empty states.
5. One natural-language revision after the operator reacts to the prototype.
6. A second revision based on a visual annotation or element selection where the tool supports it.
7. Exportable artifacts and a record of prompts, context, elapsed time, manual steps, and usage cost.

The operator should not be required to name typography scales, grid systems, radii, easing curves, or component patterns. A candidate loses points when it makes expert manual intervention necessary.

### Round-two finalists: Factory Floor and Flight Recorder

The top three advance to a harder test with two connected surfaces:

- a live factory-floor topology with real-event-driven movement, overlays, semantic zoom, and a selected work item;
- a work-item Flight Recorder that moves from semantic history to exact evidence without becoming a wall of chat or tables.

This tests spatial reasoning, temporal reasoning, meaningful motion, cross-surface continuity, and dense evidence drill-down. It also reveals whether the first-round visual language survives beyond one attractive screen.

### Evaluation method

Review should combine blind outcome scoring with workflow observation.

The rendered directions should initially be shown without candidate names. Separately record how much effort it took to get there. A result that requires a trained designer to repair it fails the non-designer gate even if it looks strong.

| Criterion | Weight | What good looks like |
|---|---:|---|
| Operator comprehension and hierarchy | 20 | The most important intervention is obvious; healthy work remains calm; density is navigable |
| Product-specific design judgment | 15 | Looks and behaves like this factory, with nothing generic about it |
| Non-designer steering | 15 | Ordinary feedback, pointing, or annotation produces controlled improvement |
| Interaction and state completeness | 10 | Real states and transitions are designed alongside the ideal screenshot |
| Visual distinction and craft | 10 | Beautiful, coherent, memorable, restrained, and free of replacement tropes |
| Evidence and drill-down model | 10 | Overview leads to explanation and raw evidence without losing context |
| Iteration quality | 8 | Revisions preserve what works and change what was requested without visual drift |
| Code/artifact portability | 5 | Result can inform or enter the actual React/Tauri codebase without platform lock-in |
| Accessibility and deterministic verification | 4 | Candidate cooperates with browser, keyboard, contrast, reduced-motion, and a11y checks |
| Time, cost, and workflow friction | 3 | Low setup and low manual repair burden; context can be reused |

Any candidate that fails the non-designer gate is removed regardless of total score.

## Change in evaluation scope

The earlier plan compared adaptations of one reverse-engineered prompt. The survey widened the field to workflows with:

- visual branching and comparison;
- direct manipulation or point-and-prompt feedback;
- codebase and design-system grounding;
- persistent design decisions;
- agent-accessible canvases through CLI, SDK, or MCP;
- deterministic browser and accessibility feedback.

Trystan's procedures remain useful source material, particularly for review passes. They are not the standard that other candidates need to resemble.

## Open research before the benchmark

1. Confirm pricing, quotas, data retention, and commercial terms for Superdesign, Stitch, Magic Patterns, and v0.
2. Confirm which tools can export an artifact we can version locally and whether later iterations round-trip without destructive regeneration.
3. Define the fixed realistic data fixture and interaction script for the Command / Attention surface.
4. Decide which coding model/harness is held constant for portable skill candidates. Add the three Kimi K3 experiments when the operator's paused/waitlisted access becomes available.
5. Decide whether proprietary hosted tools may receive the full repository context or only a sanitized design brief and fixture.
6. Inspect the install payloads, permissions, scripts, hooks, and network behavior before installing any candidate.
7. Decide whether visual outcomes can be evaluated locally at the same viewport/font/browser conditions.
8. Treat the deterministic anti-slop linters as hypotheses: inspect their rules for false authority, context blindness, and accidental enforcement of a different house style.

## Recommended next step

Do not install all candidates into the repository. Implement the approved [`UX workflow benchmark design`](benchmark/benchmark-design.md), then run the Codex baseline and Trystan-SA calibration pair. Correct the benchmark only if calibration exposes a methodological defect; a well-supported tie is a valid result. The remaining participants can then run in batches, with Kimi K3 added when access permits.
