Issue: [open-software-factory/software-factory#N (its title)](https://github.com/open-software-factory/software-factory/issues/N)
<!-- One line like this for each issue this pull request closes or advances. The reader should understand the pull request in 30 seconds without clicking anything. -->

## What and why

<!--
One sentence on what the pull request does, with the key change in bold. Lead with the change, not metrics.
Write "Rejects **unsigned tokens**", not "33 tests added".
Then one line that starts with **Why:** and gives the reason in a sentence.
Every part of this description must scan visually: bold key phrases, icons, a few words per line.
A reviewer with little context must see what to look at, why, and in what order.
-->

One sentence that says what changes, with the **key change in bold**.

**Why:** one sentence on the reason for the change.

<!--
Warning callout. Add it only when something could break or needs care, for example on deploy.
Leave it out otherwise, and never write "no warnings".
Start each paragraph with a bold lead-in, then say it in a few words. Leave a line with only > between paragraphs.

> [!WARNING]
> **Could break on deploy:** what could fail, and for whom.
>
> **Undo:** the fastest way back, in one sentence.

Other lead-ins: **Could break …:**, **Could change …:**, **Needs …:**, **Not planned …:**, **Undo …:**.
-->

**Start reading here**

1. `path/to/first-file`: a few words on why to read it first.
2. `path/to/second-file`: a few words on why to read it next.

<!-- Two to four files, in reading order, as a numbered list. Each line is the path in backticks, a colon, and a few words that say what to decide or check. Name and link every reference with its title, never a bare number. -->

<!-- osf:pr-lens:start head=<sha> -->
<!-- The pr-lens workflow writes the diagrams here. Do not edit them. The diagram that carries the main point of the change is open (details open). Secondary diagrams are collapsed. -->
<!-- osf:pr-lens:end -->

<!--
Status block, written by osf pr status. Do not edit it. The look is fixed by the tool:
- The heading is "### Status at" and the short commit hash in backticks.
- The table has the columns Check, Result and Details. Check names are bold. Details are a few words.
- Rows: Risk, Tests, CI, Commit messages, Contributor agreement, Automated review, Human review.
- Risk has a dot and the level: 🔴 High, 🟠 Medium, 🟢 Low.
- Results are one icon and a few words: ✅ passed, ⏳ waiting, ❌ failed, ⏸ not run, ➖ none automated. Never show ✅ for a check that did not run.
- The Tests row links to the Testing notes section below.
- Collapsed sections have bold titles: "Why the risk is" the level, "Automated review rounds", and "Test changes".
- Human review names people by display name, never by login.
Pass a one-line summary to the review rows with the flags on osf pr status render when the computed text is not enough.
-->
<!-- osf:status:start head=<sha> -->
<!-- osf:status:end -->

<!--
File table, collapsed, inside the osf:tree block below. Do not write it by hand. Run osf pr tree render with the base and head, save the output to a file, and write it with osf pr section write and the name tree.
What it looks like:
- The collapsed summary reads "All N files" in bold, then the group counts.
- Three groups, in this order: Code, Tests, Docs, build and infra. Each has a bold header line, such as **Code** (12 files, +446 −33).
- 15 files or fewer: one flat table with the columns File, Change, Added, Removed and Size. A path is a small directory line over the file name. The directory line is left out when it repeats the row above.
- More than 15 files: one row per component, biggest first, with the columns Component, Files, Added, Removed and Size.
- Count chips follow GitHub's diff colours: added text #aff5b4 on #033a16, removed text #ffdcd7 on #67060c. A chip is as wide as its text.
- Size bars are on a log scale, in #033a16 (added) and #67060c (removed).
- A small footnote gives A added, M modified, D deleted, R renamed, and says the size is a log scale of lines changed.
Maths allows only \color{#hex}{…}, \rule[…]{w}{h}, \rlap{…}, \hspace{…} and \texttt{…}.
Avoid \colorbox, \fcolorbox, the [RGB] and [HTML] colour models, and \, because they break in the browser or get stripped.
-->
<!-- osf:tree:start head=<sha> -->
<!-- The file table goes here, from osf pr tree render. -->
<!-- osf:tree:end -->

<!--
What changes at runtime, collapsed, inside the osf:outline block below. Add it only when the runtime flow changes.
Delete the whole block otherwise. The pr-outline skill writes it. Nothing generates it, so the author or agent writes it.
Write a short sentence on the one thing to notice. Then a call tree in a diff code block, as in this example:

<details>
<summary><b>What changes at runtime</b></summary>

Each request still validates once. The new check runs after the cache.

```diff
 Handler.Authenticate
   ReadToken                      # header, then cookie
   Validator.Validate
     cache.GetOrCreate            # 5 minutes
+    CheckSignature               # new: rejects unsigned tokens
-    AcceptAnyToken               # removed
+  Telemetry.Record               # only when a finding exists
   BuildTicket
```

</details>

Rules for the tree:
- A line that starts with + is a call this change adds. A line that starts with - is a call it removes. A line that starts with a space is unchanged context.
- Indent two spaces for each level of call. Keep the tree to about six to fifteen lines.
- End a line with a # and a few words when the name alone does not say what it does.
- Check every line against the real diff.
-->
<!-- osf:outline:start head=<sha> -->
<!-- Write the call tree here, in a collapsed details block with a bold summary. -->
<!-- osf:outline:end -->

## Testing notes

<!-- Say what the tests prove as a list of behaviours, one per line. Then say what is not covered. Then say what was run and where. Bold the key words. -->

<details open>
<summary><b>What the tests prove</b></summary>

- A behaviour the tests check, in a sentence.

</details>

**Not covered:** a behaviour no test checks.

**What was run:** the command, and the place it ran.

<!--
Optional author sections, such as impact tables, rollout plans, queries and migration notes.
Add one only when the pull request has that content. Never add an empty section or an "n/a" section.
Put each one in its own collapsed block with a bold summary that says what is inside:

<details>
<summary><b>One line saying what is inside</b></summary>

The content.

</details>

Every collapsed section in this description has a bold summary. Only the main diagram and the tests-prove list are open.
-->

<!--
Stack footer. On a stacked pull request, git-town writes a stack list at the very end of the description. Never edit it, and never add a stack row anywhere else.

Markdown hygiene:
- A table cell holds one line. It cannot hold a list or a paragraph. Every table has a real header row with no empty header cell.
- Inside a details block, leave a blank line after the summary and another before the closing tag.
- Write lists one item per line. Line up a nested item under the text of its parent: 2 spaces under a dash, 3 under a number.
- Render the description and look at it in a browser before you post it. Convert it with the GitHub markdown API and check the maths and the tables.
-->
