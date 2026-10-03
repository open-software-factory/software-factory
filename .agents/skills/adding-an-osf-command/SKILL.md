---
name: adding-an-osf-command
description: Use this skill when you add a new osf check, a lint rule, a scan rule, or a command-line command.
---

1. State what the new rule covers and what it does not.
   - Put it in a Coverage section of the rule's doc text.
   - Follow the Coverage section every existing rule carries, for example in `crates/osf/src/scan/meta.rs`.
   - Never let a clean result read as "nothing found" when it means "nothing looked at".
2. Make the rule tell "could not read" apart from "read, and found nothing".
   - Hold the change when a test command is missing. Do not pass it.
3. Give the rule a class and a source.
   - Name the class: an external standard, a published measurement, or this project's own taste.
   - Cite the source. A rule with no real citation says plainly that it is taste.
4. Report a denylist match with only the file and the line. Never print the text that matched.
5. Read only the files a change touches by default.
   - Keep a whole-repository pass as a separate, occasional run.
   - Follow the `changed` and `all` scopes of the writing gate in `.github/workflows/ci.yml`.
6. Run the new check against this repository's own files before you trust it anywhere else.
   - Run `osf explain <rule-id>` to read its doc text back.
7. Make the check fail loudly.
   - Never hide a real failure behind a suppression flag.
   - Count a check as verifying something only when it can tell a pass from a failure.

Stop when the new rule carries a class, a citation, and a Coverage note. `cargo test --workspace` must also pass against this repository's own fixtures.
