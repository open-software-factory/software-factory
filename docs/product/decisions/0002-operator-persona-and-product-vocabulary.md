# 0002: The operator, and the words the product uses

Status: accepted

Date: 2026-09-20

## Context

In August 2026 four agent-built consoles printed builder doctrine as UI labels. The agents had the vision and the design principles and had no persona and no vocabulary, so they filled the gap with the doctrine. Every surface built since has invented its own words for the same things. This record fixes both before the next surface is built.

## Decision

### The operator

The operator is an engineer who runs the factory and owns what it produces. At first that is one developer running the factory over their own repositories. Later it is a senior engineer or a lead accountable for a handful of repositories. The words are the same at both scales.

Their role has moved from writing every change to operating and managing the agents that write them. The value the product gives them is observability of the whole software lifecycle: rich live views, alerts, and history, so they can see and steer everything the factory does.

They look at the console whenever they choose: a few times a day, or after days away. They are never expected to watch it continuously, and a design that needs them to is wrong. On every arrival the product gives them enough context to act: what needs them, what happened since they last looked, and what the factory is doing now.

They can read code and diffs and should rarely need to. Machine identifiers, hashes and session identifiers sit behind a details view. They have the authority to pause and resume work, to approve or decline a decision, to override a soft cap, to retry a run and to take a work item over.

### The words

Each product word maps to one term of the domain model in architecture decision 0005. Product copy uses the left column. Code and records use the right.

| Product word | Domain term | Notes |
|---|---|---|
| Work item | Work item | The issue in the tracker, shown with the tracker's number. |
| Run | Run | One attempt by one agent, or one person, on a work item. |
| Check | Verifier run | Shown by the tool's name, with passed, failed, or could not run. |
| Review | Review run | Shown with its round number. |
| Finding | Finding | Shown with its severity and where it is. |
| Measured, computed, claimed, unverified | Observed, derived, reported, unverified | The evidence grades, in plain words. |
| Gate, held | Policy hard limit, hold | Where a policy stopped the work, and the state it left. |
| Blocked, with its cause | Blocked: dependency, human, clarification, ambiguous, capacity | Copy says the cause: "blocked, waiting for you", "blocked on a dependency". |
| Needs you | Attention | The list of items that need a person. |
| Since you last looked | Recap | The arrival summary on every surface. |
| Agent, by its product name | Actor: harness, model, model family | Shown under the run. The product has no roster of agents. |

The six surfaces are named in the product as Attention, Floor, Work, Recorder, Runway and Insights. The navigation uses the same six words.

### Words that stay off the screen

Builder doctrine is an instruction to the builder and never a label for the operator. These phrases, and the documents they come from, do not appear in operator copy or accessible names: persistent spatial backbone, calm is an explicit state, every record inspectable, same truth multiple projections, overview explanation evidence, human attention is the scarce resource, show work before agents, design for calm autonomy. The builder's nouns stay off too: verifier, reporter, policy, projection, event stream, snapshot, catalog, fixture, scenario, harness.

Test and evaluator controls never appear in a product surface. A source-level check keeps product code from importing the test interface.

## Consequences

- The deterministic copy scan the August lessons asked for gets its word list from this record.
- The status block, the command-line output and the integrations are checked against the table above, and each divergence is a work item.
- A surface built by an agent receives this record beside the design principles, and the review of that surface checks the copy against it.
- A later shift-operator persona, staffed to watch continuously, is out of scope and would need its own record.
