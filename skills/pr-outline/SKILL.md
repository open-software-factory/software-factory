---
name: pr-outline
description: Use this skill when the user wants a change outline written into a pull request description, or after opening or updating a pull request that changes code.
---

Write a short outline of the change into the pull request description, between the markers `<!-- osf:outline:start -->` and `<!-- osf:outline:end -->`. Replace only the text between those markers. Leave the rest of the description untouched.

1. Find the pull request's real base branch. Run `gh pr view <number> --json baseRefName --jq .baseRefName`. Never assume it is `main`. A stacked pull request is based on another open pull request's branch.
2. Read the full diff against that base branch: `git diff <base>...<head>`.
3. Write a file tree of the changed paths, split into two groups: the files that carry logic, and the files that are fixtures or tests. Say which file to read first, and why.
4. Write one before-and-after diff, or one pseudocode block, for the key change. Use plain text or a `diff` block. Do not draw a Mermaid diagram. The pr-lens workflow already draws one for a code change.
5. Check every claim in the outline against the real diff. Remove or fix any claim the diff does not support.
6. Read the current description: `gh pr view <number> --json body --jq .body`. Replace the text between the two markers with the outline from steps 3 and 4. Insert the markers near the end when they are not already present. Leave everything else in the description as it was.
7. Save the updated description to a file, then run `gh pr edit <number> --body-file <path>`.

Keep the whole outline short for a small pull request. A one-file change needs a one-line tree.

Stop when the outline is written and every claim in it has been checked against the real diff.

---

Inspired by the Change outline section of humanlayer/skills' visual-pr skill (MIT). This skill does not copy its code. It writes a fresh implementation for the marked-section format the pr-lens workflow also writes into.
