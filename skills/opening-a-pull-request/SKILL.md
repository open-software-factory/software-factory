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

Stop when the pull request is ready and names the issue it closes with a label. Its outline's claims must match the real diff.
