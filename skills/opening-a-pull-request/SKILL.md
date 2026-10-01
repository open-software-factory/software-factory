---
name: opening-a-pull-request
description: Use this skill when you open a pull request, or update its description.
---

1. Open a pull request ready for review. Say so in the text when work continues on the branch.
   - Not checked yet: issue 194. A continuous-integration check for this is planned in [open-software-factory/software-factory#194 (rules layout)](https://github.com/open-software-factory/software-factory/issues/194), as a second pull request.
2. Give one distinct concern its own issue, its own branch, and its own pull request. Do not bundle unrelated changes into one.
   - Not checked: needs judgment.
3. Name the issue a pull request closes as `owner/repo#N`, with a short label saying what it is.
   - Not checked: needs judgment. `.github/PULL_REQUEST_TEMPLATE.md` already asks for this.
4. Close an issue with the merge commit's hash and the pull request's link, in the closing comment.
   - Not checked: needs judgment.
5. Carry a diagram of what changed, and a short outline of where to start reading, in the pull request description. Refresh both on every push, and check every claim in the outline against the real diff first.
   - Not checked: needs judgment. See `skills/pr-outline/SKILL.md` and `skills/show-me/SKILL.md` for how to write each part.
6. Do git work for this change in its own worktree when a person also works in the same repository.
   - Not checked: needs judgment.
7. Start a new stack with `git town hack`, and add to the stack you are on with `git town append`. Record a missing parent once with `git town set-parent`. After a change lands lower in a stack, run `git town sync`. `git-town.toml` syncs by merge, so a pushed branch is never rewritten.
   - Not checked: needs judgment.
8. Carry one attribution trailer naming the model that wrote a change. Never add a `Co-Authored-By` line, the git trailer that credits a second author. Never add a link to a coding-agent session.
   - Checked by: `osf scan`, for a session link, in every commit message a pull request adds (`crates/osf/src/scan/mod.rs`). The hygiene job in `.github/workflows/ci.yml` runs it on each pull request.
   - Not checked yet: issue 194, for carrying `Code-Generator` and for never carrying `Co-Authored-By`. A commit-msg hook for this is planned in [open-software-factory/software-factory#194 (rules layout)](https://github.com/open-software-factory/software-factory/issues/194), as a second pull request.
9. Never force-push a branch you did not create.
   - Not checked yet: issue 194. A harness pre-tool hook for this is planned in [open-software-factory/software-factory#194 (rules layout)](https://github.com/open-software-factory/software-factory/issues/194), as a second pull request.
10. Let a reviewer from a model family different from the builder's review a pull request. Its own author cannot approve it. This rule is accepted, in decision 0008. The fuller roster design is decision 0016. That decision is not yet merged. It is carried in [open-software-factory/software-factory#136 (review lenses)](https://github.com/open-software-factory/software-factory/pull/136).
    - Not checked: needs judgment. The roster and the two-family quorum in decision 0016 are designed. They are not built yet.

Stop when the pull request is ready and names the issue it closes with a label. Its outline's claims must match the real diff, and a reviewer from a model family different from the builder's has reviewed it.
