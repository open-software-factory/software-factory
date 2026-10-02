# Design explorations archive

Everything explored and not chosen lives here, so we can refer back without cluttering `DESIGN.md`. Nothing in this file is guidance. Do not build from it.

## Rules for future explorations

The owner set these rules on 2 September 2026.

1. Keep a copy of every discarded direction in this file at the moment it is discarded.
2. Explore **one dimension at a time**. Try layout, then typography, then colour, and so on. The Stage 1 round tried colour, theme, brand, look-and-feel, typography, layout, and components at once, with no structure. That is a large part of why it did not work.

## Tab treatments

The team compared three tab treatments live in the prototype and picked chip on 10 September 2026. Chip is a rounded fill on the active tab, with no line.

| Discarded | Why |
|---|---|
| **Underline**, text only, a 2px accent bar under the active tab | Cleanest of the three, but it is a *line*, and this shell spent several rounds removing lines in favour of tone. It reintroduced the thing we had just taken out. |
| **Lifted**, active tab shares the body surface and merges into it, no seam | The most "designed" of the three and the previous default. It depends on the tab strip and pane body sharing a background, which constrains what a pane body can be, and it reads fussy beside a chip-shaped rail selection. |

Both are removed from the prototype along with the switcher. Recorded here rather than kept as an option: keeping three implementations alive to defer one decision is how a prototype turns into a settings screen.

## Layout presets A/B/C/D

The team retired these on 10 September 2026. Four named starting layouts had existed as a lab switcher: nav expanded, rail plus attention, compact plus tabbed right, and focus. They were never four products. They were four moments in one session. Each layout is now reachable through the product's own controls. The rail has an expand button, and a nav switch exists. A pane has a collapse-to-rail control, and the top bar has a real focus mode. The switcher was doing the work the UI should have been doing.

## Shell structural directions

Stage 1 explored these directions in August 2026. Three structurally different shells were considered. The owner chose B, Live Floor, the canvas-centred shell described in the table below, as the starting shell on 2 September 2026. The choice is a starting point, and the team can revisit it. See `DESIGN.md` section 3.1. See tracker item F1 as well.

| | A, Command Post | B, Live Floor | C, Focus Console |
|---|---|---|---|
| Home | Attention queue | Factory Floor canvas | Pulse view as context selector |
| Primary nav | 5 destinations + focus inspector | 3 modes (Floor, Stand, Lab) | Focus threads × projections × inspector |
| Graphs | Destination below Graphs | Canvas lenses | Projection of a thread |
| Attention | The home | Docked layer + canvas marks | Compact stream feeding threads |
| Evidence path | Summary, then why, then evidence, in the item | Item, then docked recorder, then evidence | Thread, then recorder and evidence panes |
| Time | Global scope control | Global scrubber on canvas | Per-thread scrubbing |
| Risk | "Five apps" failure mode | Calm floor reads as empty | Thread management overhead |

The archived decision rule works like this. If the operating profile shows triage-burst dominance with rare spatial needs, flip to A. The floor then becomes the contextual "where". The canvas and attention grammar stay the same either way. The owner set a new direction on 2 September 2026. This direction replaces the binary flip. The shell stays B. It must still support **contextual focus** on the attention queue and other surfaces, as needed.

## Design-system coverage audit

The team ran this one-off audit on 2 September 2026. It was archived here on 11 September 2026, and is kept for reference. Its adoptions went into `DESIGN.md`, the shell's design specification. Its parked dimensions are listed at the end of this section. Nothing here is a plan.

On 2 September 2026, the purpose was to check `DESIGN.md` against the dimensions that mature design systems cover, before the component and framework survey. It also checked `DESIGN.md` against 2026 agent-UX guidance. The team adopted opinions where it held none. It parked dimensions on purpose.

Reference set: Material 3, Apple HIG, Fluent 2, IBM Carbon (incl. Carbon for AI), Shopify Polaris, Adobe Spectrum, Atlassian, GOV.UK. Agent-UX: AG-UI / A2UI protocol work, and current agentic-interface pattern guidance. This guidance covers confidence states, review flows, audit trails, and human override.

Verdicts work like this. `covered` means we hold an opinion, even if exact values stay open. `partial` means the dimension exists but has holes. `none` means we hold no opinion yet.

### Classic dimensions

| Dimension | Codified by | Us today | Verdict |
|---|---|---|---|
| Principles & product character | all | `DESIGN.md` §1 | covered |
| Information hierarchy & density | Carbon, Fluent | §2 + component modes | covered, stronger than most |
| Colour tokens & theming | all | §13; exact values open (Q9, Q11) | covered |
| Typography | all | §12; final family open (Q8) | covered |
| Spacing, radii, elevation, surfaces | all | §14 | covered |
| Iconography | all | §15 | covered |
| Layout, grid, responsive | all | §4 + platform contracts | covered |
| Motion | Material, HIG | §16.5-16.6, §5.1 | covered |
| Interaction states, focus, keyboard | all | §16, §5.4 | covered |
| Accessibility | all | §17 global rules; no per-component a11y specs | partial. Adopted Level-1 primitives carry component a11y. The team verifies it rather than authoring it. |
| **Content**: voice & tone, terminology, error-message writing | Polaris, Atlassian, Material | nothing | **drafted, `DESIGN.md` §6 [C]** |
| **Feedback taxonomy**: toast vs banner vs badge vs inbox; loading, progress, skeletons | Material, Carbon | toast + banner exist with no usage rules; no loading/skeleton opinion | **drafted, `DESIGN.md` §7 [C]** |
| **Forms & validation**: error timing/placement, destructive confirmation, undo | all | only open question Q2 (intervention verbs) | **drafted, `DESIGN.md` §8 [C]** |
| **Data visualization**: chart types, encodings, chart colour roles | Carbon, Spectrum | §10 covers the canvas. It does not cover charts. Runway needs charts. | **none**, adopt (lite) before Runway work |
| **Time & number display**: absolute vs relative time, precision, timezones | GOV.UK, Carbon | timestamps appear everywhere in fixtures, no standard | **drafted, `DESIGN.md` §9 [C]** |
| Empty / error / edge states | Polaris, Spectrum | EmptyCalmState, StaleIndicator, six-state exercise rule (header working conventions) | covered |
| **Gestures & touch vocabulary**: pinch, swipe, long-press | HIG, Material | targets sized (§5.4.3); gestures not codified | partial, adopt with MotionViewport work |
| Internationalisation / RTL / localisation | all | nothing | none, **park explicitly**: English-only v1; revisit before productisation |
| Sound & haptics | HIG | open question Q4 | none, **park explicitly** (stays Q4) |
| Imagery & illustration | Material, Polaris | nothing | none, **decide minimal**: data-first console, no illustration language |

### Agent-UX dimensions (2026 guidance)

| Dimension | Us today | Verdict |
|---|---|---|
| Agent activity transparency, "what is it doing now" | AgentPin, RecorderTimeline, §11.7 | covered |
| Provenance, evidence, confidence display | §11.8, ProvenanceChip, ConfidenceTag | covered, ahead of the reference set |
| Review / approval flows | ReviewPanel, gate semantics §11.5 | covered |
| Audit trail & replay | Recorder; replay semantics open (Q6) | partial |
| **Override, interrupt, undo, error recovery** | P0 interruption exists; verbs, confirmation model and undo undecided (Q2) | **drafted, `DESIGN.md` §8 [C]** |
| Generated-UI governance, what agents may compose | Level-2 vocabulary + modes + StreamSurface contract | covered, ahead of the reference set |
| Conversational / ambient entry | AmbientAssistant, voice-first inputs | covered |
| Regulation-driven transparency (AI-act-class disclosure) | not considered | none, park; note for productisation |

### Recommended adoption order

The adoption order applies before the component survey. On 2 September 2026, the team drafted several items into `DESIGN.md` and reviewed them. These were items 1-4 in the list below. They are now `DESIGN.md` sections 6-9, after a later reorg. Item 5, dataviz and gesture vocabulary, remains open.

1. **Voice and tone**, terminology, and the error-message standard have the highest priority. Operators read this UI under stress, and agents generate text into it. A written voice standard is also a constraint the team hands to LLM output.
2. **Feedback taxonomy.** This is one table. It maps each channel, toast, banner, badge, attention item, or inbox, to a severity and a lifetime. It also gives a loading and skeleton opinion. Progressive loading becomes a first-class state in streaming UIs, under rule 5.6.
3. **Confirmation, undo, error recovery.** This resolves open question Q2 and the agent-override gap in one pass.
4. **Time & number display standard.** It is small, mechanical, and immediately usable in fixtures.
5. **Dataviz (lite)** and **gesture vocabulary** can trail. Dataviz is needed before the Runway work. Gesture vocabulary is needed before the MotionViewport work, the pan-and-zoom canvas container.

Items 1-4 change what the component survey scores, for example toast and notification libraries, and form libraries. Adopt them first, then run the survey.
