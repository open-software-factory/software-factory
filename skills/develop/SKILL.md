---
name: develop
description: Develop one provided work item into reviewable pull requests.
---

# Develop one provided work item

## Inputs

The caller provides the work item reference and the run id. If the work item reference is missing, stop and say `work item not provided`. If the run id is missing, stop and say `run id not provided`.

## Instructions

1. **Read the work item.** Read the work item from its reference in the work-tracker. Keep the work item title and body you read for the review and the report.
2. **Plan the work.** One pull request is the ideal when the work item fits. Split it into reviewable slices only when it is too large for one pull request. Read `[develop] max_files` and `max_changed_lines` in `osf.toml`. Split any slice that would exceed either.
3. **Create the branches.** For one pull request, create one branch. For a stack, follow the `opening-a-pull-request` skill's Git Town section. Use `git town hack` for the first branch and `git town append` for each next. Run `git town sync` when a change lands lower in the stack. If Git Town is unavailable, use plain git branches to build the same stack.
4. **Implement, commit and push each slice.** For each slice, implement the change, run the tests it needs, and commit. Follow the opening-a-pull-request skill for the commit trailer, worktree and push. Push each branch.
5. **Verify.** Run `osf verify --checkpoint pull-request --base <base>`. A failing check: fix it, or record why you cannot. A check that could not run is a failure to run, never a pass. Retry a known-retriable failure, such as a model or transient network error, up to three times. Record a deterministic failure and continue.
6. **Run the in-sandbox code-review.** Write the work item body you read to a temporary file, then run `osf review run --if-enabled --base <base> --work-item <that file>`. It is early feedback and is not posted. Fix every must-fix and should-fix finding, then run it again, up to `[review] in_sandbox_rounds`. A review that could not run does not consume a round. Record it. Retry a known-retriable failure up to three times.
7. **Assess the risk.** Run `osf changeset risk --base <base>`. Keep the report for the pull request body.
8. **Open the pull requests.** Follow the repository's `.github/PULL_REQUEST_TEMPLATE.md` and the `opening-a-pull-request` skill for the title, body and generated blocks. Each pull request links the work item, its parent pull request or base, its slice, and the verify, review and risk evidence. Open each slice with `osf forge pr open --head <branch> --base <parent> --title <title> --body <body> --work-item <item>`, in stack order. The first base is the repository's default branch. Each next base is its parent branch. If an adopter removed the template or skill, write the same fields directly.
9. **Report and complete.** Write the report: what changed, the checks and reviews, their evidence, and the open pull requests. Run `osf work-tracker report <item> --state in-review --recap <report file>`. Then run `osf run complete --run <run id> --outcome completed --summary "<one-line summary>"`. Stop at the open pull requests.
10. **When to stop.** Stop when you cannot continue: an unmet dependency, an ambiguous requirement, or a git/forge failure that retries cannot fix. Write the report and run `osf work-tracker report <item> --state blocked --cause <cause> --recap <report file>`, with a cause from `dependency`, `human`, `clarification`, `ambiguous` or `capacity`. Then run `osf run complete --run <run id> --outcome blocked --summary "<one-line summary>"` and stop. When the job cannot produce a change at all, report the work item `failed` and complete the run with `failed`.
