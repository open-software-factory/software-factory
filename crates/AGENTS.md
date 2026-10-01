# Rules for `crates/`

## Code layout

- The `osf` binary is a thin command-line shell. A command there parses its arguments, calls a library crate, and prints the result. It holds no logic of its own.
  - Not checked: needs judgment.
- The logic lives in library crates under `crates/`, one crate per logical group, the way `osf-lint-core` holds the lint rules. One crate per command is too fine.
  - Not checked: needs judgment.
- A functional group gets its own crate. The review check goes in `osf-review`. Pull request work, such as the status block and description sections, goes in `osf-pr`. The checkpoint runner goes in `osf-checkpoint`.
  - Not checked: needs judgment.
- A provider gets its own crate, named after the provider and matching its command group. For example, `osf github …` lives in `osf-github`. Another code host, tracker or coding-agent harness gets a crate of its own when it is added.
  - Not checked: needs judgment.
- A functional crate stays free of any one provider. It reaches a provider through an interface.
  - Not checked: needs judgment.
- A new tool goes into the crate for its group. Start a new crate only when no group fits.
  - Not checked: needs judgment.

## Language

- Write the engine, its command-line interface, and the verifier runner in Rust. See [decision 0001](../docs/architecture/decisions/0001-rust-for-the-factory-engine.md). A provider at the factory's edges may be written in any language and reached out of process.
  - Not checked: needs judgment.

## Tests and checks

Before you push a change under `crates/`, run these from the repository root, and make all four pass:

```
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all --check
```

A warning from `cargo clippy` fails the build on purpose. `.github/workflows/ci.yml` is the one source of truth for what a pull request must pass. Read it to reproduce any step by hand.
