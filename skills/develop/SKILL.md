---
name: develop
description: Use when a caller provides one work item reference and one run id for a develop job in a supplied sandbox. Take the item to stacked pull requests and stop at open pull requests.
---

You are the builder for one develop job. The caller has started the sandbox and the journal run and has given you:

- the work item reference, in the tracker's form.
- the run id.

If the work item reference is missing, stop and say `work item not provided`. If the run id is missing, stop and say `run id not provided`. Do not pick a work item, do not start a run, and do not start a sandbox.

1. **Read the work item and the repository.** Read the item and the repository rules. The caller may provide the item body. Otherwise, read it from the reference with the tracker's code host. Keep the item title and body for the review input and the recap.
2. **Plan the stack.** Before changing code, decide the reviewable logical slices of the item. Each slice becomes one pull request. Order the slices. Read `[develop] max_files` (default 15) and `max_changed_lines` (default 700) in `osf.toml`. Split any slice that would exceed either limit.
3. **Create the branch stack.** Follow the `opening-a-pull-request` skill's Git Town section. `git town hack` for the first branch, `git town append` for each next, and `git town sync` when a change lands lower in the stack. If Git Town is unavailable, use plain git branches to build the same stack. A missing Git Town is not a reason to stop.
4. **Implement, commit and push each slice.** Work only inside the supplied sandbox and repository. For each slice, implement the change, run the tests it needs, and commit. Follow the opening-a-pull-request skill for the commit trailer, worktree and push. Push each branch. Do not merge, and do not force-push a branch you did not create.
5. **Verify.** Run `osf verify --checkpoint pull-request --base <base>`. A failing check is not a hard stop: fix it, or record why you cannot. A check that could not run is a failure to run, never a pass. Retry a known-retriable failure, such as a model or transient network error, up to three times. Record a deterministic failure and continue.
6. **Review in the sandbox.** Run `osf review run --if-enabled --base <base> --work-item <work item file>`. It is early feedback and is not posted. Fix every must-fix and should-fix finding, then run it again, up to the configured round limit (three by default). A review that could not run does not consume a round. Record it. Retry a known-retriable failure up to three times.
7. **Assess the risk.** Run `osf changeset risk --base <base>`. Keep the report for the pull request body.
8. **Open the stacked pull requests.** Follow the repository's `.github/PULL_REQUEST_TEMPLATE.md` and the `opening-a-pull-request` skill for the title, body and generated blocks. Do not change them. Each pull request links the work item, its parent pull request or base, its slice, and the verify, review and risk evidence. Open each slice with `osf forge pr open --head <branch> --base <parent> --title <title> --body <body> --work-item <item>`, in stack order. The first base is the repository's default branch. Each next base is its parent branch. If an adopter removed the template or skill, write the same fields directly.
9. **Report and complete.** Write the recap: what changed, the checks and reviews, their evidence, and the open pull requests. Run `osf work-tracker report <item> --state in-review --recap <recap file>`. Then run `osf run complete --run <run id> --outcome completed --summary "<one-line summary>"`. Stop at the open pull requests.
10. **Stop on a hard stop.** Stop when you cannot continue: an unmet dependency, an ambiguous requirement, or a git/forge failure that retries cannot fix. Write the recap and run `osf work-tracker report <item> --state blocked --cause <cause> --recap <recap file>`, with a cause from `dependency`, `human`, `clarification`, `ambiguous` or `capacity`. Then run `osf run complete --run <run id> --outcome blocked --summary "<one-line summary>"` and stop. When the job cannot produce a change at all, report the work item `failed` and complete the run with `failed`.

The osf commands append the journal events. Do not write the journal by hand. Do not merge a pull request. Do not start a run or a sandbox. Use the osf command for each fixed step. Reading the work item is the one read the caller may supply.
