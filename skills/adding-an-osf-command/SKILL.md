---
name: adding-an-osf-command
description: Use this skill when you add a new osf check, a lint rule, a scan rule, or a command-line command.
---

1. State what the new rule covers and what it does not. A clean result must never read as "nothing found" when it means "nothing looked at".
   - Not checked: needs judgment. Follow the Coverage section every existing rule's doc text already carries, for example in `crates/osf/src/scan/meta.rs`.
2. Make the rule tell "could not read" apart from "read, and found nothing". A missing test command should hold the change rather than pass it. This rule comes from [decision 0003](../../docs/architecture/decisions/0003-deterministic-verification-is-authoritative.md).
   - Checked by: the writing check's stop hook already refuses a turn it could not check (`crates/osf/src/hook.rs`). It does not let that turn pass silently.
   - Not checked yet: a shrinking test count, or a suppression added in the same change, becoming a finding of its own. This is designed in decision 0018. That decision is not yet merged. It is carried in [open-software-factory/software-factory#136 (hook enforcement)](https://github.com/open-software-factory/software-factory/pull/136).
3. Give the rule a class and name its source: an external standard, a published measurement, or this project's own taste. A rule with no real citation says plainly that it is taste.
   - Checked by: a test that holds every rule id to carrying its own class and citation (`crates/osf/src/lints/writing/meta.rs`, `crates/osf/src/lints/skill.rs`).
4. Report a denylist match with only the file and the line. Never print the text that matched.
   - Checked by: `osf scan`. The `scan-denied-name` rule records no matched text at all (`crates/osf/src/scan/mod.rs`).
5. Read only the files a change touches by default. Keep a whole-repository pass as a separate, occasional run.
   - Not checked: needs judgment. `.github/workflows/ci.yml` already splits a `changed` scope from an `all` scope for the writing gate. Follow that shape.
6. Run the new check against this repository's own files before you trust it anywhere else.
   - Not checked: needs judgment.
7. Make the check fail loudly. Never hide a real failure behind a suppression flag. A check only counts as verifying something when it can tell a pass from a failure.
   - Not checked: needs judgment.

Stop when the new rule carries a class, a citation, and a Coverage note. `cargo test --workspace` must also pass against this repository's own fixtures.
