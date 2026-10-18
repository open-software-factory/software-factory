Issue: [open-software-factory/software-factory#1 (a made-up issue)](https://github.com/open-software-factory/software-factory/issues/1)

## What and why

Rejects **unsigned tokens** at the gate.

**Why:** an unsigned token could pass the old check.

**Start reading here**

1. `src/gate.rs`: read {LOCAL_PATH} first.

<!-- osf:pr-lens:start head=0123abc -->
A diagram block written by a tool.
<!-- osf:pr-lens:end -->

## Testing notes

<details open>
<summary><b>What the tests prove</b></summary>

- An unsigned token is rejected.

</details>

**Not covered:** expired tokens.

**What was run:** the unit tests.
