# Rules for `.github/workflows/`

- Define any job that reads a secret, or that can decide a merge, in a workflow file on the base branch. A pull request may change its own build and test steps. It must never change the job that judges it. This rule is in decision 0020. That decision is not yet merged. It is carried in [open-software-factory/software-factory#136 (review trust)](https://github.com/open-software-factory/software-factory/pull/136). `.github/workflows/status-block.yml` already follows it. It builds from the default branch and only reads a pull request's tree as data.
  - Not checked: needs judgment. No lint yet reads a workflow file for this pattern.
- Never check out a pull request's own code into a job that also holds a secret. Fetch it, if at all, into a separate worktree that nothing builds or runs. `status-block.yml` shows the pattern.
  - Not checked: needs judgment.
- Pin every action to a full commit SHA. Do not pin it to a tag or a branch name instead. Every `uses:` line in this repository's own workflows already does this.
  - Not checked: needs judgment.
- Grant each job only the permissions it needs, starting from `contents: read`. `ci.yml` and `status-block.yml` both narrow their own permissions from there.
  - Not checked: needs judgment.
- Start a job that holds a secret with `step-security/harden-runner` in block mode, and give it an allow-list of only the hosts that job needs. Write the allow-list as literal hosts. harden-runner applies it before any later step runs, so it cannot read a later step's output. `pr-lens.yml` already does this, pinned to a commit SHA, with `egress-policy: block` and a literal `allowed-endpoints` list.
  - Not checked: needs judgment.
- Let a review aid, such as the status block or a pull request's change outline, post information only. It must never fail a check or block a merge on its own.
  - Not checked: needs judgment.
