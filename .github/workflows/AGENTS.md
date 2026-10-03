# Rules for `.github/workflows/`

- Define any job that reads a secret, or that can decide a merge, in a workflow file on the base branch. A pull request may change its own build and test steps. It must never change the job that judges it. Copy `.github/workflows/status-block.yml`: it builds from the default branch and reads a pull request's tree only as data.
- Never check out a pull request's own code into a job that also holds a secret. If you must fetch it, fetch it into a separate worktree that nothing builds or runs. `status-block.yml` shows the pattern.
- Pin every action to a full commit SHA. Never pin it to a tag or a branch name. After you edit a workflow, check that every `uses:` line carries a full SHA.
- Grant each job only the permissions it needs, and none when it needs none. Start from `contents: read`, as `ci.yml` does, and add only what the job writes.
- Start a job that holds a secret with `step-security/harden-runner` in block mode, pinned to a commit SHA. Give it an allow-list of only the hosts that job needs, written as literal hosts. Copy `pr-lens.yml`, which sets `egress-policy: block` and a literal `allowed-endpoints` list.
- Let a review aid, such as the status block or a pull request's change outline, post information only. Never let it fail a check or block a merge on its own.
