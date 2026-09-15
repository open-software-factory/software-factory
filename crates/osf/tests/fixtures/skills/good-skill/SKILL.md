---
name: good-skill
description: Use this skill when the user wants a quick health check of a project folder.
---

Run this skill to check a folder for basic problems before it ships.

## Checks

1. Read the folder listing and note any file over ten megabytes.
2. Check that a license file exists.
3. Check that a readme file exists.
4. Write one line per problem found, with the file path.

Stop when every check has run once, whether or not it found a problem.

See [the notes file](references/notes.md) for the full list of checks.
