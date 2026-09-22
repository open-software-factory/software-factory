# 0015: Suppressions carry a reason and an expiry

Status: accepted

Date: 2026-09-23

## Context

An adopter needs a way to silence one finding without turning off the check. [Decision 0003](0003-deterministic-verification-is-authoritative.md) already says a suppression added in a change is itself a finding for review. It does not say what a suppression looks like, or what happens to the suppressions a team already has when it adopts the factory. The design is [the verification seam](../verification-seam.md).

## Options considered

| Option | What it meant | Outcome |
|---|---|---|
| A factory marker in the file's own comment syntax | One line above the finding, naming the rule, the expiry and the reason. The same fields as an entry in the configuration file, so one parser reads both. | Taken. It is the only form the factory's own checks have. |
| Native markers only | Each ecosystem's own form, such as a Rust allow attribute or a Python noqa comment, read by the factory. | Taken alongside the factory marker. Set aside as the only form, because no native form carries a reason or an expiry and the factory's own checks have none to borrow. |
| The configuration file only, with a path and a line | The source stays free of markers. | Set aside. Line numbers move as the file changes, and a reviewer reading the code cannot see that a finding was silenced. |

## Decision

A suppression is either a factory marker on the line above the finding, or an entry in `osf.toml`. Both carry the rule, a reason and an expiry date. A marker without a reason or an expiry is itself a finding. An expired marker is a finding.

```rust
// osf:suppress clippy::too_many_arguments until=2027-03-31 reason="mirrors the wire format"
```

A suppression the ecosystem's own tool understands keeps working for that tool, and the factory reads it. A native suppression a team already has at adoption stays in force and is counted. A native suppression that a change adds is a finding for review, as decision 0003 requires.

The journal counts every suppression, so the aggregation summary can show how many findings a repository silences.

## Consequences

- An adopter's existing suppressions are respected on day one.
- The suppression reader is one component with a corpus of markers in every comment syntax the supported ecosystems use.
- A finding's identity is a hash of rule, path, line and column, per [decision 0005](0005-the-factory-domain-model.md), so a marker binds to the finding it sits above and survives edits elsewhere in the file.
