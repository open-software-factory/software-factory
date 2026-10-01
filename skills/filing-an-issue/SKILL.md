---
name: filing-an-issue
description: Use this skill when you file a new issue, or edit an issue's type, parent, status, or body.
---

1. Set an issue's type, parent, blocked-by, priority, and size in the tracker's own native fields. Do not restate them in the body text. Do not put an issue link there either. This rule is in decision 0019. That decision is not yet merged. It is carried in [open-software-factory/software-factory#136 (native issue fields)](https://github.com/open-software-factory/software-factory/pull/136).
   - Not checked: needs judgment.
2. Match the body to the item's type. A container type, an Epic, a Feature, or a Story, states a goal, why, and numbered acceptance criteria. A unit of execution, a Task or a Chore, is a checklist. A Bug states expected behaviour, actual behaviour, steps to reproduce, and environment. Same source, decision 0019.
   - Not checked: needs judgment.
3. Give a Blocked item a written reason every time. Say what is missing, and who can supply it. Same source, decision 0019.
   - Not checked: needs judgment. A tracker-side lint for this is designed. It is not built yet.
4. Read the created issue's number back from the response you get when you create it through an API. Do not find a just-created issue by searching its title.
   - Not checked: needs judgment.
5. When you edit an issue or a pull request body through a script, write the whole new body to a file first. Then send the file. Never build the new body from text an API already returned as a line array.
   - Not checked: needs judgment.

Stop when the issue's type, parent, and other fields are set as native fields. Its body must match its type's default shape, and any Blocked status must carry a reason.
