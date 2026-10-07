---
title: Model reviews checked by a deterministic reducer
date: 2026-09-25
kind: research
status: draft
---

# Model reviews checked by a deterministic reducer

Research note, compiled 2026-09-25, from a review of one public coding-agent project. The project uses a language model as a reviewer, then checks the review with a plain function. This is useful prior art. The factory already has two related records. One is a note on risk-tiered review depth, at docs/research/2026-09-17-change-risk-classification.md. The other is a decision that a deterministic result outranks a model's opinion, at docs/architecture/decisions/0003-deterministic-verification-is-authoritative.md. This note compares the project against both.

## 1. The project studied

The project is bastani-inc/atomic, described by its own maintainers as "the verifiable coding agent runtime" (https://github.com/bastani-inc/atomic). It was found by searching GitHub for a project whose own code uses the words verifier, reducer and qlty together. Commit read: `1e1c5e4cc4de3596514d1952efd608da74c807c0`, on branch main, dated 2026-09-23.

Atomic is a general coding-agent command-line tool and workflow engine. It is not a dedicated review bot. Verifiers, a reducer, and qlty are one feature inside that larger tool. The rest of this note looks only at that one feature.

## 2. A verifier, in this project, is a language-model judgment call

Two workflows use the word verifier or reviewer.

The `adversarial-verification` workflow asks the model to score a change against one written criterion, on a scale of 1 to 20, and to list findings. Each finding carries a severity: veto, blocking, or note. The model's answer must match a fixed schema. A second, hand-written check confirms the schema at run time before the score is trusted. An answer that fails this check is dropped before it can be scored. Source: `packages/workflows/builtin/adversarial-verification-runner.ts`, lines 41 to 49 and 126 to 138.

How many independent scores run per criterion is a plain setting, `verifier_count`, from 1 to 5, default 3. The factory's change-risk note already names this idea. It calls it a review axis, in the sentence "how many review axes run and how deep each one reads" (docs/research/2026-09-17-change-risk-classification.md, section 1). This note calls the same idea a review lens instead, to keep it separate from the deterministic checks decision 0003 calls a Verifier. One review lens is one named way of judging a change, run some configurable number of independent times.

The "goal" workflow's reviewer stage works the same way, with a different schema: a `stop_review_loop` flag, a correctness field, and an optional error field. An answer the code cannot parse is not dropped. It is replaced with a synthetic decision whose `stop_review_loop` is forced to false. A reviewer that crashed and a reviewer that read the change and said "not done yet" look the same to the next step. Source: `packages/workflows/builtin/goal-review.ts`, lines 48 to 68.

There is no separate idea, in this project, of a deterministic command wrapped as a scored input. A test run or a lint run is read by the model as plain text, then paraphrased into a finding by hand. It does not produce its own schema-checked record.

## 3. The reducer is a plain function

Two reducers exist. Both are ordinary functions with unit tests. Neither calls a model.

`decide_verification()` takes the valid scores from one round. If too few scores came back to meet a quorum, the round is `indeterminate`. Otherwise it takes the mean of the scores. Any single veto finding forces a `repair` outcome, regardless of the mean. Source: `packages/workflows/builtin/verification-criteria.ts`, lines 315 to 330.

A verifier that never returned a valid score is tracked, but its count cannot enter the mean. The project's own code comment on this line calls the count "intentionally unread." A judge that could not run can only push the round toward `indeterminate`, by starving the quorum. It can never manufacture a pass or a fail by itself. Source: same file, line 316.

The quorum for this workflow is fixed at 100 percent. Every review lens must return a valid score before any round can accept. Zero valid scores gives `indeterminate`, and two indeterminate rounds in a row end the loop without ever accepting. Source: `packages/workflows/builtin/adversarial-verification-runner.ts`, line 39 and line 349.

`reduceGoalDecision()` and `reviewApproved()` do the same job for the goal workflow's reviewers. They count votes for `complete`, and accept once a quorum of reviewers agree, default 2 of 3. An execution failure can never approve. A parse failure is turned into an execution failure upstream, so it can never approve either. Source: `packages/workflows/builtin/goal-review.ts`, lines 40 to 46.

## 4. Comparison with decision 0003

Decision 0003 names five terms: Verifier, Reporter, Policy, Gate, Checkpoint. A Verifier "runs one check and emits evidence. It knows nothing about consequences. Any command can be a verifier." A Policy "reads evidence and decides." A Gate is "the moment a policy hard limit stops the work." Source: docs/architecture/decisions/0003-deterministic-verification-is-authoritative.md.

| This project's part | Decision 0003's closest term | Same or different | How |
|---|---|---|---|
| A verifier's model call | Verifier | Different mechanism | Decision 0003's Verifier is a deterministic command. This project's verifier is a language-model judgment, constrained by a schema. Same word, opposite mechanism. |
| A criterion, judged by several independent verifier calls | Review lens (this note's term; the factory's change-risk note calls it a review axis) | Same idea | Both are one named way of judging a change, run some configurable number of times. |
| `decide_verification()`, `reduceGoalDecision()` | Policy | Same job, different inputs | Both are pure functions that turn several individual results into one verdict. This project's policy combines several model opinions from one run. Decision 0003's Policy is written to combine deterministic command results. |
| A `repair` or `needs_human` outcome | Gate | Same idea, narrower | Both stop work from advancing. This project's gate stops one workflow round. Decision 0003's Gate can stop a merge. |
| The rule that `invalidCount` cannot enter the mean, plus the 100 percent quorum floor | Decision 0003's rule: "a read distinguishes could not read from read and found nothing" | Same rule, narrower scope | This project applies the rule only to model-judgment quorum. Decision 0003 applies it to any evidence a policy reads. |
| The 100 percent quorum floor before an accept is possible | Decision 0003's rule: "green must be earned" | Same rule, narrower scope | Both refuse to call a run clean when nothing usable ran. This project checks it for one workflow's judges. Decision 0003 checks it for test counts, suppressions, and declared commands too. |
| PR body text and file attachments written by the model | Reporter | Weaker match | Decision 0003's Reporter renders evidence onto a defined surface. This project's reporting is free-form prose the model writes, with no fixed shape. |
| The project's flagship pattern: model judgment is the gate, deterministic tools are optional evidence the model may cite | Decision 0003's central rule: a deterministic result is authoritative, and a model may not turn a failing gate into a passing one | Opposite stance | This project's review pattern makes the model the gate. Decision 0003 makes the model advisory, underneath a deterministic gate. |

## 5. How the project uses qlty

qlty appears as one of roughly a dozen tools the agent may choose to run. It is listed in a skill file, separate from the schema-checked verifier record. A finding from qlty can become a line of text inside a verifier's evidence field, typed by the model after reading qlty's console output. There is no code path that turns qlty's own JSON or SARIF output into a scored record, confirmed by a repository-wide search for the word "sarif" that found no consumer. A full look at qlty itself is in docs/research/2026-09-25-qlty-as-a-check-runner.md. Source: `packages/subagents/skills/qlty/SKILL.md`.

## 6. Reaching a repository and posting results

The project has no GitHub App, bot process, or bundled GitHub Action. It is a terminal tool. Any GitHub activity, such as opening a pull request or leaving a comment, happens because the agent calls the `gh` command-line tool itself. This is the same way a person would use `gh`. It posts Markdown text and file attachments, from `packages/coding-agent/docs/workflows/verification.md`. It does not open a required check, and it does not upload SARIF.

## 7. Licence

The project's licence is MIT, with one added clause. Its `LICENSE` file adds a rule: any product past 100 million monthly active users, or $20 million in monthly revenue, must show "Atomic" on its user interface. The exact wording is: "If your product or service exceeds 100 million monthly active users or $20 million in monthly revenue, you must prominently display 'Atomic' on the user interface of such product or service." GitHub's own licence detector reports this as `NOASSERTION`. The added clause makes it not a plain MIT licence. This holds even though the project's own README badge says "License: MIT."

## 8. Maturity, read 2026-09-24

| Metric | Value |
|---|---|
| Stars | 821 |
| Forks | 113 |
| Open issues | 35 |
| Created | 2025-10-23 |
| Last push | 2026-09-24 |
| Commits in the last 90 days | 462 |
| Releases | 462, from v0.1.0 (2026-01-21) to 0.9.20-alpha.8 (2026-09-23) |

Source: `gh repo view bastani-inc/atomic` and `gh api repos/bastani-inc/atomic/releases --paginate`, both read 2026-09-24.

About 11 months old, pre-1.0, releasing several times a week. A real but moderate community, 821 stars and 113 forks.

## 9. What this project has, worth reusing

- A tested pattern for combining several independent model opinions. It takes the mean, then applies a veto override. A judge that could not run can never manufacture a pass or a fail. This is more developed than anything the factory's own records currently describe, for combining several model reviewers on one change.
- A bounded retry on a schema failure, one re-ask per judge before giving up, as a tested, named setting (`reask_limit`).

## 10. Where the factory's own record already goes further

Decision 0003 already settles a question this project leaves open. A model may not turn a failing deterministic gate into a passing one. This project's flagship review pattern does the opposite. It makes the model's own judgment the gate. It treats a deterministic tool like qlty as optional evidence the model may or may not cite. The factory's rule is the stronger position for an unattended pipeline. It does not depend on the model choosing to run or trust a deterministic check.
