---
name: no-trigger
description: Checks a folder for common problems before a release.
---

Run this skill to check a folder for basic problems before it ships.

1. Read the folder listing and note any file over ten megabytes.
2. Check that a license file exists.
3. Check that a readme file exists.
4. Write one line per problem found, with the file path.

Stop when every check has run once, whether or not it found a problem.

<!-- osf-expect-skill
skill-description-no-trigger
-->
