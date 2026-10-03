---
name: opening-a-pull-request
description: Use this skill when you open a pull request, or update its description.
---

1. Open every pull request ready for review. Do not open a draft. Say in the description when work continues on the branch.
2. Give one distinct concern its own issue, its own branch, and its own pull request. Do not bundle unrelated changes into one.
3. Name the issue a pull request closes as `owner/repo#N`, with a short label saying what it is.
4. Close an issue with the merge commit's hash and the pull request's link, in the closing comment.
5. Carry a diagram and an outline in the pull request description.
   - Carry a diagram of what changed.
   - Carry a short outline of where to start reading.
   - Refresh both on every push, and check every claim in the outline against the real diff first.
   - See the pr-outline skill and the show-me skill for how to write each part.
6. Do git work for this change in its own worktree when a person also works in the same repository.
7. Stack branches with `git town`.
   - Start a new stack with `git town hack`.
   - Add to the stack you are on with `git town append`.
   - Record a missing parent once with `git town set-parent`.
   - After a change lands lower in a stack, run `git town sync`. `git-town.toml` syncs by merge, so a pushed branch is never rewritten.
8. Carry one attribution trailer in each commit.
   - Name the model that wrote the change: `Code-Generator: <model> <noreply@example.com>`.
   - Never add a `Co-Authored-By` line, the git trailer that credits a second author.
   - Never add a link to a coding-agent session.
9. Before you push, run `osf scan --config osf.toml --commits origin/main..HEAD`. Replace `origin/main` with the pull request's base branch. Fix every error, and rewrite any commit message it flags.
10. Never force-push a branch you did not create.
11. Have a reviewer from a model family different from the builder's review the pull request. Its own author cannot approve it.

Stop when the pull request is ready and names the issue it closes with a label. Its outline's claims must match the real diff, and a reviewer from a model family different from the builder's has reviewed it.
