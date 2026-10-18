---
name: run
description: Use this skill when an agent performs one unattended run over one ready work item, from ready to in review, with no person in between.
---

Hold the loop for one work item. The `osf` commands perform each step and write the journal. Do not write a second loop in code, and do not write the journal by hand.

1. Pick one ready item. Run `osf tracker ready --limit 1 --format json`. Stop when no item is ready.
2. Start the run. Run `osf run start --work-item <item> --format json`. Keep the run id. The command records the work item, the actor, and the run start.
3. Implement in a sandbox. Run `osf sandbox run --image <pinned-image> --repo <repo> --state <state> --network open -- osf harness run --agent <builder> --task <task-file> --out <harness.json>`. The configured builder writes the change. The harness result names the session, the branch, and the head commit. A question from the builder ends the run as blocked with the cause `clarification`.
4. Verify. Run `osf verify --checkpoint pull-request --base <base>`. A check that could not run is a failure to run, never a pass. Re-run a failed check from the same inputs before deciding.
5. Review. Run `osf review run --if-enabled --base <base> --out <review.json>`. The reviewer must be from a family different from the builder's. A review that could not run never approves.
6. Decide. Run `osf policy decide --run <run> --format json`. The policy reads only the events osf recorded. The builder's own account is reported evidence, not an input. If the verdict is hold or attention, go to the block or hand-over step.
7. Report. Run `osf forge pr open --work-item <item> --branch <branch> --base <base> --body <body-file>` to open the pull request. Run `osf pr status refresh --pr <number>` to write the status block. Run `osf tracker report --work-item <item> --state in-review --recap <recap-file>`. Run `osf run complete --run <run> --outcome completed --summary "<summary>"`. Stop.
8. Block or hand over. Run `osf tracker report --work-item <item> --state blocked --cause <cause> --recap <recap-file>`. Run `osf attention raise --work-item <item> --cause <cause>`. Run `osf run complete --run <run> --outcome blocked --summary "<recap>"`. Stop.

The causes are `dependency`, `human`, `clarification`, `ambiguous`, and `capacity`.

A step that fails writes its cause and its recap to the work item. It runs `osf run complete --run <run> --outcome failed --summary "<recap>"`, and stops. A missing or failing `osf` command stops the run. Do not replace it with a direct shell command, a direct code-host command, or a direct container command.

The run is one item. Do not pick a second item, and do not merge. Stop when the pull request is open with its checks, its review, and the run recorded. Stop when the item is blocked and the cause and recap are on the work item.
