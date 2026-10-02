Issue: [open-software-factory/software-factory#N (its title)](https://github.com/open-software-factory/software-factory/issues/N)
<!-- One line like this for each issue this pull request closes or advances. The reader should understand the pull request in 30 seconds without clicking anything. -->

## What and why

<!-- One or two sentences on what the pull request does. Lead with sentences, not metrics. Write "Rejects unsigned tokens", not "33 tests added". -->

**Why:** one sentence on the reason for the change.

<!--
Warning callout. Add it only when something could break or needs care, for example on deploy.
Say what could break and how to undo it. Leave it out otherwise, and never write "no warnings".

> [!WARNING]
> What could break, and how to undo it.
-->

**Start reading here:**

1. `path/to/first-file`: a few words on why to read it first.
2. `path/to/second-file`: a few words on why to read it next.

<!-- Two to four files, in reading order. Name and link every reference with its title, never a bare number. Say what the reviewer needs to decide or check. -->

<!-- osf:pr-lens:start head=<sha> -->
<!-- The pr-lens workflow writes the diagrams here. Do not edit them. Diagrams that carry the main point of the change are open (details open). Secondary diagrams are collapsed. -->
<!-- osf:pr-lens:end -->

<!--
Status block, written by osf pr status. Do not edit it. It is headed "Status at" and the short commit hash.
The table has the columns Check, Result and Details, and one short line per row.
Rows: Risk, Tests, CI, Commit messages, Contributor agreement, Automated review, Human review.
A collapsed "Why the risk is" section follows. It has two to four bullets written for this change only.
Results use one icon and a few words: ✅ passed, ⏳ waiting, ❌ failed, ⏸ not run, with the reason.
Never show ✅ for a check that did not run.
CI covers build, test, clippy and format. Commit messages covers self-contained commits.
Contributor agreement covers a first-time author who is not a bot.
Automated review says which rounds ran, which model family, and what was fixed in which commit.
Human review names people by display name, never by login.
The Tests row links to Testing notes.
-->
<!-- osf:status:start head=<sha> -->
<!-- osf:status:end -->

<!--
File table, collapsed, inside the osf:tree block below. Replace the guidance comment inside the block with this:

<details>
<summary>Files changed</summary>

| File | Change | Added | Removed | Size |
|---|---|---|---|---|
| **Code** | | | | |
| `path/to/file` | M | 12 | 3 | 15 |
| **Tests** | | | | |
| **Docs, build and infra** | | | | |

Change is A added, M modified, D deleted, R renamed. Size is the lines added plus removed.

</details>

15 files or fewer: one flat table with columns File | Change | Added | Removed | Size, split into the three groups above.
With more, write one row per component instead, with the columns Component, Files, Added, Removed and Size, in the same three groups.
Sort components biggest first. Do not list files. Say in the footnote that the table is grouped, and why.
Count chips follow GitHub's diff colours: added text #aff5b4 on #033a16, removed text #ffdcd7 on #67060c.
Size bars are on a log scale, in #033a16 (added) and #67060c (removed).
Maths allows only \color{#hex}{…}, \rule[…]{w}{h}, \rlap{…} and \hspace{…}.
Avoid \colorbox, \fcolorbox, the [RGB] and [HTML] colour models, and \, because they break in the browser or get stripped.
-->
<!-- osf:tree:start head=<sha> -->
<!-- Write the file table here, collapsed. -->
<!-- osf:tree:end -->

<!--
What changes at runtime, collapsed, inside the osf:outline block below. Add it only when the runtime flow changes.
Delete the whole block otherwise. The pr-outline skill writes it as a call tree of the change.
-->
<!-- osf:outline:start head=<sha> -->
<!-- Write the call tree here, in a collapsed details block. -->
<!-- osf:outline:end -->

## Testing notes

<!-- Say what the tests prove as a list of behaviours, one per line. Then say what is not covered. Then say what was run and where. -->

<details open>
<summary>What the tests prove</summary>

- A behaviour the tests check, in a sentence.

</details>

**Not covered**

- A behaviour no test checks.

**What was run, and where**

- The command, and the place it ran.

<!--
Optional author sections, such as impact tables, rollout plans, queries and migration notes.
Add one only when the pull request has that content. Never add an empty section or an "n/a" section.
Put each one in its own collapsed block with a one-line summary of what is inside:

<details>
<summary>One line saying what is inside</summary>

The content.

</details>
-->

<!--
Stack footer. On a stacked pull request, git-town writes a stack list at the very end of the description. Never edit it, and never add a stack row anywhere else.

Markdown hygiene:
- A table cell holds one line. It cannot hold a list or a paragraph. Every table has a real header row with no empty header cell.
- Inside a details block, leave a blank line after the summary and another before the closing tag.
- Write lists one item per line. Line up a nested item under the text of its parent: 2 spaces under a dash, 3 under a number.
- Render the description and look at it in a browser before you post it.
-->
