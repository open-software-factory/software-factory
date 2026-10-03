---
name: pr-outline
description: Use this skill when the user wants a change outline written into a pull request description, or after opening or updating a pull request that changes code.
---

Write the "What changes at runtime" section of the pull request description. The outline block in `.github/PULL_REQUEST_TEMPLATE.md` sets its format. Read that block first and follow it. Replace only that block. Leave the rest of the description untouched.

1. Find the pull request's real base branch. Run `gh pr view <number> --json baseRefName --jq .baseRefName`. Never assume it is `main`. A stacked pull request is based on another open pull request's branch.
2. Read the full diff against that base branch: `git diff <base>...<head>`.
3. Decide whether the runtime flow changes. If it does not, stop and write nothing.
4. Write the section as the template describes: a short sentence on what to notice, then a call tree in a `diff` code block. Mark each call the change adds with `+` and each call it removes with `-`. Do not draw a Mermaid diagram. The pr-lens workflow already draws one for a code change.
5. Check every claim in the section against the real diff. Remove or fix any claim the diff does not support.
6. Save the section from steps 4 and 5 to a file.
7. Run `osf pr section write --pr <number> --name outline --file <path>`. It replaces the block, or adds it near the end when absent. It writes the pull request's head commit into the start marker. Everything else in the description stays as it was.

Keep the whole section short for a small pull request.

Stop when the section is written and every claim in it has been checked against the real diff.
