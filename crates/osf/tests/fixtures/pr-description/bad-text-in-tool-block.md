Issue: [open-software-factory/software-factory#1 (a made-up issue)](https://github.com/open-software-factory/software-factory/issues/1)

## What and why

Rejects **unsigned tokens** at the gate.

**Why:** an unsigned token could pass the old check.

**Start reading here**

1. `src/gate.rs`: where the signature check goes.

<!-- osf:pr-lens:start head=0123abc -->
{PUFFERY} Read {LOCAL_PATH} and Zorblax.
<!-- osf:pr-lens:end -->

## Testing notes

<details open>
<summary><b>What the tests prove</b></summary>

- An unsigned token is rejected.

</details>

**Not covered:** expired tokens.

**What was run:** the unit tests.
