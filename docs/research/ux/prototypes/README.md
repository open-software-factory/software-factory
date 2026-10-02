# Operator console prototypes

Status: visual reference only

Date: captured 21 and 23 August 2026

These are screenshots of four working React prototypes of the operator console. Coding agents built them from the frozen UX vision, design principles and one incident scenario. Each prototype was a few thousand lines and passed the full deterministic gate set. The code is not kept. The screenshots are kept as reference for the visual direction and for the failure modes described in [`../agent-built-ui-lessons.md`](../agent-built-ui-lessons.md).

Every prototype shows the same three moments at 1536 by 960: `calm`, `decision-required` and `recovered`. The scenario is described in [`../../../architecture/draft-factory-vocabulary.md`](../../../architecture/draft-factory-vocabulary.md).

| Folder | Model | Direction | Design workflow |
|---|---|---|---|
| `deepseek-v4-flash-floor-as-territory/` | DeepSeek V4 Flash | A, "The Floor as Territory" | None |
| `deepseek-v4-flash-floor-first/` | DeepSeek V4 Flash | B, "Floor First" | Trystan-SA design system prompt |
| `gpt-5-6-luna-floor-as-territory/` | GPT-5.6 Luna, max reasoning | A, "The Floor as Territory" | None |
| `gpt-5-6-luna-floor-first/` | GPT-5.6 Luna, max reasoning | B, "Floor First" | Trystan-SA design system prompt |

## The two directions

**The Floor as Territory.** One continuous zoomable plane. Repositories are territories. The incident is a highlighted route across the floor. Attention and decision panels sit beside the floor.

**Floor First.** The factory floor is the whole surface. Repository zones are terrain. Overlays re-encode the same map for work, quality, bottlenecks and risk. The decision appears as a docked operations bar at the bottom.

## Read these with care

- The header controls "scenario step", "frame mode", "advance event" and "next step" are test machinery that leaked into the product. They are not a design idea.
- Labels such as "persistent spatial backbone" and "calm is an explicit state" are design doctrine printed as UI copy. They are a defect.
- The calm screens show failed checks and rolled-back deployments because the data was wrong. The design did not choose to show them.
- Raw entity IDs beside every label were required by the test contract. The design did not choose them.

What is worth studying: the density that stays readable, the decision cards with impact, confidence, reversibility and missing knowledge side by side, the affected-corridor strip in the Floor First direction, and the attention panel that keeps context while the floor stays visible.
