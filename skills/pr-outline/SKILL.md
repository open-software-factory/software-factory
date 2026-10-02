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
6. Save the file tree and the diff or pseudocode block to a file.
7. Run `osf pr section write --pr <number> --name outline --file <path>`. It replaces the text between the two markers, or adds them near the end when absent. Everything else in the description stays as it was.

Keep the whole outline short for a small pull request. A one-file change needs a one-line tree.

Stop when the outline is written and every claim in it has been checked against the real diff.

---

Inspired by the Change outline section of the visual-pr skill published by HumanLayer at github.com/humanlayer/skills (plugins/visual-pr/skills/visual-pr/SKILL.md), under the MIT licence. This skill does not copy its code. It writes a fresh implementation for the marked-section format the pr-lens workflow also writes into.
