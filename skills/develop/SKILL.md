---
name: develop
description: Use this skill when a person or a platform job starts one develop job over one 'Ready' work item, from the item to open pull requests, unattended.
---

One job, one work item. The person or platform job that started it supplied the sandbox and the repository. The builder agent does the work, and osf records it. Stop at open pull requests.

The job uses these commands:

- `osf work-tracker ready` reads the 'Ready' item.
- `osf run start` and `osf run complete` open and close the journal run.
- `osf work-tracker report` writes the state and the recap.
- `osf forge pr open` opens a pull request.
- `osf verify`, `osf review run` and `osf changeset risk` run the checks.

1. Pick one item. Run `osf work-tracker ready --limit 1 --format json`. Stop when no item is ready. Do not pick a second item. The item id is `provider:repo#number`.
2. Read the item, the repository's `AGENTS.md`, and the files it names. Decide the one coherent change the item asks for.
3. Start the run. Run `osf run start --run <run id> --work-item <item> --format json`. Keep the head hash. Use the same run id for every later journal command.
4. Implement in the sandbox. Change only the files the item needs. Do not commit, push, stash or check out another branch.
5. Verify. Run `osf verify --checkpoint pull-request --base <base>`. A check that could not run is a failure to run, never a pass. Fix a failing check, or record why you cannot. A failing check does not stop the job.
6. Review in the sandbox. Run `osf review run --if-enabled --base <base> --out <file>` for early feedback, up to the configured round limit. The review check on the pull request is the required gate.
7. Risk. Run `osf changeset risk --base <base>`. Write the risk section the pull request carries.
8. Open the pull request. Run `osf forge pr open --head <branch> --base <base> --title <title> --body <body file> --work-item <item>`. Add `--repo <owner/name>` when the origin remote is not the target. One pull request per coherent change. Do not merge it.
9. Report. Run `osf work-tracker report <item> --state in-review --recap <recap file>`. Then run `osf run complete --run <run id> --outcome completed --summary <recap file>`. Stop.
10. Block or ask. Run `osf work-tracker report <item> --state blocked --cause <cause> --recap <recap file>`. Then run `osf run complete --run <run id> --outcome blocked --summary <recap file>`. Stop. The causes are `dependency`, `human`, `clarification`, `ambiguous` and `capacity`.

A step that fails runs `osf run complete --run <run id> --outcome failed --summary <recap file>` and stops.

The commands append the journal events. Do not write the journal by hand.

Do not merge. Do not start a second job. Do not replace an osf command with a direct shell command, a direct code-host command, or a direct container command.
