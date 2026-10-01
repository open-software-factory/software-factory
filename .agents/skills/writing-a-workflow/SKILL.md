---
name: writing-a-workflow
description: Use this skill when you add or edit a GitHub Actions workflow file, or when a job needs to read a secret.
---

1. Define a workflow whose result can decide a merge on the base branch. A pull request may change its own build and test steps. It must never change the job that judges it, and that job must never check out or run the pull request's own code. This rule is in decision 0020. That decision is not yet merged. It is carried in [open-software-factory/software-factory#136 (review trust)](https://github.com/open-software-factory/software-factory/pull/136).
   - Not checked: needs judgment. `.github/workflows/status-block.yml` already follows this. It builds from the default branch and reads a pull request's tree only as data.
2. Give a job a secret only as a short-lived, narrowly scoped token minted for that job. Keep a long-lived key out of a builder agent's own workspace. Same source as rule 1, decision 0020.
   - Not checked: needs judgment.
3. Let the runner, and any reviewer-tool credential, come from the adopter's own settings, a repository variable or a repository secret. The workflow itself never reads them directly. Same source, decision 0020.
   - Not checked: needs judgment.
4. Ask for a person's written approval, with a reason, before merging a change that makes a check catch fewer real problems. This rule is in decision 0018. That decision is not yet merged. It is carried in [open-software-factory/software-factory#136 (hook enforcement)](https://github.com/open-software-factory/software-factory/pull/136).
   - Not checked: needs judgment. The weakening check that would enforce this is designed. It is not built yet.
5. Run every job inside the code host's own workflow mechanism, on whatever runner the adopter sets. Name no fixed runner label. Assume no always-on process. Same source as rule 1, decision 0020.
   - Not checked: needs judgment.

Stop when every job that can decide a merge is defined on the base branch. Every secret must reach only the job that needs it, and the workflow must name no fixed runner.
